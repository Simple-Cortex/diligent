import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { SECTION_TITLES, renderReport } from '../scripts/report.mjs';
import { FIXED_NOW, ROOT, copyTo, run, withTemp } from './helpers.mjs';

const GATE = join(ROOT, 'fixtures', 'gate');
async function gated(name, fn) {
  await withTemp(async tmp => {
    const dir = join(tmp, name);
    await copyTo(join(GATE, name), dir);
    await run('scripts/gate.mjs', ['--dir', dir, '--now', FIXED_NOW]);
    return fn(dir);
  });
}

test('REPORT.md has the required header and the nine sections in blueprint order', async () => {
  await gated('mandatory-fail', async dir => {
    const result = await run('scripts/report.mjs', ['--dir', dir]);
    assert.equal(result.code, 0, result.stderr);
    const text = await readFile(join(dir, 'REPORT.md'), 'utf8');
    const header = text.slice(0, text.indexOf('## 1.'));
    for (const label of ['Target:', 'Snapshot and date:', 'Audit modes:', 'Selected profiles:', 'Assessment status:']) assert.ok(header.includes(label), label);
    const positions = SECTION_TITLES.map((title, i) => text.indexOf(`## ${i + 1}. ${title}`));
    assert.ok(positions.every(p => p >= 0), `all sections present: ${positions}`);
    assert.deepEqual([...positions].sort((a, b) => a - b), positions, 'sections are in order');
    assert.equal(SECTION_TITLES.length, 9);
  });
});

test('a non-passing assessment still exits 0 when the report is written, and says so', async () => {
  for (const [name, status] of [['mandatory-fail', 'FAIL'], ['incomplete-untested', 'INCOMPLETE']]) {
    await gated(name, async dir => {
      const result = await run('scripts/report.mjs', ['--dir', dir]);
      assert.equal(result.code, 0);
      assert.match(result.stdout, /not that the assessment passed/);
      assert.match(await readFile(join(dir, 'REPORT.md'), 'utf8'), new RegExp(`Assessment status:\\*\\* ${status}`));
    });
  }
  const help = await run('scripts/report.mjs', ['--help']);
  assert.match(help.stdout, /Exit codes: 0 when REPORT\.md was written, even if the assessment is fail or/);
});

test('the report lists failures, exceptions and the absence of repairs without softening them', async () => {
  await gated('exception-does-not-pass', async dir => {
    await run('scripts/report.mjs', ['--dir', dir]);
    const text = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.match(text, /OBL-A11Y-1\.1\.1/);
    assert.match(text, /exc-1 on OBL-A11Y-1\.1\.1: active\. It does not change any result\./);
    assert.match(text, /No repair plan and no verification exist/);
    assert.match(text, /No release decision is recorded/);
    assert.match(text, /not a government certification/);
  });
});

test('coverage appears as counts with denominators and no blended score', async () => {
  await gated('incomplete-unknown-applicability', async dir => {
    await run('scripts/report.mjs', ['--dir', dir]);
    const text = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.match(text, /\| Unresolved applicability \| 1 \| 2 \|/);
    assert.match(text, /never blended into a single score/);
    assert.doesNotMatch(text, /\d+\s*%/);
  });
});

test('the report renders the recorded assessment and never recomputes it', () => {
  const assessment = {
    schema_version: '1.0.0', run_id: 'run-2026-10-17-abc123', computed_at: '2026-10-17T00:00:00.000Z', overall: 'pass_for_declared_scope', has_incomplete_work: false,
    counts: { selected: 1, applicable: 1, not_applicable: 0, unresolved_applicability: 0, pending: 0, required_instances: 1, executed: 1, reviewed: 0, unsupported: 0 },
    denominators: { applicable: 1, not_applicable: 1, unresolved_applicability: 1, pending: 1, executed: 1, reviewed: 0, unsupported: 1 },
    unresolved: [], failures: [], exceptions: [], ux_findings_ref: null, release_disposition: null, limitations: [], claim_boundary: 'Boundary text.'
  };
  const withFailure = { ...assessment, failures: [{ result_id: 'r1', obligation_id: 'o', instance_id: 'i', check_id: 'c' }] };
  const text = renderReport({ auditId: 'demo', assessment: withFailure });
  assert.match(text, /PASS FOR DECLARED SCOPE/, 'the recorded overall is shown as recorded; fixing it is the gate\'s job');
  assert.match(text, /\| o \| c \| i \| r1 \|/);
});

test('the report refuses a missing or invalid assessment', async () => {
  await withTemp(async tmp => {
    const missing = await run('scripts/report.mjs', ['--dir', tmp]);
    assert.equal(missing.code, 3);
    assert.match(missing.stderr, /ASSESSMENT\.json is missing/);
    await writeFile(join(tmp, 'ASSESSMENT.json'), '{"overall":"pass"}');
    const invalid = await run('scripts/report.mjs', ['--dir', tmp]);
    assert.equal(invalid.code, 3);
    assert.match(invalid.stderr, /refusing to render/);
    await rm(join(tmp, 'REPORT.md'), { force: true });
  });
});

test('init, gate and report together: demo audit reports incomplete and records the stage', async () => {
  await withTemp(async workspace => {
    const data = join(ROOT, 'fixtures', 'package');
    await run('scripts/init.mjs', ['demo', '--root', workspace, '--data-root', data, '--profile', 'mini-profile']);
    assert.equal((await run('scripts/gate.mjs', ['demo', '--root', workspace])).code, 2);
    const report = await run('scripts/report.mjs', ['demo', '--root', workspace]);
    assert.equal(report.code, 0, report.stderr);
    const text = await readFile(join(workspace, '.diligent', 'demo', 'REPORT.md'), 'utf8');
    assert.match(text, /INCOMPLETE/);
    assert.match(text, /rule pack mini-core 0\.1\.0/);
    assert.match(text, /not yet human reviewed: 2 of 2/);
    const status = JSON.parse((await run('scripts/status.mjs', ['demo', '--root', workspace, '--json'])).stdout);
    assert.equal(status.pipeline_state, 'reported');
  });
});
