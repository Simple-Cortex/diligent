/**
 * Tests for baseline and diff mode.
 *
 * Covers: baseline write, diff partitioning (new/known/resolved/unverified),
 * CLI exit codes, --fail-on modes, JSON output, fingerprint collisions,
 * result-derived entries, and baseline validation.
 */
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { writeBaseline, loadBaseline, diffFindings } from '../scripts/lib/diff.mjs';

const ROOT = resolve(fileURLToPath(import.meta.url), '..', '..');
const FIXTURES = join(ROOT, 'fixtures', 'diff');

describe('Baseline and diff', () => {
  it('writes a baseline from a run directory with findings', async () => {
    const baseline = await writeBaseline(join(FIXTURES, 'baseline-run'));
    assert.equal(baseline.schema_version, '1.0');
    assert.equal(baseline.created_from_run, 'run-2026-10-18-bbb001');
    assert.ok(baseline.engine_version);
    // Should have entries from both results and findings
    assert.ok(baseline.fingerprints.length >= 2);
    // All entries should have source field
    for (const entry of baseline.fingerprints) {
      assert.ok(['result', 'finding'].includes(entry.source), 'entry should have source field');
    }
  });

  it('writes a baseline from a run directory with results only', async () => {
    const baseline = await writeBaseline(join(FIXTURES, 'results-only-run'));
    assert.equal(baseline.schema_version, '1.0');
    assert.ok(baseline.fingerprints.length >= 2);
    // All entries should be from results
    for (const entry of baseline.fingerprints) {
      assert.equal(entry.source, 'result');
      assert.ok(entry.check_id, 'result entry should have check_id');
      assert.ok(entry.instance_id, 'result entry should have instance_id');
    }
  });

  it('baseline fingerprints are sorted by fingerprint', async () => {
    const baseline = await writeBaseline(join(FIXTURES, 'baseline-run'));
    const fps = baseline.fingerprints.map(f => f.fingerprint);
    assert.deepEqual(fps, [...fps].sort());
  });

  it('loads a baseline from file', async () => {
    const baseline = await loadBaseline(join(FIXTURES, 'baseline.json'));
    assert.equal(baseline.schema_version, '1.0');
    assert.equal(baseline.fingerprints.length, 3);
    // All entries should have source field
    for (const entry of baseline.fingerprints) {
      assert.ok(['result', 'finding'].includes(entry.source));
    }
  });

  it('partitions findings into new, known, resolved, unverified', async () => {
    const baseline = await loadBaseline(join(FIXTURES, 'baseline.json'));
    const result = await diffFindings(join(FIXTURES, 'current-run'), baseline);

    // The run also carries two failing results; they are result-derived entries and are new here.
    const newFindings = result.new.filter(e => e.source === 'finding');
    assert.equal(newFindings.length, 1);
    assert.equal(newFindings[0].fingerprint, 'fp-new-001');
    assert.equal(result.new.filter(e => e.source === 'result').length, 2);

    assert.equal(result.known.length, 1);
    assert.equal(result.known[0].fingerprint, 'fp-known-001');

    assert.equal(result.resolved.length, 1);
    assert.equal(result.resolved[0].fingerprint, 'fp-resolved-001');

    assert.equal(result.unverified.length, 1);
    assert.equal(result.unverified[0].fingerprint, 'fp-unverified-001');
  });

  it('resolved requires the rule was executed in the current run', async () => {
    const baseline = await loadBaseline(join(FIXTURES, 'baseline.json'));
    const result = await diffFindings(join(FIXTURES, 'current-run'), baseline);

    const resolvedRules = result.resolved.map(r => r.rule_id);
    assert.ok(resolvedRules.includes('FED-06'));

    const unverifiedRules = result.unverified.map(r => r.rule_id);
    assert.ok(unverifiedRules.includes('INFRA-12'));
  });

  it('warns on fingerprint collisions in baseline', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'diligent-diff-'));
    try {
      const baselinePath = join(tmpDir, 'baseline.json');
      const collisionBaseline = {
        schema_version: '1.0',
        created_from_run: 'run-test',
        engine_version: '0.2.0',
        fingerprints: [
          { source: 'finding', fingerprint: 'fp-dup', rule_id: 'R1', finding_id: 'f1', first_seen_run: 'r1' },
          { source: 'finding', fingerprint: 'fp-dup', rule_id: 'R2', finding_id: 'f2', first_seen_run: 'r1' }
        ]
      };
      await writeFile(baselinePath, JSON.stringify(collisionBaseline));
      const baseline = await loadBaseline(baselinePath);
      const result = await diffFindings(join(FIXTURES, 'current-run'), baseline);
      assert.ok(result.warnings.some(w => w.includes('baseline fingerprint collision')));
    } finally {
      await rm(tmpDir, { recursive: true, force: true });
    }
  });

  it('rejects a baseline with wrong schema_version', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'diligent-diff-'));
    try {
      const baselinePath = join(tmpDir, 'baseline.json');
      await writeFile(baselinePath, JSON.stringify({ schema_version: '2.0', fingerprints: [] }));
      await assert.rejects(() => loadBaseline(baselinePath), /invalid/);
    } finally {
      await rm(tmpDir, { recursive: true, force: true });
    }
  });

  it('rejects a baseline missing required source field', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'diligent-diff-'));
    try {
      const baselinePath = join(tmpDir, 'baseline.json');
      await writeFile(baselinePath, JSON.stringify({
        schema_version: '1.0',
        created_from_run: 'run-test',
        engine_version: '0.2.0',
        fingerprints: [
          { fingerprint: 'fp1', rule_id: 'R1', first_seen_run: 'r1' }  // missing source
        ]
      }));
      await assert.rejects(() => loadBaseline(baselinePath), /invalid/);
    } finally {
      await rm(tmpDir, { recursive: true, force: true });
    }
  });

  it('returns empty partitions when no findings match', async () => {
    const baseline = {
      schema_version: '1.0',
      created_from_run: 'run-empty',
      engine_version: '0.2.0',
      fingerprints: []
    };
    const result = await diffFindings(join(FIXTURES, 'current-run'), baseline);
    assert.equal(result.new.length, 4, 'two findings and two failing results');
    assert.equal(result.known.length, 0);
    assert.equal(result.resolved.length, 0);
  });

  it('diffs result-derived entries when the run has no FINDINGS.json', async () => {
    const baseline = await writeBaseline(join(FIXTURES, 'results-only-run'));
    const same = await diffFindings(join(FIXTURES, 'results-only-run'), baseline);
    assert.equal(same.new.length, 0);
    assert.equal(same.known.length, baseline.fingerprints.length);
    for (const entry of same.known) assert.equal(entry.source, 'result');
    const [first, ...rest] = baseline.fingerprints;
    const trimmed = { ...baseline, fingerprints: rest };
    const result = await diffFindings(join(FIXTURES, 'results-only-run'), trimmed);
    assert.deepEqual(result.new.map(e => e.fingerprint), [first.fingerprint]);
    assert.equal(result.new[0].check_id, first.check_id);
  });
});
