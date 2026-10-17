import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { validate } from '../scripts/lib/schema.mjs';
import { ROOT } from './helpers.mjs';

const load = async name => JSON.parse(await readFile(join(ROOT, 'schemas', `${name}.schema.json`), 'utf8'));
const enumOf = (schema, ...path) => path.reduce((node, key) => node[key], schema).enum;

test('frozen enums are exactly as specified', async () => {
  const check = await load('check-result');
  assert.deepEqual(enumOf(check, 'properties', 'status'), ['pass', 'fail', 'needs_review', 'not_tested', 'blocked', 'error']);
  const applicability = await load('applicability');
  assert.deepEqual(enumOf(applicability, '$defs', 'obligation', 'properties', 'state'), ['applicable', 'not_applicable', 'unknown', 'pending_effective_date']);
  const assessment = await load('assessment');
  assert.deepEqual(enumOf(assessment, 'properties', 'overall'), ['fail', 'incomplete', 'pass_for_declared_scope', 'not_applicable']);
  const findings = await load('findings');
  assert.deepEqual(enumOf(findings, '$defs', 'finding', 'properties', 'category'), ['mandatory_failure', 'possible_failure', 'evidence_gap', 'ux_barrier', 'recommendation']);
  const evidence = await load('evidence-record');
  assert.deepEqual(enumOf(evidence, 'properties', 'trust'), ['direct_observation', 'verified_document', 'owner_assertion', 'inference', 'untrusted_claim']);
  const pack = await load('rule-pack');
  assert.deepEqual(enumOf(pack, '$defs', 'rule', 'properties', 'lifecycle'), ['research', 'draft', 'validated', 'published', 'retired']);
  assert.deepEqual(enumOf(pack, '$defs', 'rule', 'properties', 'channel_support', 'properties', 'human'), ['automated', 'assisted', 'human', 'documentary', 'unsupported']);
  const run = await load('run');
  assert.deepEqual(enumOf(run, 'properties', 'pipeline_state'), ['initialized', 'discovered', 'scoped', 'planned', 'collecting', 'assessed', 'proposed', 'verified', 'reported']);
  assert.deepEqual(enumOf(run, 'properties', 'modes', 'items'), ['source', 'page', 'journey', 'documents', 'combined']);
  const registry = await load('authority-registry');
  assert.deepEqual(enumOf(registry, '$defs', 'authority', 'properties', 'kind'), ['statute', 'regulation', 'executive_order', 'binding_policy', 'contract', 'agency_policy', 'technical_standard', 'guidance']);
  assert.deepEqual(enumOf(registry, '$defs', 'authority', 'properties', 'status'), ['active', 'pending', 'draft', 'superseded', 'withdrawn', 'disputed', 'unverified']);
});

const RUN = 'run-2026-10-17-abc123';
const T = '2026-10-17T00:00:00Z';
const validResult = () => ({
  schema_version: '1.0.0', result_id: 'r1', check_id: 'c', check_version: '1', obligation_id: 'o', instance_id: 'i', applicability_ref: 'o',
  status: 'pass', evidence_refs: [], expected: 'x', observed: 'y', limitations: [], executed_at: T, engine_version: '0.1.0', supersedes_result_id: null
});

test('top-level unknown fields are rejected and extensions are accepted', async () => {
  const schema = await load('check-result');
  assert.equal(validate(schema, validResult()).ok, true);
  assert.equal(validate(schema, { ...validResult(), extensions: { 'x-org': { a: 1 } } }).ok, true);
  const typo = validate(schema, { ...validResult(), obsrved: 'oops' });
  assert.equal(typo.ok, false);
  assert.match(typo.errors[0].message, /unknown field/);
});

test('schema_version is required and must match', async () => {
  const schema = await load('check-result');
  const missing = validResult();
  delete missing.schema_version;
  assert.equal(validate(schema, missing).ok, false);
  assert.equal(validate(schema, { ...validResult(), schema_version: '2.0.0' }).ok, false);
});

test('an invalid result status and a missing required field are rejected', async () => {
  const schema = await load('check-result');
  assert.equal(validate(schema, { ...validResult(), status: 'skipped' }).ok, false);
  const missing = validResult();
  delete missing.evidence_refs;
  assert.equal(validate(schema, missing).ok, false);
});

test('a not_applicable obligation needs a cited exclusion with predicate facts', async () => {
  const schema = await load('applicability');
  const obligation = (extra) => ({
    obligation_id: 'o', rule_id: null, pack_id: null, authority_id: null, mandatory: true, state: 'not_applicable', temporal_state: 'effective',
    effective_date: null, predicate_trace: [], facts_used: [], exclusion: null, ...extra
  });
  const doc = o => ({ schema_version: '1.0.0', run_id: RUN, audit_id: 'demo', profile_id: 'p', evaluated_at: T, obligations: [o] });
  assert.equal(validate(schema, doc(obligation({}))).ok, false, 'no exclusion at all');
  assert.equal(validate(schema, doc(obligation({ exclusion: { citation: '', predicate_facts: ['f'], evidence_refs: [] } }))).ok, false, 'empty citation');
  assert.equal(validate(schema, doc(obligation({ exclusion: { citation: 'A01 E205', predicate_facts: [], evidence_refs: [] } }))).ok, false, 'no predicate facts');
  assert.equal(validate(schema, doc(obligation({ exclusion: { citation: 'A01 E205', predicate_facts: ['f'], evidence_refs: [] } }))).ok, true);
  assert.equal(validate(schema, doc(obligation({ state: 'unknown' }))).ok, true, 'unknown needs no exclusion');
});

test('a non-known scope fact must carry a question, owner role and affected obligations', async () => {
  const schema = await load('scope');
  const fact = extra => ({
    fact_id: 'f', group: 'government_owner', label: 'L', value: null, state: 'unknown', evidence_refs: [], provided_by: null, observed_at: null, valid_until: null,
    question: 'Q?', owner_role: 'owner', affected_obligations: ['all-packs'], effect_on_assessment: 'E', ...extra
  });
  const doc = f => ({ schema_version: '1.0.0', audit_id: 'demo', run_id: RUN, profile_id: 'p', modes: ['source'], facts: [f], permissions: {}, processing_boundary: {}, source_deployment_relationship: null });
  assert.equal(validate(schema, doc(fact({}))).ok, true);
  assert.equal(validate(schema, doc(fact({ question: null }))).ok, false);
  assert.equal(validate(schema, doc(fact({ affected_obligations: [] }))).ok, false);
  assert.equal(validate(schema, doc(fact({ state: 'known', value: 'US', question: null, owner_role: null, effect_on_assessment: null, affected_obligations: [] }))).ok, true);
});

test('the assessment never allows a release_disposition that pretends to be a result', async () => {
  const schema = await load('assessment');
  const base = {
    schema_version: '1.0.0', run_id: RUN, computed_at: T, overall: 'incomplete', has_incomplete_work: true,
    counts: { selected: 0, applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, required_instances: 0, executed: 0, reviewed: 0, unsupported: 0 },
    denominators: { applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, executed: 0, reviewed: 0, unsupported: 0 },
    unresolved: [], failures: [], exceptions: [{ exception_id: 'e', obligation_id: 'o', status: 'active', changes_result: false }],
    ux_findings_ref: null, release_disposition: null, limitations: [], claim_boundary: 'x'
  };
  assert.equal(validate(schema, base).ok, true);
  assert.equal(validate(schema, { ...base, exceptions: [{ exception_id: 'e', obligation_id: 'o', status: 'active', changes_result: true }] }).ok, false);
  assert.equal(validate(schema, { ...base, overall: 'pass' }).ok, false);
});
