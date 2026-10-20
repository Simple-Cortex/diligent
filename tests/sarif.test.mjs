/**
 * Tests for the SARIF 2.1.0 emitter.
 *
 * Covers: minimal run, empty findings, research lifecycle, non-ASCII/special
 * characters, determinism, CLI exit codes, structural validation, result-derived
 * entries, and schema validation (option 3: test-only walk to strip uri-reference).
 */
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';
import { convertToSarif, validateSarif, serializeSarif } from '../scripts/lib/sarif.mjs';
import { validate, checkSchema } from '../scripts/lib/schema.mjs';

const ROOT = resolve(fileURLToPath(import.meta.url), '..', '..');
const FIXTURES = join(ROOT, 'fixtures', 'sarif');

function fixture(name) { return join(FIXTURES, name); }

/**
 * Load the vendored SARIF schema and strip unsupported keywords for validation.
 * Option 3 from the return: test-only walk to delete format:uri-reference and
 * any other unsupported keywords, then validate.
 */
async function loadSarifSchemaForValidation() {
  const schemaPath = join(ROOT, 'schemas', 'vendor', 'sarif-schema-2.1.0.json');
  const schema = JSON.parse(await readFile(schemaPath, 'utf8'));

  // Deep walk and delete unsupported keywords
  function walk(obj) {
    if (!obj || typeof obj !== 'object') return;
    if (Array.isArray(obj)) {
      obj.forEach(walk);
      return;
    }
    for (const key of Object.keys(obj)) {
      if (key === 'format' && obj[key] === 'uri-reference') {
        delete obj[key];
      } else if (key === 'patternProperties' || key === 'minProperties' || key === 'maxProperties') {
        delete obj[key];
      } else {
        walk(obj[key]);
      }
    }
  }
  walk(schema);
  return schema;
}

describe('SARIF emitter', () => {
  it('produces valid SARIF from a minimal run with one finding', async () => {
    const { ok, sarif, errors } = await convertToSarif(fixture('run-minimal'));
    assert.ok(ok, `conversion failed: ${errors.join(', ')}`);
    assert.equal(sarif.version, '2.1.0');
    assert.ok(sarif.$schema.includes('sarif-schema-2.1.0'));
    assert.equal(sarif.runs.length, 1);

    const run = sarif.runs[0];
    assert.equal(run.tool.driver.name, 'Diligent');
    assert.equal(run.tool.driver.informationUri, 'https://diligent.style');
    assert.ok(run.tool.driver.semanticVersion);
    // Now emits both finding-derived and result-derived entries
    assert.ok(run.results.length >= 1, 'should have at least 1 result');

    // Find the finding-derived result (has finding_id property)
    const findingResult = run.results.find(r => r.properties.finding_id === 'find-001');
    assert.ok(findingResult, 'should have finding-derived result');
    assert.equal(findingResult.ruleId, 'INFRA-12');
    assert.equal(findingResult.level, 'note');
    assert.equal(findingResult.message.text, 'strict-transport-security header is absent.');
    assert.equal(findingResult.partialFingerprints['diligent/fingerprint/v1'], 'fp-aaa-111');
    assert.equal(findingResult.properties.finding_id, 'find-001');
    assert.equal(findingResult.properties.lifecycle, 'research');
    assert.equal(findingResult.properties.diligent_note,
      'This rule is at the research lifecycle: the finding is a lead for human review, not a compliance determination.');
  });

  it('produces valid SARIF with empty results for zero findings', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-empty'));
    assert.ok(ok);
    assert.equal(sarif.runs[0].results.length, 0);
    assert.equal(sarif.runs[0].tool.driver.rules.length, 0);
    const v = validateSarif(sarif);
    assert.ok(v.ok, `validation failed: ${v.errors.join(', ')}`);
  });

  it('marks research-lifecycle findings as level note with diligent_note', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-research'));
    assert.ok(ok);
    const result = sarif.runs[0].results[0];
    assert.equal(result.level, 'note');
    assert.equal(result.properties.diligent_note,
      'This rule is at the research lifecycle: the finding is a lead for human review, not a compliance determination.');
    assert.equal(result.properties.lifecycle, 'research');
  });

  it('counts review queue items in invocations', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-research'));
    assert.ok(ok);
    const inv = sarif.runs[0].invocations[0];
    assert.equal(inv.properties.review_queue_count, 1);
    assert.equal(inv.properties.run_id, 'run-2026-10-18-ccdd02');
    assert.equal(inv.executionSuccessful, true);
  });

  it('handles non-ASCII observations and selectors with quotes', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-special'));
    assert.ok(ok);
    const result = sarif.runs[0].results[0];
    assert.ok(result.message.text.includes('é'));
    assert.ok(result.message.text.includes('ü'));
    const selector = result.locations[0].logicalLocations[0].fullyQualifiedName;
    assert.ok(selector.includes('"'));
  });

  it('produces deterministic output (byte-identical on repeat)', async () => {
    const { sarif: a } = await convertToSarif(fixture('run-minimal'));
    const { sarif: b } = await convertToSarif(fixture('run-minimal'));
    assert.equal(serializeSarif(a), serializeSarif(b));
  });

  it('sorts results by (ruleId, fingerprint, finding_id or result_id)', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-minimal'));
    assert.ok(ok);
    const ids = sarif.runs[0].results.map(r => r.ruleId);
    const sorted = [...ids].sort();
    assert.deepEqual(ids, sorted);
  });

  it('sorts rules by id', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-minimal'));
    assert.ok(ok);
    const ids = sarif.runs[0].tool.driver.rules.map(r => r.id);
    const sorted = [...ids].sort();
    assert.deepEqual(ids, sorted);
  });

  it('validates structurally via validateSarif', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-minimal'));
    assert.ok(ok);
    const v = validateSarif(sarif);
    assert.ok(v.ok, `validation errors: ${v.errors.join(', ')}`);
  });

  it('rejects a SARIF document with wrong version', () => {
    const v = validateSarif({ version: '1.0', runs: [] });
    assert.ok(!v.ok);
    assert.ok(v.errors.some(e => e.includes('version')));
  });

  it('rejects a SARIF document with no runs', () => {
    const v = validateSarif({ version: '2.1.0', $schema: 'sarif-schema-2.1.0', runs: [] });
    assert.ok(!v.ok);
  });

  it('returns errors for a missing run directory', async () => {
    const { ok, errors } = await convertToSarif('/nonexistent/path');
    assert.ok(!ok);
    assert.ok(errors.length > 0);
  });

  it('clips observations longer than 1000 characters', async () => {
    const longObs = 'x'.repeat(2000);
    const { ok, sarif } = await convertToSarif(fixture('run-minimal'));
    assert.ok(ok);
    const maxLen = Math.max(...sarif.runs[0].results.map(r => r.message.text.length));
    assert.ok(maxLen <= 1000);
  });

  it('serializes with sorted keys', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-minimal'));
    assert.ok(ok);
    const json = serializeSarif(sarif);
    const parsed = JSON.parse(json);
    const topKeys = Object.keys(parsed);
    assert.deepEqual(topKeys, [...topKeys].sort());
  });

  it('emits result-derived entries from RESULTS.jsonl', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-results-only'));
    assert.ok(ok);
    const results = sarif.runs[0].results;
    assert.ok(results.length >= 3, 'should have at least 3 result-derived entries (fail, needs_review, unmapped)');

    const failResult = results.find(r => r.properties.status === 'fail' && r.properties.check_id === 'INFRA-02-hsts');
    assert.ok(failResult, 'should have a fail result for INFRA-02-hsts');
    assert.ok(failResult.partialFingerprints['diligent/result/v1'], 'should have result fingerprint');
    assert.equal(failResult.properties.check_id, 'INFRA-02-hsts');
    assert.equal(failResult.properties.result_id, 'res-r01');
    assert.equal(failResult.properties.instance_id, 'https://agency.example.gov/');

    const needsReviewResult = results.find(r => r.properties.status === 'needs_review');
    assert.ok(needsReviewResult, 'should have a needs_review result');
    assert.equal(needsReviewResult.level, 'note');
    assert.equal(needsReviewResult.properties.needs_review, true);

    const unmappedResult = results.find(r => r.properties.check_id === 'BRW-TEXT-CONTRAST');
    assert.ok(unmappedResult, 'should have an unmapped result using check_id as rule_id');
    assert.equal(unmappedResult.ruleId, 'BRW-TEXT-CONTRAST');
  });

  it('does not emit pass/not_tested/blocked/error results', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-results-only'));
    assert.ok(ok);
    const results = sarif.runs[0].results;
    const passResult = results.find(r => r.properties.status === 'pass');
    assert.ok(!passResult, 'should not emit pass results');
  });

  it('includes invocation properties for result counts', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-results-only'));
    assert.ok(ok);
    const inv = sarif.runs[0].invocations[0];
    assert.ok(inv.properties.results_total > 0, 'should have results_total');
    assert.ok(inv.properties.results_emitted > 0, 'should have results_emitted');
    assert.ok(inv.properties.results_by_status, 'should have results_by_status');
    assert.ok(inv.properties.results_by_status.fail > 0, 'should count fail results');
    assert.ok(inv.properties.results_by_status.pass > 0, 'should count pass results');
  });

  it('uses rule obligation text for descriptions', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-results-only'));
    assert.ok(ok);
    const infra02Rule = sarif.runs[0].tool.driver.rules.find(r => r.id === 'INFRA-02');
    assert.ok(infra02Rule, 'should have INFRA-02 rule');
    assert.ok(infra02Rule.shortDescription.text.length > 10, 'description should be from obligation text, not just rule_id');
    assert.ok(!infra02Rule.shortDescription.text.startsWith('strict-transport-security'), 'should not be finding observation');
  });

  it('locates static source rows by file and line', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-source'));
    assert.ok(ok);
    const results = sarif.runs[0].results;

    const withLine = results.find(r => r.properties.result_id === 'res-s01');
    assert.ok(withLine, 'source fail row is emitted');
    const loc = withLine.locations[0];
    assert.equal(loc.physicalLocation.artifactLocation.uri, 'src/Card.tsx');
    assert.deepEqual(loc.physicalLocation.region, { startLine: 8 });
    assert.equal(loc.logicalLocations[0].fullyQualifiedName, 'html > body > img:nth-of-type(2)');
    assert.equal(withLine.properties.instance_id, 'src/Card.tsx::html > body > img:nth-of-type(2)');

    const noLine = results.find(r => r.properties.result_id === 'res-s02');
    assert.ok(noLine, 'source needs_review row is emitted');
    assert.equal(noLine.locations[0].physicalLocation.artifactLocation.uri, 'src/Card.tsx');
    assert.equal(noLine.locations[0].physicalLocation.region, undefined, 'no region when the line is unknown');
    assert.equal(noLine.locations[0].logicalLocations[0].fullyQualifiedName, 'html > body > a:nth-of-type(1)');
  });

  it('leaves non-source rows with the instance id as location', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-source'));
    assert.ok(ok);
    const infra = sarif.runs[0].results.find(r => r.properties.result_id === 'res-r01');
    assert.deepEqual(infra.locations, [{
      physicalLocation: { artifactLocation: { uri: 'https://agency.example.gov/' } }
    }]);
  });

  it('keeps source-row SARIF valid against the vendored schema', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-source'));
    assert.ok(ok);
    const schema = await loadSarifSchemaForValidation();
    const result = validate(schema, sarif);
    assert.ok(result.ok, `SARIF validation failed: ${result.errors.slice(0, 5).join('; ')}`);
    assert.ok(validateSarif(sarif).ok);
  });

  it('validates against the vendored SARIF schema (option 3)', async () => {
    const { ok, sarif } = await convertToSarif(fixture('run-results-only'));
    assert.ok(ok);
    const schema = await loadSarifSchemaForValidation();
    const problems = checkSchema(schema);
    assert.equal(problems.length, 0, `schema has unsupported keywords: ${problems.join(', ')}`);
    const result = validate(schema, sarif);
    assert.ok(result.ok, `SARIF validation failed: ${result.errors.slice(0, 5).join('; ')}`);
  });
});
