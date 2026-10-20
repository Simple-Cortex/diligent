/** Every script the bin dispatcher can run, and everything those scripts import, must ship. */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, join, relative } from 'node:path';
import { loadManifest, patternToRegex } from '../scripts/lib/manifest.mjs';
import { ROOT } from './helpers.mjs';

const matches = (patterns, path) => patterns.some(pattern => patternToRegex(pattern).test(path));

/** Script file names from the UTILITIES map of bin/diligent.mjs. */
async function dispatchedScripts() {
  const source = await readFile(join(ROOT, 'bin/diligent.mjs'), 'utf8');
  const block = source.match(/const UTILITIES = \{([\s\S]*?)\};/);
  assert.ok(block, 'UTILITIES map not found in bin/diligent.mjs');
  const scripts = [...block[1].matchAll(/:\s*'([^']+\.mjs)'/g)].map(match => `scripts/${match[1]}`);
  assert.ok(scripts.length >= 10, 'UTILITIES map parsed to too few scripts');
  return scripts;
}

/** Static relative imports and re-exports of one file, as repository-relative paths. */
async function relativeImports(file) {
  const source = await readFile(join(ROOT, file), 'utf8');
  const found = [];
  for (const match of source.matchAll(/(?:import|export)\s[^'"]*?from\s*'(\.[^']+)'|import\s*'(\.[^']+)'|import\(\s*'(\.[^']+)'\s*\)/g)) {
    const spec = match[1] ?? match[2] ?? match[3];
    found.push(relative(ROOT, join(ROOT, dirname(file), spec)));
  }
  return found;
}

test('dispatched scripts and their relative imports are in package.json files and the manifest include set', async () => {
  const pkg = JSON.parse(await readFile(join(ROOT, 'package.json'), 'utf8'));
  const manifest = await loadManifest(ROOT);
  const seen = new Set();
  const queue = await dispatchedScripts();
  while (queue.length) {
    const file = queue.shift();
    if (seen.has(file)) continue;
    seen.add(file);
    queue.push(...await relativeImports(file));
  }
  const missingFromFiles = [...seen].filter(file => !matches(pkg.files, file)).sort();
  const missingFromManifest = [...seen].filter(file => !matches(manifest.include, file)).sort();
  assert.deepEqual(missingFromFiles, [], 'reachable files missing from package.json "files"');
  assert.deepEqual(missingFromManifest, [], 'reachable files missing from runtime-manifest.json "include"');
});
