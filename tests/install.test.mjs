import { test } from 'node:test';
import assert from 'node:assert/strict';
import { lstat, mkdir, readFile, readlink, readdir, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { includedFiles, loadManifest } from '../scripts/lib/manifest.mjs';
import { ROOT, VERSION, run, withTemp } from './helpers.mjs';

const install = (args, opts) => run('scripts/install.mjs', args, opts);

test('--yes installs exactly the runtime-manifest include set and nothing else', async () => {
  await withTemp(async tmp => {
    const dest = join(tmp, 'skills', 'diligent');
    const result = await install(['--dest', dest, '--yes']);
    assert.equal(result.code, 0, result.stderr);
    const expected = await includedFiles(ROOT, await loadManifest(ROOT));
    const walk = async (dir, prefix = '') => (await Promise.all((await readdir(join(dir, prefix), { withFileTypes: true })).map(async e => (e.isDirectory() ? walk(dir, `${prefix}${e.name}/`) : [`${prefix}${e.name}`])))).flat();
    assert.deepEqual((await walk(dest)).sort(), expected);
    for (const forbidden of ['tests', 'fixtures', 'crates', 'evals', 'Cargo.toml', 'DEVLOG.md']) {
      await assert.rejects(() => stat(join(dest, forbidden)), forbidden);
    }
    await stat(join(dest, 'bin', 'diligent.mjs'));
    await stat(join(dest, 'schemas', 'run.schema.json'));
    const version = await run('bin/diligent.mjs', ['--version'], { cwd: dest }).then(() => import('node:child_process').then(({ execFileSync }) => execFileSync(process.execPath, [join(dest, 'bin', 'diligent.mjs'), '--version'], { encoding: 'utf8' })));
    assert.equal(version.trim(), VERSION);
  });
});

test('the default destination is ~/.agents/skills/diligent under --home', async () => {
  await withTemp(async home => {
    const result = await install(['--home', home, '--yes']);
    assert.equal(result.code, 0, result.stderr);
    await stat(join(home, '.agents', 'skills', 'diligent', 'SKILL.md'));
  });
});

test('--link-claude links ~/.claude/skills/diligent relative to the default install', async () => {
  await withTemp(async home => {
    const result = await install(['--home', home, '--yes', '--link-claude']);
    assert.equal(result.code, 0, result.stderr);
    const link = join(home, '.claude', 'skills', 'diligent');
    assert.equal((await lstat(link)).isSymbolicLink(), true);
    assert.equal(await readlink(link), join('..', '..', '.agents', 'skills', 'diligent'));
    await stat(join(link, 'SKILL.md'));
    const again = await install(['--home', home, '--yes', '--link-claude']);
    assert.equal(again.code, 0);
    assert.match(again.stdout, /already links/);
  });
});

test('--link-claude refuses to replace a real directory or a foreign link', async () => {
  await withTemp(async home => {
    await mkdir(join(home, '.claude', 'skills', 'diligent'), { recursive: true });
    const result = await install(['--home', home, '--yes', '--link-claude', '--force']);
    assert.equal(result.code, 2);
    assert.match(result.stderr, /is not a symlink; not replacing it/);
  });
});

test('it refuses to overwrite a newer installed version without --force', async () => {
  await withTemp(async tmp => {
    const dest = join(tmp, 'diligent');
    await mkdir(dest, { recursive: true });
    await writeFile(join(dest, 'package.json'), JSON.stringify({ name: 'diligent', version: '9.9.9' }));
    const refused = await install(['--dest', dest, '--yes']);
    assert.equal(refused.code, 2);
    assert.ok(refused.stderr.includes(`newer than ${VERSION}`));
    assert.equal(JSON.parse(await readFile(join(dest, 'package.json'), 'utf8')).version, '9.9.9');
    const forced = await install(['--dest', dest, '--yes', '--force']);
    assert.equal(forced.code, 0, forced.stderr);
    assert.equal(JSON.parse(await readFile(join(dest, 'package.json'), 'utf8')).version, VERSION);
  });
});

test('it refuses a destination that is not a Diligent install, and leaves it untouched', async () => {
  await withTemp(async tmp => {
    const dest = join(tmp, 'other');
    await mkdir(dest, { recursive: true });
    await writeFile(join(dest, 'precious.txt'), 'keep me');
    const result = await install(['--dest', dest, '--yes']);
    assert.equal(result.code, 2);
    assert.match(result.stderr, /is not a Diligent install/);
    assert.equal(await readFile(join(dest, 'precious.txt'), 'utf8'), 'keep me');
  });
});

test('reinstalling the same or an older version replaces files cleanly', async () => {
  await withTemp(async tmp => {
    const dest = join(tmp, 'diligent');
    assert.equal((await install(['--dest', dest, '--yes'])).code, 0);
    await writeFile(join(dest, 'stale-extra.txt'), 'leftover');
    assert.equal((await install(['--dest', dest, '--yes'])).code, 0);
    await assert.rejects(() => stat(join(dest, 'stale-extra.txt')), 'a reinstall removes files that are no longer part of the skill');
    assert.deepEqual((await readdir(tmp)).sort(), ['diligent'], 'no staging or backup directories remain');
  });
});

test('--dry-run changes nothing', async () => {
  await withTemp(async tmp => {
    const dest = join(tmp, 'diligent');
    const result = await install(['--dest', dest, '--dry-run']);
    assert.equal(result.code, 0);
    assert.ok(result.stdout.includes(`Would install Diligent ${VERSION}`));
    await assert.rejects(() => stat(dest));
  });
});

test('without --yes a non-interactive run is refused', async () => {
  await withTemp(async tmp => {
    const result = await install(['--dest', join(tmp, 'diligent')]);
    assert.equal(result.code, 2);
    assert.match(result.stderr, /needs confirmation/);
    await assert.rejects(() => stat(join(tmp, 'diligent')));
  });
});

test('it will not install into the package directory itself', async () => {
  const result = await install(['--dest', ROOT, '--yes']);
  assert.equal(result.code, 2);
  const inside = await install(['--dest', join(ROOT, 'nested'), '--yes']);
  assert.equal(inside.code, 2);
});
