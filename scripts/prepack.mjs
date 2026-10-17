#!/usr/bin/env node
/** Asserts package.json "files" equals the runtime-manifest include set. */
import { readFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { join } from 'node:path';
import { PACKAGE_ROOT } from './lib/artifacts.mjs';
import { isMain, parseArgs, runMain } from './lib/cli.mjs';
import { includedFiles, loadManifest } from './lib/manifest.mjs';

const exec = promisify(execFile);

const HELP = `Usage: node scripts/prepack.mjs [--root <package-directory>] [--tarball]

Fails when package.json "files" and runtime-manifest.json "include" differ, so
the published tarball and the installed skill can never drift apart.

--tarball also runs "npm pack --dry-run" and fails unless the files npm would
publish are exactly the files the runtime manifest includes.`;

export async function compareTarball(root) {
  const { stdout } = await exec('npm', ['pack', '--dry-run', '--json', '--ignore-scripts'], { cwd: root });
  const packed = new Set(JSON.parse(stdout)[0].files.map(file => file.path));
  const expected = new Set(await includedFiles(root, await loadManifest(root)));
  return {
    missing: [...expected].filter(f => !packed.has(f)).sort(),
    extra: [...packed].filter(f => !expected.has(f)).sort(),
    count: packed.size
  };
}

export async function comparePackageFiles(root) {
  const pkg = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
  const manifest = await loadManifest(root);
  const files = new Set(pkg.files || []);
  const include = new Set(manifest.include);
  return {
    onlyInPackage: [...files].filter(f => !include.has(f)).sort(),
    onlyInManifest: [...include].filter(f => !files.has(f)).sort()
  };
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2), { root: 'string', tarball: 'boolean' });
  if (options.help) { console.log(HELP); return 0; }
  const root = options.root || PACKAGE_ROOT;
  const { onlyInPackage, onlyInManifest } = await comparePackageFiles(root);
  if (onlyInPackage.length || onlyInManifest.length) {
    console.error('prepack: package.json "files" and runtime-manifest.json "include" differ.');
    for (const f of onlyInPackage) console.error(`  only in package.json: ${f}`);
    for (const f of onlyInManifest) console.error(`  only in runtime-manifest.json: ${f}`);
    return 1;
  }
  console.log('prepack: package.json "files" matches the runtime-manifest include set.');
  if (options.tarball) {
    const { missing, extra, count } = await compareTarball(root);
    if (missing.length || extra.length) {
      console.error('prepack: the tarball does not equal the manifest include set.');
      for (const f of missing) console.error(`  missing from tarball: ${f}`);
      for (const f of extra) console.error(`  not in manifest: ${f}`);
      return 1;
    }
    console.log(`prepack: the tarball equals the manifest include set (${count} files).`);
  }
  return 0;
}

if (isMain(import.meta.url)) runMain(main);
