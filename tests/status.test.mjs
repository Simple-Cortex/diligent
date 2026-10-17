import { test } from 'node:test';
import assert from 'node:assert/strict';
import { aggregate, activeResults, exitCodeFor } from '../scripts/lib/status.mjs';

const ob = (id, state = 'applicable', extra = {}) => ({
  obligation_id: id, mandatory: true, state,
  exclusion: state === 'not_applicable' ? { citation: 'A01 E205', predicate_facts: ['f'], evidence_refs: [] } : null, ...extra
});
const res = (n, obligation, status, extra = {}) => ({ result_id: `r${n}`, check_id: `c-${obligation}`, obligation_id: obligation, instance_id: `i${n}`, status, supersedes_result_id: null, ...extra });
const item = (n, status) => ({ item_id: `q${n}`, status, blocking_reason: 'needs a person' });
const NOW = '2026-10-17T00:00:00.000Z';
const run = (obligations, results = [], { exceptions = [], review = [], coverage = {} } = {}) => aggregate(obligations, results, exceptions, review, { now: NOW, ...coverage });

const table = [
  ['all applicable obligations pass', [ob('a'), ob('b')], [res(1, 'a', 'pass'), res(2, 'b', 'pass')], {}, 'pass_for_declared_scope'],
  ['a mandatory failure fails', [ob('a'), ob('b')], [res(1, 'a', 'pass'), res(2, 'b', 'fail')], {}, 'fail'],
  ['a failure wins over unresolved work', [ob('a'), ob('b', 'unknown')], [res(1, 'a', 'fail')], {}, 'fail'],
  ['a non-mandatory failure does not fail the gate', [ob('a', 'applicable', { mandatory: false })], [res(1, 'a', 'fail')], {}, 'pass_for_declared_scope'],
  ['not_tested is incomplete', [ob('a')], [res(1, 'a', 'not_tested')], {}, 'incomplete'],
  ['blocked is incomplete', [ob('a')], [res(1, 'a', 'blocked')], {}, 'incomplete'],
  ['error is incomplete', [ob('a')], [res(1, 'a', 'error')], {}, 'incomplete'],
  ['needs_review is incomplete', [ob('a')], [res(1, 'a', 'needs_review')], {}, 'incomplete'],
  ['an applicable obligation with no result is incomplete (skipped is not pass)', [ob('a')], [], {}, 'incomplete'],
  ['unknown applicability is incomplete', [ob('a'), ob('b', 'unknown')], [res(1, 'a', 'pass')], {}, 'incomplete'],
  ['pending_effective_date is incomplete, not pass', [ob('a'), ob('b', 'pending_effective_date')], [res(1, 'a', 'pass')], {}, 'incomplete'],
  ['an open review item is incomplete', [ob('a')], [res(1, 'a', 'pass')], { review: [item(1, 'open')] }, 'incomplete'],
  ['an in-progress review item is incomplete', [ob('a')], [res(1, 'a', 'pass')], { review: [item(1, 'in_progress')] }, 'incomplete'],
  ['a resolved review item does not block', [ob('a')], [res(1, 'a', 'pass')], { review: [item(1, 'resolved')] }, 'pass_for_declared_scope'],
  ['stale evidence is incomplete', [ob('a')], [res(1, 'a', 'pass', { stale: true })], {}, 'incomplete'],
  ['stale evidence listed by the caller is incomplete', [ob('a')], [res(1, 'a', 'pass')], { coverage: { stale_result_ids: ['r1'] } }, 'incomplete'],
  ['an unsupported obligation is incomplete even with a pass result', [ob('a')], [res(1, 'a', 'pass')], { coverage: { unsupported_obligation_ids: ['a'] } }, 'incomplete'],
  ['every obligation not_applicable with citations is not_applicable', [ob('a', 'not_applicable'), ob('b', 'not_applicable')], [], {}, 'not_applicable'],
  ['not_applicable without a cited predicate is incomplete', [ob('a', 'not_applicable', { exclusion: null })], [], {}, 'incomplete'],
  ['not_applicable with an empty citation is incomplete', [ob('a', 'not_applicable', { exclusion: { citation: ' ', predicate_facts: ['f'] } })], [], {}, 'incomplete'],
  ['mixed: cited exclusion plus passing obligations passes for the declared scope', [ob('a'), ob('b', 'not_applicable')], [res(1, 'a', 'pass')], {}, 'pass_for_declared_scope'],
  ['mixed: cited exclusion plus an untested obligation is incomplete', [ob('a'), ob('b', 'not_applicable')], [], {}, 'incomplete'],
  ['no obligations at all is incomplete, never a pass', [], [], {}, 'incomplete'],
  ['a result for an unselected obligation is incomplete', [ob('a')], [res(1, 'a', 'pass'), res(2, 'ghost', 'pass')], {}, 'incomplete'],
  ['a duplicate obligation is incomplete', [ob('a'), ob('a')], [res(1, 'a', 'pass')], {}, 'incomplete']
];
for (const [name, obligations, results, options, expected] of table) {
  test(`overall: ${name}`, () => {
    const out = run(obligations, results, options);
    assert.equal(out.overall, expected, JSON.stringify(out.unresolved));
    assert.equal(out.has_incomplete_work, out.unresolved.length > 0);
    if (expected === 'incomplete') assert.ok(out.unresolved.length > 0, 'incomplete must say why');
  });
}

test('an exception never flips a failure and is listed separately', () => {
  const exception = { exception_id: 'x1', obligation_id: 'a', expires_on: '2027-01-01', approval_evidence_refs: ['ev'] };
  const out = run([ob('a')], [res(1, 'a', 'fail')], { exceptions: [exception] });
  assert.equal(out.overall, 'fail');
  assert.equal(out.failures.length, 1);
  assert.deepEqual(out.exceptions, [{ exception_id: 'x1', obligation_id: 'a', status: 'active', changes_result: false }]);
});

test('an exception does not turn an untested obligation into a pass', () => {
  const out = run([ob('a')], [res(1, 'a', 'not_tested')], { exceptions: [{ exception_id: 'x1', obligation_id: 'a', expires_on: null, approval_evidence_refs: ['ev'] }] });
  assert.equal(out.overall, 'incomplete');
});

test('expired and unevidenced exceptions are labelled and still change nothing', () => {
  const out = run([ob('a')], [res(1, 'a', 'fail')], {
    exceptions: [
      { exception_id: 'old', obligation_id: 'a', expires_on: '2026-01-01', approval_evidence_refs: ['ev'] },
      { exception_id: 'noev', obligation_id: 'a', expires_on: null, approval_evidence_refs: [] }
    ]
  });
  assert.equal(out.overall, 'fail');
  assert.deepEqual(out.exceptions.map(e => [e.exception_id, e.status]), [['noev', 'unverified'], ['old', 'expired']]);
});

test('a rerun supersedes the earlier result; superseding a fail with a pass clears it', () => {
  const results = [res(1, 'a', 'fail'), res(2, 'a', 'pass', { supersedes_result_id: 'r1' })];
  assert.deepEqual(activeResults(results).map(r => r.result_id), ['r2']);
  assert.equal(run([ob('a')], results).overall, 'pass_for_declared_scope');
  const reverse = [res(1, 'a', 'pass'), res(2, 'a', 'fail', { supersedes_result_id: 'r1' })];
  assert.equal(run([ob('a')], reverse).overall, 'fail');
});

test('coverage is reported as counts with denominators, never a blended score', () => {
  const out = run(
    [ob('a'), ob('b'), ob('c', 'unknown'), ob('d', 'pending_effective_date'), ob('e', 'not_applicable')],
    [res(1, 'a', 'pass'), res(2, 'b', 'not_tested')],
    { review: [item(1, 'resolved'), item(2, 'open')], coverage: { unsupported_obligation_ids: ['b', 'e'] } }
  );
  assert.deepEqual(out.counts, {
    selected: 5, applicable: 2, not_applicable: 1, unresolved_applicability: 1, pending: 1,
    required_instances: 2, executed: 1, reviewed: 1, unsupported: 1
  });
  assert.deepEqual(out.denominators, { applicable: 5, not_applicable: 5, unresolved_applicability: 5, pending: 5, executed: 2, reviewed: 2, unsupported: 2 });
  assert.equal('score' in out, false);
});

test('a plan-supplied required instance count sets the denominator', () => {
  const out = run([ob('a')], [res(1, 'a', 'pass')], { coverage: { required_instances: 4 } });
  assert.equal(out.counts.required_instances, 4);
  assert.equal(out.counts.executed, 1);
  assert.equal(out.denominators.executed, 4);
});

test('aggregation is deterministic regardless of input order', () => {
  const obligations = [ob('b', 'unknown'), ob('a', 'unknown'), ob('c')];
  const results = [res(3, 'c', 'not_tested'), res(1, 'c', 'blocked')];
  const one = run(obligations, results);
  const two = run([...obligations].reverse(), [...results].reverse());
  assert.deepEqual(one, two);
});

test('exit codes follow the gate contract', () => {
  assert.equal(exitCodeFor('pass_for_declared_scope'), 0);
  assert.equal(exitCodeFor('not_applicable'), 0);
  assert.equal(exitCodeFor('fail'), 1);
  assert.equal(exitCodeFor('incomplete'), 2);
  assert.equal(exitCodeFor('anything-else'), 3);
});
