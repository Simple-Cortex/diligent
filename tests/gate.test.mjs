import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir, rm, writeFile } from 'node:fs/promises';
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

for (const name of (await readdir(GATE)).sort()) {
  test(`gate scenario ${name}: exit code and overall match expected.json`, async () => {
    const expected = JSON.parse(await readFile(join(GATE, name, 'expected.json'), 'utf8'));
    await scenario(name, async dir => {
      const result = await gate(dir);
      assert.equal(result.code, expected.exit_code, `${result.stdout}\n${result.stderr}`);
      const assessment = await readAssessment(dir);
      assert.equal(assessment.overall, expected.overall);
      const loaded = await loadArtifact(dir, 'ASSESSMENT.json');
      assert.equal(loaded.ok, true, loaded.errors.join('; '));
      assert.equal(assessment.release_disposition, null, 'the gate never invents a release decision');
    });
  });
}

test('the five exit codes: only the pass and not_applicable outcomes exit 0', async () => {
  const seen = new Map();
  for (const name of await readdir(GATE)) {
    const expected = JSON.parse(await readFile(join(GATE, name, 'expected.json'), 'utf8'));
    seen.set(expected.overall, new Set([...(seen.get(expected.overall) || []), expected.exit_code]));
  }
  assert.deepEqual([...seen.get('incomplete')].sort(), [2, 3]);
  assert.deepEqual([...seen.get('fail')], [1]);
  assert.deepEqual([...seen.get('pass_for_declared_scope')], [0]);
});

test('an exception does not turn a failure into a pass and is listed separately', async () => {
  await scenario('exception-does-not-pass', async dir => {
    const result = await gate(dir);
    assert.equal(result.code, 1);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'fail');
    assert.equal(assessment.failures.length, 1);
    assert.deepEqual(assessment.exceptions.map(e => [e.exception_id, e.status, e.changes_result]), [['exc-1', 'active', false]]);
    assert.match(assessment.limitations.join('\n'), /Exceptions never change a result/);
  });
});

test('invalid input still writes an incomplete assessment, names the artifact and line, and exits 3', async () => {
  await scenario('invalid-schema', async dir => {
    await writeFile(join(dir, 'ASSESSMENT.json'), JSON.stringify({ stale: 'previous assessment' }));
    const result = await gate(dir);
    assert.equal(result.code, 3);
    const assessment = await readAssessment(dir);
    assert.equal(assessment.overall, 'incomplete');
    assert.equal(assessment.unresolved[0].kind, 'invalid_artifact');
    assert.equal(assessment.unresolved[0].ref, 'RESULTS.jsonl');
    assert.match(assessment.limitations.join('\n'), /RESULTS\.jsonl line 1 \/status/);
  });
});

for (const missing of ['APPLICABILITY.json', 'RESULTS.jsonl']) {
  test(`a missing ${missing} is incomplete with exit 2 and an explicit limitation, never a pass`, async () => {
    await scenario('pass-declared-scope', async dir => {
      await rm(join(dir, missing));
      const result = await gate(dir);
      assert.equal(result.code, 2, result.stdout);
      const assessment = await readAssessment(dir);
      assert.equal(assessment.overall, 'incomplete');
      assert.ok(assessment.unresolved.some(u => u.kind === 'missing_artifact' && u.ref === missing));
      assert.match(assessment.limitations.join('\n'), new RegExp(`${missing.replace('.', '\\.')} is missing`));
    });
  });
}

test('a missing RULES.lock.json is incomplete: support for the checks cannot be established', async () => {
  await scenario('pass-declared-scope', async dir => {
    await rm(join(dir, 'RULES.lock.json'));
    assert.equal((await gate(dir)).code, 2);
    assert.ok((await readAssessment(dir)).unresolved.some(u => u.ref === 'RULES.lock.json'));
  });
});

test('missing exceptions and review queue are recorded as limitations, not silently treated as clean', async () => {
  await scenario('pass-declared-scope', async dir => {
    await rm(join(dir, 'EXCEPTIONS.json'));
    await rm(join(dir, 'REVIEW-QUEUE.json'));
    const result = await gate(dir);
    assert.equal(result.code, 0);
    const text = (await readAssessment(dir)).limitations.join('\n');
    assert.match(text, /EXCEPTIONS\.json is absent/);
    assert.match(text, /REVIEW-QUEUE\.json is absent/);
  });
});

test('artifacts from different runs are rejected', async () => {
  await scenario('pass-declared-scope', async dir => {
    const path = join(dir, 'EXCEPTIONS.json');
    const doc = JSON.parse(await readFile(path, 'utf8'));
    doc.run_id = 'run-2026-10-18-ffffff';
    await writeFile(path, JSON.stringify(doc));
    const result = await gate(dir);
    assert.equal(result.code, 3);
    assert.ok((await readAssessment(dir)).unresolved.some(u => u.kind === 'run_id_mismatch'));
  });
});

test('an obligation that no supported channel can test blocks a pass', async () => {
  await scenario('pass-declared-scope', async dir => {
    const path = join(dir, 'RULES.lock.json');
    const lock = JSON.parse(await readFile(path, 'utf8'));
    lock.supported_check_matrix[0].unsupported = true;
    for (const channel of Object.keys(lock.supported_check_matrix[0].channel_support)) lock.supported_check_matrix[0].channel_support[channel] = 'unsupported';
    await writeFile(path, JSON.stringify(lock));
    assert.equal((await gate(dir)).code, 2);
    const assessment = await readAssessment(dir);
    assert.ok(assessment.unresolved.some(u => u.kind === 'unsupported_obligation'));
    assert.equal(assessment.counts.unsupported, 1);
  });
});

test('pending_effective_date keeps an otherwise passing audit incomplete', async () => {
  await scenario('pass-declared-scope', async dir => {
    const path = join(dir, 'APPLICABILITY.json');
    const doc = JSON.parse(await readFile(path, 'utf8'));
    Object.assign(doc.obligations[1], { state: 'pending_effective_date', temporal_state: 'pending' });
    await writeFile(path, JSON.stringify(doc));
    assert.equal((await gate(dir)).code, 2);
    assert.equal((await readAssessment(dir)).counts.pending, 1);
  });
});

test('rerunning after a result regresses overwrites the earlier pass', async () => {
  await scenario('pass-declared-scope', async dir => {
    assert.equal((await gate(dir)).code, 0);
    const path = join(dir, 'RESULTS.jsonl');
    const lines = (await readFile(path, 'utf8')).trim().split('\n').map(JSON.parse);
    lines[1].status = 'blocked';
    await writeFile(path, lines.map(l => JSON.stringify(l)).join('\n') + '\n');
    assert.equal((await gate(dir)).code, 2);
    assert.equal((await readAssessment(dir)).overall, 'incomplete');
  });
});

test('identical frozen inputs give byte-identical assessments', async () => {
  await scenario('incomplete-untested', async dir => {
    await gate(dir);
    const first = await readFile(join(dir, 'ASSESSMENT.json'), 'utf8');
    await gate(dir);
    assert.equal(await readFile(join(dir, 'ASSESSMENT.json'), 'utf8'), first);
  });
});

test('--json prints the assessment and a nonexistent audit exits 3 without writing', async () => {
  await scenario('mandatory-fail', async dir => {
    const result = await gate(dir, ['--json']);
    assert.equal(result.code, 1);
    assert.equal(JSON.parse(result.stdout).overall, 'fail');
  });
  await withTemp(async tmp => {
    const result = await gate(join(tmp, 'ghost'));
    assert.equal(result.code, 3);
    assert.match(result.stderr, /does not exist/);
  });
});

test('a freshly initialized audit gates as incomplete (exit 2), records the event and advances the run', async () => {
  await withTemp(async workspace => {
    const data = join(ROOT, 'fixtures', 'package');
    assert.equal((await run('scripts/init.mjs', ['demo', '--root', workspace, '--data-root', data, '--profile', 'mini-profile'])).code, 0);
    const result = await run('scripts/gate.mjs', ['demo', '--root', workspace, '--json']);
    assert.equal(result.code, 2, result.stderr);
    const assessment = JSON.parse(result.stdout);
    assert.equal(assessment.overall, 'incomplete');
    const audit = join(workspace, '.diligent', 'demo');
    const kinds = assessment.unresolved.map(u => `${u.kind}:${u.ref}`);
    assert.ok(kinds.includes('missing_artifact:APPLICABILITY.json'));
    assert.ok(kinds.includes('missing_artifact:RESULTS.jsonl'));
    const events = (await loadArtifact(audit, 'EVENTS.jsonl')).data;
    assert.equal(events.at(-1).event, 'gate_computed');
    assert.equal(events.at(-1).artifact, 'ASSESSMENT.json');
    const [runId] = await readdir(join(audit, 'runs'));
    const runRecord = (await loadArtifact(join(audit, 'runs', runId), 'RUN.json')).data;
    assert.equal(runRecord.pipeline_state, 'assessed');
    assert.equal(runRecord.assessment_complete, false);
  });
});

test('gate --help documents the exit codes', async () => {
  const result = await run('scripts/gate.mjs', ['--help']);
  assert.equal(result.code, 0);
  assert.match(result.stdout, /2\s+incomplete/);
});

test('SIGNALS.jsonl never reaches the gate: ASSESSMENT.json is byte-identical with and without it, even when it is garbage', async () => {
  for (const name of ['mandatory-fail', 'pass-declared-scope', 'incomplete-untested']) {
    await scenario(name, async dir => {
      const first = await gate(dir);
      const without = await readFile(join(dir, 'ASSESSMENT.json'));
      const valid = { schema_version: '1.0.0', signal_id: 'sig-0123456789ab', signal_key: 'craft.font_family_count', family: 'craft', instance_id: 'page-1', state: 'lead', value: 5, unit: null, observed: 'Five families.', threshold: null, label: 'static', normative: false, evidence_refs: [], limitations: ['Practice.'], executed_at: FIXED_NOW, engine_version: '0.2.0', run_id: 'run-2026-10-17-abc123', extensions: {} };
      for (const body of [`${JSON.stringify(valid)}\n`, `${JSON.stringify({ ...valid, state: 'needs_review' })}\nnot json\n\u0000\n`, '']) {
        await writeFile(join(dir, 'SIGNALS.jsonl'), body);
        const again = await gate(dir);
        assert.equal(again.code, first.code, `${name}: exit code unchanged`);
        assert.equal(again.stdout, first.stdout, `${name}: gate output unchanged`);
        assert.deepEqual(await readFile(join(dir, 'ASSESSMENT.json')), without, `${name}: ASSESSMENT.json byte-identical`);
      }
    });
  }
});

test('guard: the gate and the status arithmetic never mention SIGNALS', async () => {
  for (const file of ['scripts/gate.mjs', 'scripts/lib/status.mjs']) {
    assert.doesNotMatch(await readFile(join(ROOT, file), 'utf8'), /SIGNALS/, `${file} must not read SIGNALS.jsonl`);
  }
});
