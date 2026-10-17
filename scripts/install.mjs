#!/usr/bin/env node
/** diligent install: copy the runtime-manifest file set to an agent skills directory. */
import { cp, lstat, mkdir, readFile, readlink, rename, rm, symlink, unlink } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { createInterface } from 'node:readline/promises';
import { PACKAGE_ROOT, UsageError, pathExists } from './lib/artifacts.mjs';
import { parseArgs, runMain } from './lib/cli.mjs';
import { includedFiles, loadManifest } from './lib/manifest.mjs';

const HELP = `Usage: diligent install [--dest <skill-directory>] [--link-claude] [--yes] [--force] [--dry-run] [--home <dir>]

Copies exactly the runtime-manifest include set (no tests, docs, crates or
maintainer files) to the skill directory. Default destination:
  ~/.agents/skills/diligent
--link-claude additionally links ~/.claude/skills/diligent to the install.
Without --yes an interactive terminal is asked to confirm; a non-interactive
run without --yes is refused.

Refuses to overwrite a newer installed version, or a directory that is not a
Diligent install, unless --force is given. The install is staged and swapped
into place so an interrupted copy never leaves a half-installed skill.
Nothing is downloaded and nothing outside the destination (and the optional
link) is touched.`;

function compareVersions(a, b) {
  const pa = String(a).split('.').map(Number);
  const pb = String(b).split('.').map(Number);
  for (let i = 0; i < 3; i++) { if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) < (pb[i] || 0) ? -1 : 1; }
  return 0;
}

async function installedInfo(dest) {
  try {
    const pkg = JSON.parse(await readFile(join(dest, 'package.json'), 'utf8'));
    return { isDiligent: pkg.name === 'diligent', version: pkg.version };
  } catch { return { isDiligent: false, version: null }; }
}

async function ask(question, fallback) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  try {
    const answer = (await rl.question(`${question} `)).trim().toLowerCase();
    if (answer === '') return fallback;
    return answer === 'y' || answer === 'yes';
  } finally { rl.close(); }
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2), {
    dest: 'string', 'link-claude': 'boolean', yes: 'boolean', force: 'boolean', 'dry-run': 'boolean', home: 'string'
  });
  if (options.help) { console.log(HELP); return 0; }
  const home = options.home ? resolve(options.home) : homedir();
  const dest = resolve(options.dest || join(home, '.agents', 'skills', 'diligent'));
  if (dest === PACKAGE_ROOT) throw new UsageError('the destination is this package directory itself.');
  if (relative(PACKAGE_ROOT, dest) !== '' && !relative(PACKAGE_ROOT, dest).startsWith('..')) throw new UsageError('the destination must not be inside this package directory.');

  const manifest = await loadManifest(PACKAGE_ROOT);
  const files = await includedFiles(PACKAGE_ROOT, manifest);
  const version = JSON.parse(await readFile(join(PACKAGE_ROOT, 'package.json'), 'utf8')).version;

  const present = await pathExists(dest);
  if (present) {
    const existing = await installedInfo(dest);
    if (!existing.isDiligent && !options.force) throw new UsageError(`${dest} exists and is not a Diligent install. Use --force only if replacing it is intended.`);
    if (existing.isDiligent && existing.version && compareVersions(existing.version, version) > 0 && !options.force) {
      throw new UsageError(`${dest} holds Diligent ${existing.version}, which is newer than ${version}. Use --force to downgrade.`);
    }
  }
  const linkPath = join(home, '.claude', 'skills', 'diligent');
  const defaultDest = resolve(join(home, '.agents', 'skills', 'diligent'));
  const linkTarget = dest === defaultDest ? join('..', '..', '.agents', 'skills', 'diligent') : dest;

  if (options['dry-run']) {
    console.log(`Would ${present ? 'replace' : 'install'} Diligent ${version} (${files.length} files) at ${dest}`);
    if (options['link-claude']) console.log(`Would link ${linkPath} -> ${linkTarget}`);
    return 0;
  }
  let linkClaude = Boolean(options['link-claude']);
  if (!options.yes) {
    if (!process.stdin.isTTY) throw new UsageError('install needs confirmation. Pass --yes in a non-interactive run, or --dry-run to preview.');
    if (!(await ask(`Install Diligent ${version} (${files.length} files) to ${dest}? [Y/n]`, true))) { console.log('Cancelled. Nothing was changed.'); return 0; }
    if (!linkClaude) linkClaude = await ask(`Also link ${linkPath} to it? [y/N]`, false);
  }

  const stage = `${dest}.staging-${process.pid}`;
  await rm(stage, { recursive: true, force: true });
  try {
    for (const file of files) {
      await mkdir(dirname(join(stage, file)), { recursive: true });
      await cp(join(PACKAGE_ROOT, file), join(stage, file));
    }
    await mkdir(dirname(dest), { recursive: true });
    const backup = `${dest}.previous-${process.pid}`;
    if (present) await rename(dest, backup);
    try { await rename(stage, dest); } catch (error) { if (present) await rename(backup, dest); throw error; }
    if (present) await rm(backup, { recursive: true, force: true });
  } catch (error) {
    await rm(stage, { recursive: true, force: true });
    throw error;
  }
  console.log(`Installed Diligent ${version} (${files.length} files) at ${dest}`);

  if (linkClaude) {
    await mkdir(dirname(linkPath), { recursive: true });
    let existing = null;
    try { existing = await lstat(linkPath); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (existing) {
      if (existing.isSymbolicLink() && (await readlink(linkPath)) === linkTarget) { console.log(`${linkPath} already links to the install.`); return 0; }
      if (!existing.isSymbolicLink()) throw new UsageError(`${linkPath} exists and is not a symlink; not replacing it.`);
      if (!options.force) throw new UsageError(`${linkPath} is a symlink to something else. Use --force to replace the link.`);
      await unlink(linkPath);
    }
    await symlink(linkTarget, linkPath);
    console.log(`Linked ${linkPath} -> ${linkTarget}`);
  }
  return 0;
}

runMain(main);
