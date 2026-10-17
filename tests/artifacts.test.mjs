import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { UsageError, appendEvent, assertAuditId, atomicWrite, atomicWriteJson, loadArtifact, newRunId, resolveRoot, sha256 } from '../scripts/lib/artifacts.mjs';
import { parseArgs } from '../scripts/lib/cli.mjs';
import { withTemp } from './helpers.mjs';

test('resolveRoot honors --root, then DILIGENT_ROOT, then the nearest .diligent ancestor, then cwd', async () => {
  await withTemp(async dir => {
    const nested = join(dir, 'a', 'b');
    await mkdir(join(dir, '.diligent'), { recursive: true });
    await mkdir(nested, { recursive: true });
    assert.equal(resolveRoot({ root: '/x/y', env: { DILIGENT_ROOT: '/z' }, cwd: nested }), '/x/y');
    assert.equal(resolveRoot({ env: { DILIGENT_ROOT: '/z' }, cwd: nested }), '/z');
    assert.equal(resolveRoot({ env: {}, cwd: nested }), dir);
    const bare = join(dir, '..');
    assert.equal(resolveRoot({ env: {}, cwd: join(dir, 'a') }), dir);
    assert.ok(resolveRoot({ env: {}, cwd: bare }));
  });
});

test('audit ids must be kebab-case', () => {
  assert.equal(assertAuditId('public-site'), 'public-site');
  for (const bad of ['', 'Public', 'a_b', '-a', 'a--b', '../x', 'a/b']) assert.throws(() => assertAuditId(bad), UsageError, bad);
});

test('run ids have the documented shape', () => {
  assert.match(newRunId(new Date('2026-10-17T05:00:00Z')), /^run-2026-10-17-[0-9a-f]{6}$/);
});

test('atomicWrite replaces content and leaves no temp files', async () => {
  await withTemp(async dir => {
    const path = join(dir, 'sub', 'A.json');
    await atomicWriteJson(path, { a: 1 });
    await atomicWriteJson(path, { a: 2 });
    assert.deepEqual(JSON.parse(await readFile(path, 'utf8')), { a: 2 });
    assert.deepEqual(await readdir(join(dir, 'sub')), ['A.json']);
  });
});

test('atomicWrite to an impossible target fails without leaving a temp file', async () => {
  await withTemp(async dir => {
    await writeFile(join(dir, 'file'), 'x');
    await assert.rejects(() => atomicWrite(join(dir, 'file', 'child.json'), '{}'));
    assert.deepEqual((await readdir(dir)).sort(), ['file']);
  });
});

test('appendEvent writes schema-valid JSONL lines with the documented fields', async () => {
  await withTemp(async dir => {
    await appendEvent(dir, { run_id: 'run-2026-10-17-abc123', stage: 'initialized', event: 'audit_initialized', artifact: 'RUN.json', sha256: sha256('x') });
    await appendEvent(dir, { run_id: 'run-2026-10-17-abc123', stage: 'assessed', event: 'gate_computed', artifact: null, sha256: null });
    const loaded = await loadArtifact(dir, 'EVENTS.jsonl');
    assert.equal(loaded.ok, true, loaded.errors.join('\n'));
    assert.equal(loaded.data.length, 2);
    for (const key of ['ts', 'run_id', 'stage', 'event', 'artifact', 'sha256']) assert.ok(key in loaded.data[0], key);
  });
});

test('loadArtifact reports missing, malformed and schema-invalid files distinctly, naming file and line', async () => {
  await withTemp(async dir => {
    assert.deepEqual(await loadArtifact(dir, 'RESULTS.jsonl').then(r => [r.exists, r.ok]), [false, false]);
    await writeFile(join(dir, 'RESULTS.jsonl'), '{not json}\n');
    const malformed = await loadArtifact(dir, 'RESULTS.jsonl');
    assert.equal(malformed.ok, false);
    assert.match(malformed.errors[0], /RESULTS\.jsonl line 1: not valid JSON/);
    await writeFile(join(dir, 'RESULTS.jsonl'), `${JSON.stringify({ schema_version: '1.0.0', status: 'pass' })}\n`);
    const invalid = await loadArtifact(dir, 'RESULTS.jsonl');
    assert.equal(invalid.ok, false);
    assert.ok(invalid.errors.some(e => e.startsWith('RESULTS.jsonl line 1 /result_id: required field is missing')), invalid.errors.join('\n'));
    await writeFile(join(dir, 'ASSESSMENT.json'), '[]');
    const wrongType = await loadArtifact(dir, 'ASSESSMENT.json');
    assert.equal(wrongType.ok, false);
    assert.match(wrongType.errors[0], /^ASSESSMENT\.json \/: expected object/);
  });
});

test('parseArgs: values, booleans, unknown options and missing values', () => {
  const { positional, options } = parseArgs(['demo', '--mode', 'page', '--force', '--root=/tmp/x'], { mode: 'string', force: 'boolean', root: 'string' });
  assert.deepEqual(positional, ['demo']);
  assert.deepEqual(options, { mode: 'page', force: true, root: '/tmp/x' });
  assert.throws(() => parseArgs(['--nope'], {}), /unknown option --nope/);
  assert.throws(() => parseArgs(['--mode'], { mode: 'string' }), /requires a value/);
  assert.throws(() => parseArgs(['--mode', '--force'], { mode: 'string', force: 'boolean' }), /requires a value/);
  assert.equal(parseArgs(['-h']).options.help, true);
});
