/** runtime-manifest.json handling: the single authority for what ships. */
import { readFile, readdir, stat } from 'node:fs/promises';
import { join } from 'node:path';

export async function loadManifest(root) {
  const manifest = JSON.parse(await readFile(join(root, 'runtime-manifest.json'), 'utf8'));
  if (!Array.isArray(manifest.include) || manifest.include.some(entry => typeof entry !== 'string')) {
    throw new Error('runtime-manifest.json must contain an "include" array of path patterns');
  }
  return manifest;
}

/** Only `*` within one path segment is supported. `**` is rejected on purpose: list files explicitly. */
export function patternToRegex(pattern) {
  if (pattern.includes('**')) throw new Error(`pattern "${pattern}": "**" is not supported; list files explicitly`);
  const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, '\\$&').replaceAll('*', '[^/]*');
  return new RegExp(`^${escaped}$`);
}

export function matchesInclude(manifest, relativePath) {
  return manifest.include.some(pattern => patternToRegex(pattern).test(relativePath));
}

async function walk(root, segments, prefix, out) {
  const [head, ...rest] = segments;
  const regex = patternToRegex(head);
  let entries;
  try { entries = await readdir(join(root, prefix)); } catch { return; }
  for (const name of entries.sort()) {
    if (!regex.test(name)) continue;
    const rel = prefix ? `${prefix}/${name}` : name;
    const info = await stat(join(root, rel));
    if (rest.length === 0) { if (info.isFile()) out.push(rel); }
    else if (info.isDirectory()) await walk(root, rest, rel, out);
  }
}

/** Expand each include pattern to the files that exist. Returns Map(pattern -> files[]). */
export async function expandInclude(root, manifest) {
  const result = new Map();
  for (const pattern of manifest.include) {
    const files = [];
    await walk(root, pattern.split('/'), '', files);
    result.set(pattern, files);
  }
  return result;
}

export async function includedFiles(root, manifest) {
  const files = new Set();
  for (const list of (await expandInclude(root, manifest)).values()) for (const file of list) files.add(file);
  return [...files].sort();
}
