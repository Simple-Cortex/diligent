#!/usr/bin/env node
/** diligent validate-skill: runtime drift checks for the packaged skill. */
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { PACKAGE_ROOT, pathExists } from './lib/artifacts.mjs';
import { isMain, parseArgs, runMain } from './lib/cli.mjs';
import { expandInclude, loadManifest, matchesInclude } from './lib/manifest.mjs';

const HELP = `Usage: diligent validate-skill [--root <package-directory>] [--json]

Checks the packaged skill for drift:
  - SKILL.md frontmatter name is "diligent"
  - each skills/<dir>/SKILL.md frontmatter name is "diligent-<dir>"
  - every relative path referenced in SKILL.md and the subskills exists and is
    part of the runtime manifest (a path that would not ship is a broken link)
  - package.json version == runtime-manifest.json version == top released
    CHANGELOG.md version
  - every runtime-manifest include pattern resolves to at least one file
Exit 0 when clean, 3 when any check fails.`;

const PATH_PATTERN = /(?<![\w./-])((?:references|skills|schemas|authorities|rules|profiles|prompts|docs|scripts|bin)\/[A-Za-z0-9_.\-/*<>]+)/g;

export function frontmatter(text) {
  const match = /^---\n([\s\S]*?)\n---/.exec(text);
  if (!match) return null;
  const fields = {};
  for (const line of match[1].split('\n')) {
    const field = /^([A-Za-z_-]+):\s*(.*)$/.exec(line);
    if (field) fields[field[1]] = field[2].trim().replace(/^"(.*)"$/, '$1').replace(/^'(.*)'$/, '$1');
  }
  return fields;
}

export function referencedPaths(text) {
  const paths = new Set();
  for (const match of text.matchAll(PATH_PATTERN)) {
    const path = match[1].replace(/[.,;:)\]]+$/, '');
    if (/[<>*]/.test(path)) continue;
    paths.add(path);
  }
  return [...paths].sort();
}

export async function validateSkill(root) {
  const errors = [];
  const fail = message => errors.push(message);
  let manifest = null;
  try { manifest = await loadManifest(root); } catch (error) { fail(`runtime-manifest.json: ${error.message}`); }

  const skillPath = join(root, 'SKILL.md');
  let skillText = null;
  if (!(await pathExists(skillPath))) fail('SKILL.md: file is missing');
  else {
    skillText = await readFile(skillPath, 'utf8');
    const fm = frontmatter(skillText);
    if (!fm) fail('SKILL.md: missing frontmatter');
    else if (fm.name !== 'diligent') fail(`SKILL.md: frontmatter name is "${fm.name}", expected "diligent"`);
  }

  const documents = skillText === null ? [] : [['SKILL.md', skillText]];
  const skillsDir = join(root, 'skills');
  if (await pathExists(skillsDir)) {
    for (const entry of (await readdir(skillsDir, { withFileTypes: true })).filter(e => e.isDirectory()).sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const rel = `skills/${entry.name}/SKILL.md`;
      if (!(await pathExists(join(root, rel)))) { fail(`${rel}: file is missing`); continue; }
      const text = await readFile(join(root, rel), 'utf8');
      documents.push([rel, text]);
      const fm = frontmatter(text);
      if (!fm) fail(`${rel}: missing frontmatter`);
      else if (fm.name !== `diligent-${entry.name}`) fail(`${rel}: frontmatter name is "${fm.name}", expected "diligent-${entry.name}"`);
    }
  } else fail('skills/: directory is missing');

  for (const [file, text] of documents) {
    for (const path of referencedPaths(text)) {
      if (!(await pathExists(join(root, path)))) fail(`${file}: references ${path}, which does not exist`);
      else if (manifest && !matchesInclude(manifest, path)) fail(`${file}: references ${path}, which exists but is not in the runtime manifest, so an installed skill would have a broken link`);
    }
  }

  let packageVersion = null;
  try { packageVersion = JSON.parse(await readFile(join(root, 'package.json'), 'utf8')).version; } catch (error) { fail(`package.json: ${error.message}`); }
  if (manifest && packageVersion && manifest.version !== packageVersion) fail(`version mismatch: package.json ${packageVersion} vs runtime-manifest.json ${manifest.version}`);
  const changelogPath = join(root, 'CHANGELOG.md');
  if (!(await pathExists(changelogPath))) fail('CHANGELOG.md: file is missing');
  else {
    const released = [...(await readFile(changelogPath, 'utf8')).matchAll(/^##\s+\[(\d+\.\d+\.\d+)\]/gm)].map(m => m[1]);
    if (released.length === 0) fail('CHANGELOG.md: no released version heading like "## [0.1.0] - 2026-10-17" found');
    else if (packageVersion && released[0] !== packageVersion) fail(`version mismatch: package.json ${packageVersion} vs top released CHANGELOG.md version ${released[0]}`);
  }

  if (manifest) {
    for (const [pattern, files] of await expandInclude(root, manifest)) {
      if (files.length === 0) fail(`runtime-manifest.json: include pattern "${pattern}" matches no file`);
    }
  }
  return { ok: errors.length === 0, errors };
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2), { root: 'string', json: 'boolean' });
  if (options.help) { console.log(HELP); return 0; }
  const result = await validateSkill(options.root || PACKAGE_ROOT);
  if (options.json) console.log(JSON.stringify(result, null, 2));
  else if (result.ok) console.log('validate-skill: OK. Skill frontmatter, referenced paths, versions and manifest globs are consistent.');
  else {
    console.error(`validate-skill: INVALID. ${result.errors.length} problem${result.errors.length === 1 ? '' : 's'}:`);
    for (const line of result.errors) console.error(`  ${line}`);
  }
  return result.ok ? 0 : 3;
}

if (isMain(import.meta.url)) runMain(main);
