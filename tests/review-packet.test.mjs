import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';
import { ROOT, copyTo, run, withTemp } from './helpers.mjs';

const FIXTURE = join(ROOT, 'fixtures', 'review-packet');
const SCRIPT = 'scripts/review-packet.mjs';

async function generate(dir, extra = []) {
  const out = join(dir, 'packet.md');
  const result = await run(SCRIPT, ['generate', '--out', out, '--data-root', dir, ...extra]);
  return { result, out };
}

test('generate writes one row per informed rule with counts and statistics', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const { result, out } = await generate(dir, ['--catalog', join(dir, 'catalog.json')]);
    assert.equal(result.code, 0, result.stderr);
    const text = await readFile(out, 'utf8');
    assert.match(text, /Catalog: file .*catalog\.json \(binary subcommand not used\)/);
    const rows = text.split('\n').filter(l => /^\| `FX-/.test(l));
    assert.deepEqual(rows.map(r => r.split('|')[1].trim()), ['`FX-1`', '`FX-2`', '`FX-4`']);
    const fx1 = rows[0];
    assert.match(fx1, /\| fixture-pack \| research \| A01 Fixture Authority One \\\| with a pipe \| Clause FX-1 \| shall \| 05 \|/);
    assert.match(fx1, /`BRW-ONE` \(browser\), `BRW-TWO` \(browser\)/);
    assert.match(fx1, /`aaa111`/);
    // BRW-ONE: 2 ACT cases (skipped excluded) + 1 authored; BRW-TWO: 1 authored.
    assert.match(fx1, /\| 4 \| 1\.00 \| 0\.50 \| research \|$/);
    const fx2 = rows[1];
    assert.match(fx2, /`SRC-ONE` \(source\)/);
    assert.match(fx2, /\| 4 \|/);
    assert.match(rows[2], /`INF-ONE` \(network\)/);
    assert.match(text, /Checks inform rule ids that no pack defines: `FX-GHOST`/);
    assert.equal(text.split('\n').filter(l => l.startsWith('- [ ] ')).length, 3);
    assert.doesNotMatch(text, /FX-3/, 'a rule no check informs is not in the packet');
  });
});

test('missing manifests and metrics are named in the header and shown as n/a', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const { rm } = await import('node:fs/promises');
    await rm(join(dir, 'docs'), { recursive: true });
    await rm(join(dir, 'fixtures'), { recursive: true });
    await copyTo(join(FIXTURE, 'catalog.json'), join(dir, 'catalog.json'));
    const { result, out } = await generate(dir, ['--catalog', join(dir, 'catalog.json')]);
    assert.equal(result.code, 0, result.stderr);
    const text = await readFile(out, 'utf8');
    assert.match(text, /Missing inputs, shown as n\/a: .*docs\/a11y-check-metrics\.md/);
    assert.match(text, /\| 0 \| n\/a \| n\/a \| research \|$/m);
  });
});

test('without a catalog file and without a binary, generate fails with a usage error', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const { result } = await generate(dir);
    assert.equal(result.code, 2);
    assert.match(result.stderr, /--catalog <file\.json>/);
  });
});

test('promote changes only lifecycle and source_review_status of the listed rules', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const path = join(dir, 'rules', 'packs', 'fixture-pack.json');
    const before = await readFile(path, 'utf8');
    const result = await run(SCRIPT, ['promote', '--rules', 'FX-1,FX-2', '--data-root', dir]);
    assert.equal(result.code, 0, result.stderr);
    assert.match(result.stdout, /npm run validate/);
    const after = await readFile(path, 'utf8');
    const a = JSON.parse(before);
    const b = JSON.parse(after);
    for (const [i, rule] of a.rules.entries()) {
      const changed = ['FX-1', 'FX-2'].includes(rule.rule_id);
      const expected = changed ? { ...rule, lifecycle: 'draft', source_review_status: 'reviewed' } : rule;
      assert.deepEqual(b.rules[i], expected);
      assert.deepEqual(Object.keys(b.rules[i]), Object.keys(rule), 'key order is kept');
    }
    assert.equal(after, JSON.stringify(b, null, 2) + '\n');
    const lines = before.split('\n').length;
    assert.equal(after.split('\n').length, lines, 'no lines added or removed');
  });
});

test('promote refuses unknown ids and non-research rules and writes nothing', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const path = join(dir, 'rules', 'packs', 'fixture-pack.json');
    const before = await readFile(path, 'utf8');
    for (const ids of ['FX-1,NOPE', 'FX-3', 'FX-1,FX-3,NOPE']) {
      const result = await run(SCRIPT, ['promote', '--rules', ids, '--data-root', dir]);
      assert.equal(result.code, 2, ids);
      assert.match(result.stderr, /refusing to promote/);
      assert.equal(await readFile(path, 'utf8'), before, `${ids}: nothing written`);
    }
    const none = await run(SCRIPT, ['promote', '--data-root', dir]);
    assert.equal(none.code, 2);
  });
});

test('promote refuses a pack it could not rewrite byte for byte', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const path = join(dir, 'rules', 'packs', 'fixture-pack.json');
    const { writeFile } = await import('node:fs/promises');
    const compact = JSON.stringify(JSON.parse(await readFile(path, 'utf8')));
    await writeFile(path, compact);
    const result = await run(SCRIPT, ['promote', '--rules', 'FX-1', '--data-root', dir]);
    assert.equal(result.code, 2);
    assert.match(result.stderr, /not formatted as 2-space JSON/);
    assert.equal(await readFile(path, 'utf8'), compact);
  });
});

test('promote refuses a rule whose authority.section is still unatomized', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const path = join(dir, 'rules', 'packs', 'fixture-pack.json');
    const { writeFile } = await import('node:fs/promises');
    const pack = JSON.parse(await readFile(path, 'utf8'));
    pack.rules[0].authority.section = 'unatomized: clause locator pending source review';
    const text = JSON.stringify(pack, null, 2) + '\n';
    await writeFile(path, text);
    const result = await run(SCRIPT, ['promote', '--rules', 'FX-1,FX-2', '--data-root', dir]);
    assert.equal(result.code, 2);
    assert.match(result.stderr, /FX-1 still has an unatomized authority\.section/);
    assert.equal(await readFile(path, 'utf8'), text, 'nothing written');
  });
});

test('apply sets exactly the listed fields and keeps the file format', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const path = join(dir, 'rules', 'packs', 'fixture-pack.json');
    const { writeFile } = await import('node:fs/promises');
    const file = join(dir, 'corr.json');
    await writeFile(file, JSON.stringify({ 'FX-1': { 'authority.section': 'New s.1', 'authority.modality': 'must' }, 'FX-2': { baseline_test_id: '07' } }));
    const before = JSON.parse(await readFile(path, 'utf8'));
    const result = await run(SCRIPT, ['apply', '--corrections', file, '--data-root', dir]);
    assert.equal(result.code, 0, result.stderr);
    const after = await readFile(path, 'utf8');
    const b = JSON.parse(after);
    assert.equal(after, JSON.stringify(b, null, 2) + '\n');
    const expected = structuredClone(before);
    expected.rules[0].authority.section = 'New s.1';
    expected.rules[0].authority.modality = 'must';
    expected.rules[1].baseline_test_id = '07';
    assert.deepEqual(b, expected);
    assert.deepEqual(Object.keys(b.rules[1]), [...Object.keys(before.rules[1]), 'baseline_test_id'], 'a new field goes last, others keep order');
  });
});

test('apply refuses unknown ids and disallowed fields and writes nothing', async () => {
  await withTemp(async dir => {
    await copyTo(FIXTURE, dir);
    const path = join(dir, 'rules', 'packs', 'fixture-pack.json');
    const before = await readFile(path, 'utf8');
    const { writeFile } = await import('node:fs/promises');
    const file = join(dir, 'corr.json');
    const cases = [
      { 'FX-1': { 'authority.section': 'ok' }, NOPE: { 'authority.section': 'x' } },
      { 'FX-1': { 'authority.section': 'ok' }, 'FX-2': { lifecycle: 'draft' } },
      { 'FX-1': { 'authority.authority_id': 'A02' } },
      { 'FX-1': { 'authority.modality': 'maybe' } },
      { 'FX-1': {} },
      [],
    ];
    for (const c of cases) {
      await writeFile(file, JSON.stringify(c));
      const result = await run(SCRIPT, ['apply', '--corrections', file, '--data-root', dir]);
      assert.equal(result.code, 2, JSON.stringify(c));
      assert.match(result.stderr, /refusing to apply/);
      assert.equal(await readFile(path, 'utf8'), before, 'nothing written');
    }
    assert.equal((await run(SCRIPT, ['apply', '--data-root', dir])).code, 2);
  });
});

test('usage errors exit 2', async () => {
  assert.equal((await run(SCRIPT, [])).code, 2);
  assert.equal((await run(SCRIPT, ['generate'])).code, 2);
  assert.equal((await run(SCRIPT, ['frobnicate'])).code, 2);
  assert.equal((await run(SCRIPT, ['--help'])).code, 0);
});
