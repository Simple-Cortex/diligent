#!/usr/bin/env node
/**
 * diligent rules validate: schemas plus cross-checks for the authority
 * registry, rule packs and profiles (blueprint 20.1 and 26.2).
 */
import { basename, join } from 'node:path';
import { PACKAGE_ROOT, validateData } from './lib/artifacts.mjs';
import { isMain, parseArgs, runMain } from './lib/cli.mjs';
import { listJsonFiles, readJsonFile } from './lib/data.mjs';

const HELP = `Usage: diligent rules validate [--data-root <dir>] [--json]

Validates authorities/registry.json, every rules/packs/*.json and every
profiles/*.json against their schemas, then cross-checks:
  - every normative rule cites an authority_id that exists in the registry
    (packs marked "normative": false, such as ux-service, are exempt; so is a rule with
    "normative": false and a non-empty extensions.authority_basis)
  - rule_id values are unique across all packs
  - pack_id matches the file name, and pack profile_ids exist
  - every profile selected_packs entry exists
  - authority supersedes / superseded_by references exist
  - a rule with lifecycle "published" needs non-empty positive and negative
    fixtures and source_review_status "reviewed" (nothing in this release may be published)
Exit 0 when valid, 3 when anything is invalid or when no data was found.`;

export async function validateRules(dataRoot) {
  const errors = [];
  const stats = { authorities: 0, packs: 0, rules: 0, profiles: 0 };
  const err = (file, message) => errors.push(`${file}: ${message}`);

  const registryPath = join(dataRoot, 'authorities', 'registry.json');
  let registry = null;
  try { registry = await readJsonFile(registryPath); } catch (error) { err('authorities/registry.json', error.code === 'ENOENT' ? 'file is missing' : error.message); }
  const authorityIds = new Set();
  if (registry) {
    const result = await validateData('authority-registry', registry.json, 'authorities/registry.json');
    errors.push(...result.errors);
    for (const authority of Array.isArray(registry.json.authorities) ? registry.json.authorities : []) {
      stats.authorities++;
      if (authorityIds.has(authority.authority_id)) err('authorities/registry.json', `duplicate authority_id ${authority.authority_id}`);
      authorityIds.add(authority.authority_id);
    }
    for (const authority of Array.isArray(registry.json.authorities) ? registry.json.authorities : []) {
      for (const field of ['supersedes', 'superseded_by', 'incorporates']) {
        for (const id of authority[field] || []) {
          if (!authorityIds.has(id)) err('authorities/registry.json', `${authority.authority_id}.${field} references unknown authority ${id}`);
        }
      }
    }
  }

  const profileFiles = await listJsonFiles(join(dataRoot, 'profiles'));
  const packFiles = await listJsonFiles(join(dataRoot, 'rules', 'packs'));
  if (profileFiles.length === 0) err('profiles/', 'no profile files found');
  if (packFiles.length === 0) err('rules/packs/', 'no rule pack files found');

  const packIds = new Set(packFiles.map(file => basename(file, '.json')));
  const profileIds = new Set(profileFiles.map(file => basename(file, '.json')));

  for (const file of profileFiles) {
    const label = `profiles/${basename(file)}`;
    let profile;
    try { profile = await readJsonFile(file); } catch (error) { err(label, error.message); continue; }
    stats.profiles++;
    errors.push(...(await validateData('profile', profile.json, label)).errors);
    if (profile.json.profile_id !== basename(file, '.json')) err(label, `profile_id "${profile.json.profile_id}" does not match the file name`);
    for (const packId of profile.json.selected_packs || []) if (!packIds.has(packId)) err(label, `selected_packs references missing pack "${packId}"`);
  }

  const seenRules = new Map();
  for (const file of packFiles) {
    const label = `rules/packs/${basename(file)}`;
    let pack;
    try { pack = await readJsonFile(file); } catch (error) { err(label, error.message); continue; }
    stats.packs++;
    errors.push(...(await validateData('rule-pack', pack.json, label)).errors);
    if (pack.json.pack_id !== basename(file, '.json')) err(label, `pack_id "${pack.json.pack_id}" does not match the file name`);
    for (const profileId of pack.json.profile_ids || []) if (!profileIds.has(profileId)) err(label, `profile_ids references missing profile "${profileId}"`);
    const normative = pack.json.normative !== false;
    for (const [index, rule] of (Array.isArray(pack.json.rules) ? pack.json.rules : []).entries()) {
      stats.rules++;
      const where = `${label} rules[${index}] ${rule?.rule_id ?? '(no rule_id)'}`;
      if (rule?.rule_id) {
        if (seenRules.has(rule.rule_id)) err(where, `duplicate rule_id, first seen in ${seenRules.get(rule.rule_id)}`);
        else seenRules.set(rule.rule_id, label);
      }
      const ruleNormative = normative && rule?.normative !== false;
      if (rule?.normative === false && !rule?.authority?.authority_id) {
        const basis = rule?.extensions?.authority_basis;
        if (typeof basis !== 'string' || basis.trim() === '') err(where, 'non-normative rule without an authority_id needs a non-empty extensions.authority_basis');
      }
      if (ruleNormative) {
        const id = rule?.authority?.authority_id;
        if (!id) err(where, 'normative rule has no authority.authority_id');
        else if (!authorityIds.has(id)) err(where, `authority_id "${id}" does not exist in authorities/registry.json`);
      }
      if (rule?.lifecycle === 'published') {
        if (!rule.fixtures?.positive?.length) err(where, 'lifecycle "published" requires non-empty fixtures.positive');
        if (!rule.fixtures?.negative?.length) err(where, 'lifecycle "published" requires non-empty fixtures.negative');
        if (rule.source_review_status !== 'reviewed') err(where, 'lifecycle "published" requires source_review_status "reviewed"');
      }
    }
  }
  return { ok: errors.length === 0, errors, stats };
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2), { 'data-root': 'string', json: 'boolean' });
  if (options.help) { console.log(HELP); return 0; }
  const dataRoot = options['data-root'] || PACKAGE_ROOT;
  const result = await validateRules(dataRoot);
  if (options.json) console.log(JSON.stringify(result, null, 2));
  else if (result.ok) {
    const s = result.stats;
    console.log(`rules validate: OK. ${s.authorities} authorities, ${s.packs} rule packs (${s.rules} rules), ${s.profiles} profiles validated.`);
  } else {
    console.error(`rules validate: INVALID. ${result.errors.length} error${result.errors.length === 1 ? '' : 's'}:`);
    for (const line of result.errors) console.error(`  ${line}`);
  }
  return result.ok ? 0 : 3;
}

if (isMain(import.meta.url)) runMain(main);
