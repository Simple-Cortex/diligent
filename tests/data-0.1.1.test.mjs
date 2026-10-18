import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { ROOT } from './helpers.mjs';

const PACKS_DIR = join(ROOT, 'rules', 'packs');
const REGISTRY = join(ROOT, 'authorities', 'registry.json');

async function loadPacks() {
  const files = await readdir(PACKS_DIR);
  const packs = [];
  for (const f of files) {
    const pack = JSON.parse(await readFile(join(PACKS_DIR, f), 'utf8'));
    packs.push({ fileName: f, pack });
  }
  return packs;
}

async function loadRegistry() {
  return JSON.parse(await readFile(REGISTRY, 'utf8'));
}

test('every a11y-wcag20 rule has three wcag_mappings with legal_floor, title_ii_target and recommended roles', async () => {
  const packs = await loadPacks();
  const wcag20 = packs.find(p => p.pack.pack_id === 'a11y-wcag20-aa');
  assert.ok(wcag20, 'a11y-wcag20-aa pack must exist');
  for (const rule of wcag20.pack.rules) {
    if (!rule.wcag_mappings) continue;
    assert.equal(rule.wcag_mappings.length, 3,
      `${rule.rule_id}: expected 3 wcag_mappings in 2.0 pack, got ${rule.wcag_mappings.length}`);
    const roles = rule.wcag_mappings.map(m => m.role).sort();
    assert.deepEqual(roles, ['legal_floor', 'recommended', 'title_ii_target'],
      `${rule.rule_id}: roles must be legal_floor, title_ii_target, recommended`);
    const versions = rule.wcag_mappings.map(m => m.version);
    assert.ok(versions.includes('2.0'), `${rule.rule_id}: must include version 2.0`);
    assert.ok(versions.includes('2.1'), `${rule.rule_id}: must include version 2.1`);
    assert.ok(versions.includes('2.2'), `${rule.rule_id}: must include version 2.2`);
  }
});

test('every a11y-wcag21 rule has two wcag_mappings with title_ii_target and recommended roles', async () => {
  const packs = await loadPacks();
  const wcag21 = packs.find(p => p.pack.pack_id === 'a11y-wcag21-aa');
  assert.ok(wcag21, 'a11y-wcag21-aa pack must exist');
  for (const rule of wcag21.pack.rules) {
    if (!rule.wcag_mappings) continue;
    assert.equal(rule.wcag_mappings.length, 2,
      `${rule.rule_id}: expected 2 wcag_mappings in 2.1 pack, got ${rule.wcag_mappings.length}`);
    const roles = rule.wcag_mappings.map(m => m.role).sort();
    assert.deepEqual(roles, ['recommended', 'title_ii_target'],
      `${rule.rule_id}: roles must be title_ii_target, recommended`);
  }
});

test('every a11y-wcag22 rule has one wcag_mapping with recommended role', async () => {
  const packs = await loadPacks();
  const wcag22 = packs.find(p => p.pack.pack_id === 'a11y-wcag22-aa');
  assert.ok(wcag22, 'a11y-wcag22-aa pack must exist');
  for (const rule of wcag22.pack.rules) {
    if (!rule.wcag_mappings) continue;
    assert.equal(rule.wcag_mappings.length, 1,
      `${rule.rule_id}: expected 1 wcag_mapping in 2.2 pack, got ${rule.wcag_mappings.length}`);
    assert.equal(rule.wcag_mappings[0].role, 'recommended',
      `${rule.rule_id}: role must be recommended`);
    assert.equal(rule.wcag_mappings[0].version, '2.2',
      `${rule.rule_id}: version must be 2.2`);
  }
});

test('every a11y-wcag20 rule with wcag_mappings has a baseline_test_id matching the expected pattern', async () => {
  const packs = await loadPacks();
  const wcag20 = packs.find(p => p.pack.pack_id === 'a11y-wcag20-aa');
  assert.ok(wcag20, 'a11y-wcag20-aa pack must exist');
  const pattern = /^(0[1-9]|1[0-9]|2[0-4])$/;
  for (const rule of wcag20.pack.rules) {
    if (!rule.wcag_mappings) continue;
    assert.ok(rule.baseline_test_id,
      `${rule.rule_id}: 2.0 rule with wcag_mappings must have baseline_test_id`);
    assert.match(rule.baseline_test_id, pattern,
      `${rule.rule_id}: baseline_test_id "${rule.baseline_test_id}" must match ^(0[1-9]|1[0-9]|2[0-4])$`);
  }
});

test('A61 transition dates are 2027-05-11 and 2028-05-10', async () => {
  const reg = await loadRegistry();
  const a61 = reg.authorities.find(a => a.authority_id === 'A61');
  assert.ok(a61, 'A61 must exist in registry');
  assert.equal(a61.transition_dates.length, 2, 'A61 must have exactly 2 transition dates');
  const dates = a61.transition_dates.map(t => t.date).sort();
  assert.deepEqual(dates, ['2027-05-11', '2028-05-10'],
    'A61 transition dates must be 2027-05-11 and 2028-05-10');
});

test('A14 superseded_by is A64', async () => {
  const reg = await loadRegistry();
  const a14 = reg.authorities.find(a => a.authority_id === 'A14');
  assert.ok(a14, 'A14 must exist in registry');
  assert.ok(Array.isArray(a14.superseded_by), 'A14.superseded_by must be an array');
  assert.ok(a14.superseded_by.includes('A64'),
    'A14.superseded_by must include A64');
});

test('no authority_id is duplicated in the registry', async () => {
  const reg = await loadRegistry();
  const ids = reg.authorities.map(a => a.authority_id);
  const unique = new Set(ids);
  assert.equal(ids.length, unique.size,
    `duplicate authority_ids found: ${ids.filter((id, i) => ids.indexOf(id) !== i).join(', ')}`);
});

test('FED rules with standards_status have a non-empty acceptance_criteria in extensions', async () => {
  const packs = await loadPacks();
  const fed = packs.find(p => p.pack.pack_id === 'federal-public-web');
  assert.ok(fed, 'federal-public-web pack must exist');
  for (const rule of fed.pack.rules) {
    if (!rule.standards_status) continue;
    const ac = rule.extensions?.acceptance_criteria;
    assert.ok(ac && typeof ac === 'string' && ac.trim().length > 0,
      `${rule.rule_id}: standards_status "${rule.standards_status}" requires non-empty extensions.acceptance_criteria`);
  }
});
