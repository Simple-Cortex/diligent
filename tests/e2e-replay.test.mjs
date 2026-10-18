import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { ROOT, run } from './helpers.mjs';

const BINARY = join(ROOT, 'target', 'debug', 'diligent-rs');

// The recorded captures (fixtures/a11y/captures) do not ship in the public repository.
// Skip with a stated reason when they are absent; never pass silently otherwise.
const CAPTURES_PRESENT = existsSync(join(ROOT, 'fixtures', 'a11y', 'captures'));

test('the replay and its expectation files are well formed and point at recorded captures', { skip: CAPTURES_PRESENT ? false : 'skipped: fixtures/a11y/captures is absent (recorded captures do not ship publicly)' }, async () => {
  const replay = JSON.parse(await readFile(join(ROOT, 'fixtures', 'e2e', 'replay.json'), 'utf8'));
  const expected = JSON.parse(await readFile(join(ROOT, 'fixtures', 'e2e', 'replay-expected.json'), 'utf8'));
  assert.equal(replay.pages.length, 6);
  assert.equal(new Set(replay.pages.map(p => p.url)).size, 6);
  for (const page of replay.pages) {
    assert.ok(page.url.startsWith(new URL(replay.origin).origin), page.url);
    assert.ok(existsSync(join(ROOT, page.captures, 'manifest.jsonl')), page.captures);
    const recorded = new URL(JSON.parse((await readFile(join(ROOT, page.captures, 'manifest.jsonl'), 'utf8')).split('\n', 1)[0]).final_url);
    assert.equal(new URL(page.url).pathname, recorded.pathname, 'the replay url keeps the recorded path');
  }
  assert.equal(expected.gate_exit, 1);
  assert.deepEqual(expected.failing_result_checks, [...expected.failing_result_checks].sort());
  assert.ok(expected.gate_failing_checks.every(id => expected.failing_result_checks.includes(id)));
});

test('without the binary the offline replay is SKIPPED when allowed, and fails loudly when not', async () => {
  const env = { DILIGENT_RS_BIN: '/nonexistent/diligent-rs' };
  const allowed = await run('scripts/e2e-replay.mjs', ['--allow-missing-binary'], { env });
  assert.equal(allowed.code, 0);
  assert.match(allowed.stdout, /e2e replay: SKIPPED \(diligent-rs binary \(\/nonexistent\/diligent-rs\) not found\)/);
  const strict = await run('scripts/e2e-replay.mjs', [], { env });
  assert.equal(strict.code, 1);
  assert.match(strict.stdout, /FAILED\s+prerequisites/);
});

test('with a built binary the offline replay passes end to end without Chrome or network', { skip: (!existsSync(BINARY) && 'target/debug/diligent-rs is not built') || (!CAPTURES_PRESENT && 'skipped: fixtures/a11y/captures is absent (recorded captures do not ship publicly)') }, async () => {
  const result = await run('scripts/e2e-replay.mjs', [], { env: { DILIGENT_RS_BIN: BINARY, DILIGENT_CHROME: '/nonexistent/chrome' }, });
  assert.equal(result.code, 0, `${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout, /e2e replay: PASSED/);
});
