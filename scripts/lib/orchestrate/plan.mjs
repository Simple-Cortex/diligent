/** Page selection, rule lookup and AUDIT-PLAN.json for `diligent audit`. */
import { loadPack, loadProfile } from '../data.mjs';
import { pageSlug } from './engine.mjs';

const PACK_ID = /^[a-z0-9-]+$/;

/** Load the profile and its packs from the data root, and index rules by obligation id. */
export async function loadProfileData(dataRoot, profileId) {
  const profile = await loadProfile(dataRoot, profileId);
  const packs = [];
  const rules = new Map();
  for (const packId of profile.json.selected_packs || []) {
    if (!PACK_ID.test(packId)) throw new Error(`profile ${profileId} names an invalid pack id`);
    const pack = (await loadPack(dataRoot, packId)).json;
    packs.push(pack);
    for (const rule of pack.rules || []) if (!rules.has(rule.obligation_id)) rules.set(rule.obligation_id, { rule, pack_id: pack.pack_id });
  }
  return { profile: profile.json, packs, rules };
}

/** Structured pages first, then random ones, deduplicated, up to `limit`. Only pages on `origin`. */
export function selectPages(sample, limit, origin) {
  const seen = new Set();
  const pages = [];
  const skipped = [];
  for (const entry of [...(sample?.structured || []), ...(sample?.random || [])]) {
    const url = entry?.url;
    if (typeof url !== 'string' || seen.has(url)) continue;
    seen.add(url);
    let pageOrigin = null;
    try { pageOrigin = new URL(url).origin; } catch { /* skipped below */ }
    if (pageOrigin !== origin) { skipped.push(url); continue; }
    if (pages.length < limit) pages.push({ url, slug: pageSlug(url), reason: entry.reason ?? 'random' });
  }
  return { pages, skipped };
}

export function buildAuditPlan({ runId, auditId, now, applicability, rules, pages, sample, budgets }) {
  const required = [];
  const manual = [];
  const documentary = [];
  for (const obligation of applicability.obligations || []) {
    if (obligation.state !== 'applicable') continue;
    const entry = rules.get(obligation.obligation_id);
    const checkIds = entry?.rule.check_ids || [];
    required.push({
      obligation_id: obligation.obligation_id,
      check_ids: checkIds,
      instances: checkIds.length ? pages.map(page => ({ instance_id: `page-${page.slug}`, target: page.url, context: { sample_reason: page.reason } })) : []
    });
    const support = entry?.rule.channel_support || {};
    if (support.human === 'human') manual.push({ obligation_id: obligation.obligation_id, rule_id: entry.rule.rule_id, owner_role: entry.rule.owner_role ?? 'reviewer' });
    if (support.documentary === 'documentary') documentary.push({ obligation_id: obligation.obligation_id, rule_id: entry.rule.rule_id, owner_role: entry.rule.owner_role ?? 'reviewer' });
  }
  return {
    schema_version: '1.0.0', run_id: runId, audit_id: auditId, computed_at: now,
    required_obligations: required,
    sampling: {
      method: sample?.method ?? 'wcag-em-2.0', seed: sample?.seed ?? null,
      rationale: `${sample?.structured?.length ?? 0} structured and ${sample?.random?.length ?? 0} random pages were sampled; ${pages.length} are collected under the budget of ${budgets.pages} pages. Pages outside the sample are untested.`
    },
    manual_work: manual, documentary_work: documentary, budgets
  };
}
