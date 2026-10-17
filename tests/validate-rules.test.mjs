import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { validateRules } from '../scripts/validate-rules.mjs';
import { ROOT, run, withTemp } from './helpers.mjs';

const FIX = join(ROOT, 'fixtures');
const messages = async dir => (await validateRules(dir)).errors.join('\n');

test('the real shipped authorities, rule packs and profiles validate', async () => {
  const result = await validateRules(ROOT);
  assert.deepEqual(result.errors, []);
  assert.equal(result.ok, true);
  assert.ok(result.stats.authorities >= 20);
  assert.equal(result.stats.packs, 7);
  assert.equal(result.stats.profiles, 2);
  assert.ok(result.stats.rules > 100);
});

test('nothing shipped in 0.1.0 is published, and every shipped rule is in the research lifecycle', async () => {
  const { readFile, readdir } = await import('node:fs/promises');
  const dir = join(ROOT, 'rules', 'packs');
  for (const file of await readdir(dir)) {
    const pack = JSON.parse(await readFile(join(dir, file), 'utf8'));
    for (const rule of pack.rules) {
      assert.equal(rule.lifecycle, 'research', `${pack.pack_id} ${rule.rule_id}`);
      assert.equal(rule.source_review_status, 'research', `${pack.pack_id} ${rule.rule_id}`);
    }
  }
});

test('the fixture data root is valid', async () => {
  const result = await validateRules(join(FIX, 'package'));
  assert.deepEqual(result.errors, []);
  assert.deepEqual(result.stats, { authorities: 2, packs: 2, rules: 4, profiles: 1 });
});

test('a rule that cites a dangling authority fails and names the rule and the id', async () => {
  const text = await messages(join(FIX, 'rules', 'dangling-authority'));
  assert.match(text, /rules\/packs\/mini-core\.json rules\[0\] A11Y-1\.1\.1: authority_id "A99" does not exist/);
});

test('a published rule without fixtures or review fails', async () => {
  const text = await messages(join(FIX, 'rules', 'published-without-fixtures'));
  assert.match(text, /requires non-empty fixtures\.positive/);
  assert.match(text, /requires non-empty fixtures\.negative/);
  assert.match(text, /requires source_review_status "reviewed"/);
});

test('a published rule with fixtures but unreviewed source still fails', async () => {
  const text = await messages(join(FIX, 'rules', 'published-unreviewed'));
  assert.match(text, /requires source_review_status "reviewed"/);
  assert.doesNotMatch(text, /fixtures\./);
});

test('a published rule that meets every condition passes (the check is not a blanket ban)', async () => {
  assert.deepEqual((await validateRules(join(FIX, 'rules', 'published-complete'))).errors, []);
});

test('duplicate rule ids across packs fail', async () => {
  assert.match(await messages(join(FIX, 'rules', 'duplicate-rule-id')), /duplicate rule_id, first seen in rules\/packs\/mini-core\.json/);
});

test('a profile selecting a missing pack fails', async () => {
  assert.match(await messages(join(FIX, 'rules', 'missing-pack')), /selected_packs references missing pack "mini-ghost"/);
});

test('a pack_id that does not match its file name fails', async () => {
  assert.match(await messages(join(FIX, 'rules', 'pack-id-mismatch')), /pack_id "mini-core" does not match the file name/);
});

test('schema violations are reported with file and path', async () => {
  const text = await messages(join(FIX, 'rules', 'bad-schema'));
  assert.match(text, /rules\/packs\/mini-core\.json \/rules\/1\/obligation: required field is missing/);
  assert.match(text, /\/rules\/2\/lifecycle: must be one of/);
});

test('an unknown field is rejected, not ignored', async () => {
  assert.match(await messages(join(FIX, 'rules', 'unknown-field')), /\/rules\/0\/mystery_field: unknown field/);
});

test('a supersedes reference to a missing authority fails', async () => {
  assert.match(await messages(join(FIX, 'rules', 'dangling-supersedes')), /A02\.supersedes references unknown authority A77/);
});

test('the non-normative pack is exempt from the authority requirement, a normative pack is not', async () => {
  const valid = await validateRules(join(FIX, 'package'));
  assert.equal(valid.ok, true, 'ux rules with a null authority_id are accepted in a pack marked normative:false');
  const { readFile, writeFile } = await import('node:fs/promises');
  await withTemp(async tmp => {
    const { cp } = await import('node:fs/promises');
    await cp(join(FIX, 'package'), tmp, { recursive: true });
    const path = join(tmp, 'rules', 'packs', 'mini-ux.json');
    const pack = JSON.parse(await readFile(path, 'utf8'));
    delete pack.normative;
    await writeFile(path, JSON.stringify(pack));
    assert.match(await messages(tmp), /mini-ux\.json rules\[0\] UX-001: normative rule has no authority\.authority_id/);
  });
});

test('an empty data root is invalid rather than vacuously valid', async () => {
  await withTemp(async tmp => {
    await mkdir(tmp, { recursive: true });
    const text = await messages(tmp);
    assert.match(text, /authorities\/registry\.json: file is missing/);
    assert.match(text, /no rule pack files found/);
    assert.match(text, /no profile files found/);
  });
});

test('the CLI exits 0 for valid data and 3 for invalid data', async () => {
  const ok = await run('scripts/validate-rules.mjs', ['--data-root', join(FIX, 'package')]);
  assert.equal(ok.code, 0);
  assert.match(ok.stdout, /rules validate: OK/);
  const bad = await run('scripts/validate-rules.mjs', ['--data-root', join(FIX, 'rules', 'dangling-authority')]);
  assert.equal(bad.code, 3);
  assert.match(bad.stderr, /INVALID/);
  const json = await run('scripts/validate-rules.mjs', ['--data-root', join(FIX, 'rules', 'dangling-authority'), '--json']);
  assert.equal(JSON.parse(json.stdout).ok, false);
});

test('a rule with normative:false and an authority_basis may have a null authority_id', async () => {
  assert.deepEqual((await validateRules(join(FIX, 'rules', 'rule-non-normative-valid'))).errors, []);
});

test('a non-normative rule with a null authority_id and no authority_basis fails; normative rules still need an authority', async () => {
  assert.match(await messages(join(FIX, 'rules', 'rule-non-normative-no-basis')), /FED-01: non-normative rule without an authority_id needs a non-empty extensions\.authority_basis/);
  assert.match(await messages(join(FIX, 'rules', 'dangling-authority')), /does not exist/);
});
