import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { ROOT, VERSION, run, withTemp } from './helpers.mjs';

test('package.json, runtime-manifest.json, CHANGELOG.md and Cargo.toml versions are aligned', async () => {
  const pkg = JSON.parse(await readFile(join(ROOT, 'package.json'), 'utf8'));
  const manifest = JSON.parse(await readFile(join(ROOT, 'runtime-manifest.json'), 'utf8'));
  const changelog = await readFile(join(ROOT, 'CHANGELOG.md'), 'utf8');
  const cargoToml = await readFile(join(ROOT, 'Cargo.toml'), 'utf8');
  const installDoc = await readFile(join(ROOT, 'docs', 'install.md'), 'utf8');
  assert.equal(pkg.version, manifest.version, 'package.json and runtime-manifest.json versions match');
  const changelogMatch = changelog.match(/^## \[(\d+\.\d+\.\d+)\]/m);
  assert.ok(changelogMatch, 'CHANGELOG.md has a released version heading');
  assert.equal(pkg.version, changelogMatch[1], 'package.json version matches CHANGELOG.md top released heading');
  const cargoVersionMatch = cargoToml.match(/^version\s*=\s*"(\d+\.\d+\.\d+)"/m);
  assert.ok(cargoVersionMatch, 'Cargo.toml has a version');
  const cargoVersion = cargoVersionMatch[1];
  assert.equal(cargoVersion, pkg.version, 'Cargo workspace version equals the package version (the release workflow requires binary version == tag)');
  assert.match(installDoc, /version.*tag|tag.*version/i, 'docs/install.md mentions the version-equals-tag note');
});

async function walk(dir) {
  const out = [];
  for (const entry of await readdir(join(ROOT, dir), { withFileTypes: true })) {
    const rel = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...await walk(rel));
    else if (/\.mjs$/.test(entry.name)) out.push(rel);
  }
  return out;
}

test('no literal "Diligent 0.1." version string is hard-coded in bin/ or scripts/', async () => {
  const hits = [];
  for (const file of [...await walk('bin'), ...await walk('scripts')]) {
    (await readFile(join(ROOT, file), 'utf8')).split('\n').forEach((line, i) => {
      if (line.includes('Diligent 0.1.')) hits.push(`${file}:${i + 1}`);
    });
  }
  assert.deepEqual(hits, [], 'use DILIGENT_VERSION from scripts/lib/cli.mjs');
});

test('init and gate output carry the package version in the limitation text', async () => {
  await withTemp(async tmp => {
    const init = await run('scripts/init.mjs', ['vs', '--root', tmp, '--json']);
    assert.ok(init.stdout.includes(`Diligent ${VERSION} ships zero automated website checks`));
    assert.ok(!init.stdout.includes('Diligent 0.1.0 '));
  });
});
