import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { validateData } from '../scripts/lib/artifacts.mjs';
import { elementCheckIds, runComponentGate } from '../scripts/lib/orchestrate/component-gate.mjs';
import { StorybookError, discoverStories, isPrivateAddress, parseStoryIndex, rngFromSeed, storyPages, storybookInventory } from '../scripts/lib/orchestrate/storybook.mjs';
import { findChrome } from '../scripts/e2e-smoke.mjs';
import { ROOT, tempDir } from './helpers.mjs';
import { startStaticServer } from './support/static-server.mjs';
import { storybookRoutes } from './support/serve-storybook.mjs';

const fixtureIndex = JSON.parse(readFileSync(join(ROOT, 'fixtures', 'storybook', 'index.json'), 'utf8'));
const expected = JSON.parse(readFileSync(join(ROOT, 'fixtures', 'storybook', 'expected.json'), 'utf8'));
const publicLookup = async () => [{ address: '93.184.216.34', family: 4 }];
const BASE = 'https://components.example.test/storybook/';

/** A fetch stand-in serving fixed bodies by path; anything else is 404. */
function fakeFetch(files, { redirects = {} } = {}) {
  return async href => {
    const path = new URL(href).pathname;
    if (redirects[path]) return new Response(null, { status: 302, headers: { location: redirects[path] } });
    if (!(path in files)) return new Response('not found', { status: 404 });
    return new Response(typeof files[path] === 'string' ? files[path] : JSON.stringify(files[path]), { status: 200, headers: { 'content-type': 'application/json' } });
  };
}
const discover = (files, extra = {}) => discoverStories(BASE, { fetchImpl: fakeFetch(files, extra.fetch), lookup: publicLookup, ...extra.options });

test('discovery reads index.json, keeps story entries and skips docs entries', async () => {
  const stories = await discover({ '/storybook/index.json': fixtureIndex });
  assert.deepEqual(stories.map(s => s.id), ['example-button--broken', 'example-button--clean', 'example-card--wrong-lang']);
  assert.deepEqual(stories[0], { id: 'example-button--broken', title: 'Example/Button', name: 'Broken', importPath: './src/Button.stories.js' });
});

test('story pages are iframe.html with the story id and story view mode, one URL per story', () => {
  const pages = storyPages(BASE, parseStoryIndex(fixtureIndex, 'index.json'));
  assert.equal(pages[0].url, 'https://components.example.test/storybook/iframe.html?id=example-button--broken&viewMode=story');
  assert.equal(new Set(pages.map(p => p.url)).size, pages.length);
});

test('discovery names the problem when the index is missing, empty or not Storybook-shaped', async () => {
  await assert.rejects(discover({}), error => error instanceof StorybookError && /no index\.json or stories\.json/.test(error.message));
  await assert.rejects(discover({ '/storybook/index.json': { hello: 'world' } }), /not a Storybook index/);
  await assert.rejects(discover({ '/storybook/index.json': '<html>' }), /not valid JSON/);
  await assert.rejects(discover({ '/storybook/index.json': { v: 5, entries: { a: { type: 'docs', id: 'a--docs', title: 'A', name: 'Docs' } } } }), /no story entries/);
  await assert.rejects(discover({ '/storybook/index.json': { v: 5, entries: { 'x y': { type: 'story', id: 'x y', title: 'A', name: 'B' } } } }), /not a plain Storybook id/);
});

test('a Storybook 6 stories.json is recognised when index.json is absent, and docs-only entries are skipped', async () => {
  const stories = await discover({ '/storybook/stories.json': { v: 3, stories: {
    'a--one': { id: 'a--one', title: 'A', name: 'One', kind: 'A', story: 'One' },
    'a--docs': { id: 'a--docs', title: 'A', name: 'Docs', parameters: { docsOnly: true } }
  } } });
  assert.deepEqual(stories.map(s => s.id), ['a--one']);
});

test('discovery stays on the origin and refuses private addresses unless allowed', async () => {
  const files = { '/storybook/index.json': fixtureIndex };
  await assert.rejects(discover(files, { fetch: { redirects: { '/storybook/index.json': 'https://elsewhere.example.test/index.json' } } }), /outside the audited origin/);
  const privateLookup = async () => [{ address: '10.0.0.5', family: 4 }];
  await assert.rejects(discover(files, { options: { lookup: privateLookup } }), /private address/);
  assert.equal((await discover(files, { options: { lookup: privateLookup, allowPrivate: true } })).length, 3);
  await assert.rejects(discoverStories('http://localhost:6006/', { fetchImpl: fakeFetch(files) }), /local address/);
  assert.deepEqual(['127.0.0.1', '10.1.2.3', '192.168.0.1', '172.16.0.1', '169.254.1.1', '::1', 'fd00::1', '::ffff:10.0.0.1'].map(isPrivateAddress), Array(8).fill(true));
  assert.deepEqual(['93.184.216.34', '2606:2800:220:1::1'].map(isPrivateAddress), [false, false]);
});

test('the story sampler uses the same generator as the site sampler', () => {
  // Reference vectors from crates/diligent-inventory/tests/prng.rs.
  const rng = rngFromSeed('diligent');
  assert.deepEqual([rng.next(), rng.next(), rng.next()], [11815162611370927224n, 3896702070645662550n, 15025632453180051863n]);
});

const synthetic = Array.from({ length: 30 }, (_, i) => ({ id: `lib-c${i % 6}--s${i}`, title: `Lib/C${i % 6}`, name: `S${i}`, importPath: null }));
const build = (stories, pages, seed = 'seed-a') => storybookInventory({ baseUrl: BASE, stories, runId: 'run-2026-10-20-abc123', auditId: 'sb', seed, pages });

test('sampling takes the first story of each component, then a seeded draw, within the cap', async () => {
  const small = build(parseStoryIndex(fixtureIndex, 'index.json'), 10);
  assert.equal(small.sampled, 3);
  assert.deepEqual(small.sample.structured.map(s => s.url.split('id=')[1].split('&')[0]), ['example-button--broken', 'example-card--wrong-lang']);
  assert.equal(small.sample.partial, false);
  const capped = build(parseStoryIndex(fixtureIndex, 'index.json'), 1);
  assert.equal(capped.sampled, 1);
  assert.equal(capped.sample.partial, true);

  const big = build(synthetic, 10);
  assert.equal(big.total, 30);
  assert.equal(big.sampled, 10);
  assert.equal(big.sample.structured.length, 6);
  assert.equal(big.sample.random.length, 4);
  assert.equal(new Set(big.selected.map(s => s.id)).size, 10);
  assert.deepEqual(big.selected.map(s => s.id), build(synthetic, 10).selected.map(s => s.id), 'same seed, same sample');
  assert.notDeepEqual(big.selected.map(s => s.id), build(synthetic, 10, 'seed-b').selected.map(s => s.id), 'a different seed draws differently');
  const record = big.sample.extensions['diligent.storybook'];
  assert.equal(record.total_stories, 30);
  assert.equal(record.sampled_stories, 10);
  for (const [schema, doc, name] of [['inventory', big.inventory, 'INVENTORY.json'], ['sample', big.sample, 'SAMPLE.json']]) {
    const check = await validateData(schema, doc, name);
    assert.ok(check.ok, check.errors.join('; '));
  }
});

test('element-level check ids are read from the checks catalog, not hard-coded', () => {
  assert.deepEqual([...elementCheckIds({ checks: [{ id: 'A', scope: 'element' }, { id: 'B', scope: 'document' }] })], ['A']);
  assert.equal(elementCheckIds({ checks: [{ id: 'A' }] }), null, 'a binary without scope is refused');
  assert.equal(elementCheckIds({}), null);
});

// End to end, with the real binary and a real browser. Skipped with a stated reason when either is missing.
const binary = process.env.DILIGENT_RS_BIN || '';
const chrome = findChrome();
const skipReason = !binary || !existsSync(binary) || !statSync(binary).isFile()
  ? 'skipped: DILIGENT_RS_BIN is not set to a built diligent-rs'
  : !chrome ? 'skipped: no Chrome found (set DILIGENT_CHROME)' : false;

function capture(args, env) {
  return new Promise(resolve => {
    let stdout = ''; let stderr = '';
    const child = spawn(process.execPath, args, { cwd: ROOT, env, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', c => { stdout += c; });
    child.stderr.on('data', c => { stderr += c; });
    child.on('close', code => resolve({ code: code ?? 1, stdout, stderr }));
  });
}

test('end to end: the component gate fails the broken story, records whole-page checks as not tested, and is not a site claim', { skip: skipReason, timeout: 300000 }, async () => {
  const server = await startStaticServer(storybookRoutes());
  const workspace = await tempDir('diligent-storybook-');
  try {
    const run = await capture(['bin/diligent.mjs', 'audit', '--storybook', `${server.origin}/`, 'sb', '--allow-private', '--pages', '5',
      '--facts', 'references/facts/us-federal-public-web.example.json', '--root', workspace, '--json'], { ...process.env, DILIGENT_RS_BIN: binary, DILIGENT_CHROME: chrome });
    assert.equal(run.code, expected.gate_exit, `${run.stdout}\n${run.stderr.slice(-2000)}`);
    const summary = JSON.parse(run.stdout);
    assert.equal(summary.component_gate.state, 'fail');
    assert.equal(summary.component_gate.exit, 1);
    assert.match(summary.component_gate.line, /^Component gate: fail \(3 stories, \d+ failed, exit 1\)\. Not a site claim\.$/);
    assert.ok(summary.stages.find(s => s.name === 'infra').state === 'skipped', 'infra is skipped for a Storybook');

    const dir = join(workspace, '.diligent', 'sb');
    const baseline = JSON.parse(await readFile(join(dir, 'COMPONENT-BASELINE.json'), 'utf8'));
    const check = await validateData('component-baseline', baseline, 'COMPONENT-BASELINE.json');
    assert.ok(check.ok, check.errors.join('; '));
    const info = baseline.extensions['diligent.storybook'];
    assert.equal(info.total_stories, expected.stories);
    const byId = Object.fromEntries(info.stories.map(s => [s.id, s]));
    assert.equal(byId[expected.failing_story].failed, true, 'the broken story is named as failing');
    for (const id of expected.failing_checks_include) assert.ok(byId[expected.failing_story].failed_checks.includes(id), `${id} fails on the broken story`);
    assert.equal(byId[expected.also_failing_story].failed, true, 'the wrong-lang story is named as failing');
    assert.equal(byId[expected.clean_story].failed, false, 'the clean story does not fail');
    assert.equal(new Set(info.stories.map(s => s.final_url)).size, expected.stories, 'each story has its own final URL');

    const results = (await readFile(join(dir, 'RESULTS.jsonl'), 'utf8')).split('\n').filter(Boolean).map(line => JSON.parse(line));
    const document = new Set(['BRW-BYPASS-BLOCKS', 'BRW-FED-BANNER', 'BRW-FED-DAP', 'BRW-REFLOW-320', 'BRW-LINK-SAME-NAME']);
    const rows = results.filter(row => document.has(row.check_id));
    assert.ok(rows.length >= document.size * expected.stories, 'every story has a row per whole-page check');
    for (const row of rows) {
      assert.equal(row.status, 'not_tested', row.check_id);
      assert.ok(row.limitations.includes('component mode: whole-page obligation'), row.check_id);
    }
    assert.equal(new Set(results.map(row => row.instance_id.split('::')[0])).size, expected.stories, 'instance ids are per story');

    const report = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.match(report, /Component audit of/);
    assert.match(report, /not a site claim/i);
    assert.ok(report.includes(expected.failing_story));
    const run2 = JSON.parse(await readFile(join(dir, 'runs', summary.run_id, 'RUN.json'), 'utf8'));
    assert.ok(run2.modes.includes('component'));
  } finally {
    await server.close();
    await rm(workspace, { recursive: true, force: true });
  }
});

test('runComponentGate returns incomplete (exit 2) when no story failed and no applicable mandatory obligation was evaluated', async () => {
  const dir = await tempDir('diligent-component-empty-');
  try {
    await import('node:fs').then(fs => {
      fs.writeFileSync(join(dir, 'APPLICABILITY.json'), JSON.stringify({ obligations: [{ obligation_id: 'X', state: 'unknown', mandatory: true }] }));
      fs.writeFileSync(join(dir, 'RESULTS.jsonl'), JSON.stringify({ result_id: 'r1', instance_id: 'https://s.example/a::X', check_id: 'C1', obligation_id: 'X', status: 'pass' }) + '\n');
      fs.writeFileSync(join(dir, 'SAMPLE.json'), JSON.stringify({ seed: 's', method: 'wcag-em-2.0' }));
    });
    const result = await runComponentGate({
      dir, runId: 'run-2026-10-20-abcdef', auditId: 'sb', storybookUrl: 'https://s.example/',
      stories: [{ id: 'a', title: 'T', name: 'N', url: 'https://s.example/a' }],
      selected: [{ id: 'a', title: 'T', name: 'N', url: 'https://s.example/a' }],
      pageRecords: [{ url: 'https://s.example/a', collect: 'collected', assess: 'assessed' }],
      elementIds: new Set(['C1']),
      now: '2026-10-20T00:00:00Z'
    });
    assert.equal(result.state, 'incomplete');
    assert.equal(result.exit, 2);
    assert.ok(result.problems.includes('no applicable mandatory obligation was evaluated by an element-level check; supply facts so applicability is known'));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('getText cancels the stream and throws when the body exceeds maxBytes', async () => {
  let cancelled = false;
  let pulls = 0;
  const stream = new ReadableStream({
    pull(controller) {
      pulls++;
      if (pulls <= 3) controller.enqueue(new Uint8Array(50).fill(0x40 + pulls));
    },
    cancel() { cancelled = true; }
  });
  const fetchImpl = async () => new Response(stream, { status: 200, headers: { 'content-type': 'application/json' } });
  await assert.rejects(
    discoverStories(BASE, { fetchImpl, lookup: publicLookup, maxBytes: 100 }),
    error => error instanceof StorybookError && /larger than 100 bytes/.test(error.message)
  );
  assert.equal(cancelled, true, 'the stream was cancelled');
});

test('--storybook is mutually exclusive with --url and --replay', async () => {
  const { run } = await import('./helpers.mjs');
  const both = await run('scripts/audit.mjs', ['--storybook', 'https://a.example/', '--url', 'https://b.example/']);
  assert.equal(both.code, 2);
  assert.match(both.stderr, /mutually exclusive/);
});
