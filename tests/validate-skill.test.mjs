import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { frontmatter, referencedPaths, validateSkill } from '../scripts/validate-skill.mjs';
import { ROOT, run, withTemp } from './helpers.mjs';

async function writeTree(dir, overrides = {}) {
  const files = {
    'SKILL.md': '---\nname: "diligent"\ndescription: "Test skill"\n---\nRead references/workflow.md and skills/init/SKILL.md. Data: profiles/p.json.\n',
    'skills/init/SKILL.md': '---\nname: diligent-init\ndescription: "Init"\n---\nSee references/workflow.md.\n',
    'references/workflow.md': '# Workflow\n',
    'profiles/p.json': '{}\n',
    'CHANGELOG.md': '# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-10-17\n- First.\n',
    'package.json': JSON.stringify({ name: 'diligent', version: '0.1.0' }),
    'runtime-manifest.json': JSON.stringify({ version: '0.1.0', include: ['SKILL.md', 'references/*.md', 'skills/*/SKILL.md', 'profiles/*.json', 'runtime-manifest.json', 'package.json', 'CHANGELOG.md'] }),
    ...overrides
  };
  for (const [path, content] of Object.entries(files)) {
    if (content === null) continue;
    await mkdir(join(dir, path, '..'), { recursive: true });
    await writeFile(join(dir, path), content);
  }
}
const problems = async overrides => withTemp(async dir => { await writeTree(dir, overrides); return (await validateSkill(dir)).errors.join('\n'); });

test('a consistent minimal tree is valid', async () => {
  await withTemp(async dir => {
    await writeTree(dir);
    const result = await validateSkill(dir);
    assert.deepEqual(result.errors, []);
  });
});

test('the real package tree is valid', async () => {
  const result = await validateSkill(ROOT);
  assert.deepEqual(result.errors, []);
});

test('SKILL.md must be named diligent', async () => {
  assert.match(await problems({ 'SKILL.md': '---\nname: "considered"\ndescription: "x"\n---\n' }), /SKILL\.md: frontmatter name is "considered", expected "diligent"/);
  assert.match(await problems({ 'SKILL.md': 'no frontmatter' }), /SKILL\.md: missing frontmatter/);
});

test('each subskill must be named diligent-<dir>', async () => {
  assert.match(await problems({ 'skills/init/SKILL.md': '---\nname: diligent-start\ndescription: "x"\n---\n' }), /skills\/init\/SKILL\.md: frontmatter name is "diligent-start", expected "diligent-init"/);
});

test('a referenced path that does not exist is reported with the referring file', async () => {
  assert.match(await problems({ 'SKILL.md': '---\nname: "diligent"\ndescription: "x"\n---\nSee references/ghost.md.\n' }), /SKILL\.md: references references\/ghost\.md, which does not exist/);
  assert.match(await problems({ 'skills/init/SKILL.md': '---\nname: diligent-init\ndescription: "x"\n---\nSee skills/nope/SKILL.md\n' }), /skills\/init\/SKILL\.md: references skills\/nope\/SKILL\.md/);
});

test('a referenced path that exists but would not ship is a broken link', async () => {
  const text = await problems({ 'docs/internal.md': '# private\n', 'SKILL.md': '---\nname: "diligent"\ndescription: "x"\n---\nSee docs/internal.md.\n' });
  assert.match(text, /docs\/internal\.md, which exists but is not in the runtime manifest/);
});

test('placeholders and globs in prose are not treated as paths', () => {
  assert.deepEqual(referencedPaths('Use skills/<cmd>/SKILL.md, rules/packs/*.json, and references/workflow.md.'), ['references/workflow.md']);
  assert.deepEqual(referencedPaths('Trailing punctuation (see references/evidence.md). Also `scripts/gate.mjs`, and rules/packs/x.json;'), ['references/evidence.md', 'rules/packs/x.json', 'scripts/gate.mjs']);
});

test('versions must agree across package.json, the manifest and the CHANGELOG', async () => {
  assert.match(await problems({ 'package.json': JSON.stringify({ name: 'diligent', version: '0.2.0' }) }), /package\.json 0\.2\.0 vs runtime-manifest\.json 0\.1\.0/);
  assert.match(await problems({ 'CHANGELOG.md': '## [Unreleased]\n\n## [0.0.9] - 2026-01-01\n' }), /package\.json 0\.1\.0 vs top released CHANGELOG\.md version 0\.0\.9/);
  assert.match(await problems({ 'CHANGELOG.md': '# nothing released\n' }), /no released version heading/);
  assert.match(await problems({ 'CHANGELOG.md': null }), /CHANGELOG\.md: file is missing/);
});

test('an include pattern that matches no file is reported', async () => {
  const manifest = JSON.stringify({ version: '0.1.0', include: ['SKILL.md', 'references/*.md', 'skills/*/SKILL.md', 'profiles/*.json', 'prompts/*.md', 'runtime-manifest.json', 'package.json', 'CHANGELOG.md'] });
  assert.match(await problems({ 'runtime-manifest.json': manifest }), /include pattern "prompts\/\*\.md" matches no file/);
});

test('frontmatter parsing handles quotes', () => {
  assert.deepEqual(frontmatter('---\nname: "diligent"\ndescription: \'x y\'\n---\nbody'), { name: 'diligent', description: 'x y' });
  assert.equal(frontmatter('body only'), null);
});

test('the CLI exits 0 on a clean tree and 3 on a broken one', async () => {
  await withTemp(async dir => {
    await writeTree(dir);
    assert.equal((await run('scripts/validate-skill.mjs', ['--root', dir])).code, 0);
    await writeFile(join(dir, 'SKILL.md'), '---\nname: wrong\ndescription: "x"\n---\n');
    const bad = await run('scripts/validate-skill.mjs', ['--root', dir]);
    assert.equal(bad.code, 3);
    assert.match(bad.stderr, /INVALID/);
  });
});
