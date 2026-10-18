import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { loadArtifact } from '../scripts/lib/artifacts.mjs';
import { FIXED_NOW, ROOT, copyTo, run, withTemp } from './helpers.mjs';

const GATE = join(ROOT, 'fixtures', 'gate');
const gate = (dir, extra = []) => run('scripts/gate.mjs', ['--dir', dir, '--now', FIXED_NOW, ...extra]);
const readAssessment = async dir => JSON.parse(await readFile(join(dir, 'ASSESSMENT.json'), 'utf8'));

async function scenario(name, fn) {
  await withTemp(async tmp => {
    const dir = join(tmp, name);
    await copyTo(join(GATE, name), dir);
    return fn(dir);
  });
}

// ---------------------------------------------------------------------------
// (a) Invalid result state string — schema rejects "passed", exit 3
// ---------------------------------------------------------------------------
test('adv-invalid-result-state: a RESULTS row with an invalid state string exits 3 and is incomplete', async () => {
  await scenario('adv-invalid-result-state', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 3, `expected exit 3, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'incomplete');
    assert.ok(assessment.unresolved.some(u => u.kind === 'invalid_artifact' && u.ref === 'RESULTS.jsonl'),
      'expected an invalid_artifact unresolved entry for RESULTS.jsonl');
  });
});

// ---------------------------------------------------------------------------
// (b) All results pass but one applicability fact unknown — invariant 2
// ---------------------------------------------------------------------------
test('adv-unknown-applicability: unknown applicability keeps status incomplete even when all results pass', async () => {
  await scenario('adv-unknown-applicability', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 2, `expected exit 2, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'incomplete');
    assert.ok(assessment.unresolved.some(u => u.kind === 'unknown_applicability'),
      'expected an unknown_applicability unresolved entry');
    assert.equal(assessment.counts.unresolved_applicability, 1);
  });
});

// ---------------------------------------------------------------------------
// (c) All pass but one review item open — invariant 2
// ---------------------------------------------------------------------------
test('adv-open-review: an open review item keeps status incomplete even when all results pass', async () => {
  await scenario('adv-open-review', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 2, `expected exit 2, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'incomplete');
    assert.ok(assessment.unresolved.some(u => u.kind === 'open_review_item'),
      'expected an open_review_item unresolved entry');
  });
});

// ---------------------------------------------------------------------------
// (d) Exception on a failing mandatory rule — invariant 3
// ---------------------------------------------------------------------------
test('adv-exception-on-mandatory-fail: an exception beside a mandatory failure does not turn it into a pass', async () => {
  await scenario('adv-exception-on-mandatory-fail', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 1, `expected exit 1, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'fail');
    assert.equal(assessment.failures.length, 1, 'expected exactly one failure');
    assert.equal(assessment.failures[0].obligation_id, 'OBL-A11Y-1.1.1');
    assert.equal(assessment.exceptions.length, 1);
    assert.equal(assessment.exceptions[0].changes_result, false, 'exception must not change the result');
    assert.match(assessment.limitations.join('\n'), /Exceptions never change a result/);
  });
});

// ---------------------------------------------------------------------------
// (e) A rule in RULES.lock not present in RESULTS — invariant 2
// ---------------------------------------------------------------------------
test('adv-missing-result: an applicable obligation with no result is incomplete, not pass', async () => {
  await scenario('adv-missing-result', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 2, `expected exit 2, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'incomplete');
    assert.ok(assessment.unresolved.some(u => u.kind === 'no_result' && u.ref === 'OBL-A11Y-1.4.3'),
      'expected a no_result unresolved entry for the untested obligation');
  });
});

// ---------------------------------------------------------------------------
// (f) Pre-written ASSESSMENT.json claiming pass must be overwritten — invariant 6
// ---------------------------------------------------------------------------
test('adv-prewritten-pass: the gate recomputes from artifacts and overwrites a pre-written pass', async () => {
  await scenario('adv-prewritten-pass', async dir => {
    // The fixture ships with a pre-written ASSESSMENT.json claiming pass_for_declared_scope.
    // The RESULTS.jsonl has a fail, so the gate must overwrite it.
    const prewritten = JSON.parse(await readFile(join(dir, 'ASSESSMENT.json'), 'utf8'));
    assert.equal(prewritten.overall, 'pass_for_declared_scope', 'fixture should ship with a pre-written pass');

    const result = await gate(dir);
    assert.equal(result.code, 1, `expected exit 1, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'fail', 'the gate must recompute from artifacts, not trust the pre-written status');
    assert.notEqual(assessment.computed_at, prewritten.computed_at, 'the computed_at must be updated');
    const loaded = await loadArtifact(dir, 'ASSESSMENT.json');
    assert.equal(loaded.ok, true, loaded.errors.join('; '));
  });
});

// ---------------------------------------------------------------------------
// (g) Empty RESULTS with non-empty rules lock — invariant 2
// ---------------------------------------------------------------------------
test('adv-empty-results: empty RESULTS with applicable obligations is incomplete', async () => {
  await scenario('adv-empty-results', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 2, `expected exit 2, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'incomplete');
    assert.ok(assessment.unresolved.some(u => u.kind === 'no_result'),
      'expected at least one no_result unresolved entry');
  });
});

// ---------------------------------------------------------------------------
// (h) Duplicate RESULTS rows with conflicting states — safe behaviour
// ---------------------------------------------------------------------------
test('adv-duplicate-results: duplicate results with conflicting states do not report pass', async () => {
  await scenario('adv-duplicate-results', async dir => {
    const result = await gate(dir);
    const assessment = await readAssessment(dir);
    // The gate processes both results. One is pass and one is fail for the same
    // mandatory obligation. The safe behaviour is that the fail enters failures
    // and the overall is fail — it must NOT report pass.
    assert.notEqual(assessment.overall, 'pass_for_declared_scope',
      'duplicate conflicting results must not produce a pass');
    assert.equal(result.code, 1, `expected exit 1 (fail), got ${result.code}\n${result.stdout}\n${result.stderr}`);
    assert.equal(assessment.overall, 'fail');
    assert.ok(assessment.failures.length >= 1, 'expected at least one failure from the conflicting results');
  });
});

// ---------------------------------------------------------------------------
// (i) A blocked result — invariant 2
// ---------------------------------------------------------------------------
test('adv-blocked-result: a blocked result keeps status incomplete', async () => {
  await scenario('adv-blocked-result', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 2, `expected exit 2, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'incomplete');
    assert.ok(assessment.unresolved.some(u => u.kind === 'result_blocked'),
      'expected a result_blocked unresolved entry');
  });
});

// ---------------------------------------------------------------------------
// (j) An error result — invariant 2
// ---------------------------------------------------------------------------
test('adv-error-result: an error result keeps status incomplete', async () => {
  await scenario('adv-error-result', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 2, `expected exit 2, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'incomplete');
    assert.ok(assessment.unresolved.some(u => u.kind === 'result_error'),
      'expected a result_error unresolved entry');
  });
});

// ---------------------------------------------------------------------------
// (k) Zero applicable rules — not_applicable
// ---------------------------------------------------------------------------
test('adv-zero-applicable: zero applicable obligations with cited exclusions yields not_applicable', async () => {
  await scenario('adv-zero-applicable', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 0, `expected exit 0, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'not_applicable');
    assert.equal(assessment.counts.applicable, 0);
    assert.equal(assessment.counts.not_applicable, 2);
  });
});

// ---------------------------------------------------------------------------
// (l) Non-mandatory obligation failing — advisory, not fail
// ---------------------------------------------------------------------------
test('adv-non-mandatory-fail: a non-mandatory obligation failing does not alone produce fail', async () => {
  await scenario('adv-non-mandatory-fail', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 0, `expected exit 0, got ${result.code}\n${result.stdout}\n${result.stderr}`);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'pass_for_declared_scope',
      'a non-mandatory failure must not enter the regulatory gate as fail');
    assert.equal(assessment.failures.length, 0, 'non-mandatory failures must not appear in the failures array');
    assert.match(assessment.limitations.join('\n'), /non-mandatory obligations/,
      'expected a limitation noting the advisory failure');
  });
});

// ===========================================================================
// Dedicated invariant tests
// ===========================================================================

// ---------------------------------------------------------------------------
// Invariant 2: incomplete can never become pass
// ---------------------------------------------------------------------------
test('invariant 2: unknown applicability, open review, blocked, error, missing result, and empty results all yield incomplete, never pass', async () => {
  const incompleteFixtures = [
    'adv-unknown-applicability',
    'adv-open-review',
    'adv-missing-result',
    'adv-blocked-result',
    'adv-error-result',
    'adv-empty-results',
  ];
  for (const name of incompleteFixtures) {
    await scenario(name, async dir => {
      const result = await gate(dir);
      const assessment = await readAssessment(dir);
      assert.notEqual(assessment.overall, 'pass_for_declared_scope',
        `${name}: incomplete condition must never become pass_for_declared_scope`);
      assert.notEqual(assessment.overall, 'not_applicable',
        `${name}: incomplete condition must never become not_applicable`);
      assert.equal(assessment.overall, 'incomplete',
        `${name}: expected incomplete, got ${assessment.overall}`);
    });
  }
});

// ---------------------------------------------------------------------------
// Invariant 3: an exception beside a failure never turns it into pass
// ---------------------------------------------------------------------------
test('invariant 3: an exception recorded beside a failure never turns the overall status into pass', async () => {
  await scenario('adv-exception-on-mandatory-fail', async dir => {
    const result = await gate(dir);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'fail', 'overall must remain fail despite the exception');
    assert.ok(assessment.exceptions.length > 0, 'expected at least one exception');
    for (const exc of assessment.exceptions) {
      assert.equal(exc.changes_result, false, `exception ${exc.exception_id} must not change the result`);
    }
    assert.match(assessment.limitations.join('\n'), /Exceptions never change a result/);
  });
});

// ---------------------------------------------------------------------------
// Invariant 4: a mandatory rule failing yields fail regardless of other passes
// ---------------------------------------------------------------------------
test('invariant 4: a mandatory rule failing yields fail regardless of how many other rules pass', async () => {
  await scenario('adv-exception-on-mandatory-fail', async dir => {
    const result = await gate(dir);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'fail');
    assert.equal(result.code, 1);
    assert.ok(assessment.failures.length >= 1, 'expected at least one mandatory failure');
  });
  // Also verify with the duplicate-results fixture: one pass + one fail on mandatory = fail
  await scenario('adv-duplicate-results', async dir => {
    const result = await gate(dir);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'fail');
    assert.equal(result.code, 1);
  });
});

// ---------------------------------------------------------------------------
// Invariant 5: exit codes 0/1/2/3 map correctly
// ---------------------------------------------------------------------------
test('invariant 5: exit codes — 0 pass, 1 fail, 2 incomplete, 3 invalid', async () => {
  const exitCodeCases = [
    { name: 'adv-zero-applicable', expectedCode: 0, expectedOverall: 'not_applicable' },
    { name: 'adv-non-mandatory-fail', expectedCode: 0, expectedOverall: 'pass_for_declared_scope' },
    { name: 'adv-exception-on-mandatory-fail', expectedCode: 1, expectedOverall: 'fail' },
    { name: 'adv-duplicate-results', expectedCode: 1, expectedOverall: 'fail' },
    { name: 'adv-unknown-applicability', expectedCode: 2, expectedOverall: 'incomplete' },
    { name: 'adv-open-review', expectedCode: 2, expectedOverall: 'incomplete' },
    { name: 'adv-missing-result', expectedCode: 2, expectedOverall: 'incomplete' },
    { name: 'adv-blocked-result', expectedCode: 2, expectedOverall: 'incomplete' },
    { name: 'adv-error-result', expectedCode: 2, expectedOverall: 'incomplete' },
    { name: 'adv-empty-results', expectedCode: 2, expectedOverall: 'incomplete' },
    { name: 'adv-invalid-result-state', expectedCode: 3, expectedOverall: 'incomplete' },
  ];
  for (const { name, expectedCode, expectedOverall } of exitCodeCases) {
    await scenario(name, async dir => {
      const result = await gate(dir);
      assert.equal(result.code, expectedCode, `${name}: expected exit ${expectedCode}, got ${result.code}`);
      const assessment = await readAssessment(dir);
      assert.equal(assessment.overall, expectedOverall, `${name}: expected overall ${expectedOverall}, got ${assessment.overall}`);
    });
  }
});

// ---------------------------------------------------------------------------
// Invariant 6: the gate recomputes from artifacts, never trusts a pre-written status
// ---------------------------------------------------------------------------
test('invariant 6: the gate recomputes from artifacts and does not trust a status written into an artifact', async () => {
  await scenario('adv-prewritten-pass', async dir => {
    // Pre-existing ASSESSMENT.json claims pass_for_declared_scope
    const before = JSON.parse(await readFile(join(dir, 'ASSESSMENT.json'), 'utf8'));
    assert.equal(before.overall, 'pass_for_declared_scope');

    const result = await gate(dir);
    const after = await readAssessment(dir);
    // The actual data has a fail, so the gate must overwrite the pre-written pass
    assert.equal(result.code, 1);
    assert.equal(after.overall, 'fail', 'gate must recompute from artifacts, not trust the pre-written pass');
    assert.equal(after.release_disposition, null, 'the gate never invents a release decision');
  });
});
