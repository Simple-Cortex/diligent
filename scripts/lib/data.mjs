/** Loading of shipped data files (authorities, rule packs, profiles). */
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { sha256, pathExists } from './artifacts.mjs';

export async function readJsonFile(path) {
  const bytes = await readFile(path);
  let json;
  try { json = JSON.parse(bytes.toString('utf8')); } catch {
    throw new Error(`${path}: not valid JSON`);
  }
  return { path, json, sha256: sha256(bytes) };
}

export async function listJsonFiles(dir) {
  if (!(await pathExists(dir))) return [];
  return (await readdir(dir)).filter(name => name.endsWith('.json')).sort().map(name => join(dir, name));
}

export async function loadRegistry(dataRoot) {
  return readJsonFile(join(dataRoot, 'authorities', 'registry.json'));
}

export async function loadProfile(dataRoot, profileId) {
  return readJsonFile(join(dataRoot, 'profiles', `${profileId}.json`));
}

export async function loadPack(dataRoot, packId) {
  return readJsonFile(join(dataRoot, 'rules', 'packs', `${packId}.json`));
}

/** True when every channel is unsupported. */
export function isUnsupported(channelSupport) {
  const values = Object.values(channelSupport || {});
  return values.length > 0 && values.every(value => value === 'unsupported');
}
