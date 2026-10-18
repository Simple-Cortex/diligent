/**
 * Operator-supplied scope facts for `diligent audit`. Facts are supplied, never guessed:
 * each one lands in SCOPE.json as known, provided_by "operator", with a time.
 */
import { readFile } from 'node:fs/promises';
import { UsageError } from '../artifacts.mjs';
import { buildInitialFacts } from '../scope-facts.mjs';

const FACT_ID = /^[a-z][a-z0-9_]*$/;

/** Group a fact that the starter catalog does not carry. The group only organizes SCOPE.json. */
function groupFor(factId) {
  if (/wcag|standard/.test(factId)) return 'adopted_standards';
  if (/fisma|security|system|authenticat|nist|identity|workforce|cisa|m2618|https|mail|cloud/.test(factId)) return 'system';
  if (/personal|privacy|sensitive|records|pii|dataset|foia|information_from/.test(factId)) return 'data';
  if (/service|transaction|upload|search|esignature|attestation|brief/.test(factId)) return 'service';
  if (/public|content|508|ada_|electronic|document/.test(factId)) return 'audience_and_channel';
  if (/vendor|third_party|tracking|analytics|dap|embed|ict/.test(factId)) return 'dependencies';
  return 'government_owner';
}

export function parseFactValue(raw) {
  try { return JSON.parse(raw); } catch { return raw; }
}

/** Read --facts <file> and repeated --fact id=value. Pairs win over the file. */
export async function readFactsInput({ file = null, pairs = [] }) {
  const facts = {};
  if (file) {
    let parsed;
    try { parsed = JSON.parse(await readFile(file, 'utf8')); } catch (error) {
      throw new UsageError(`--facts ${file}: ${error.code === 'ENOENT' ? 'file not found' : `not valid JSON (${error.message})`}.`);
    }
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) throw new UsageError(`--facts ${file} must hold a JSON object of fact_id: value.`);
    Object.assign(facts, parsed);
  }
  for (const pair of pairs) {
    const at = pair.indexOf('=');
    if (at < 1) throw new UsageError(`--fact "${pair}" must look like id=value.`);
    facts[pair.slice(0, at)] = parseFactValue(pair.slice(at + 1));
  }
  for (const [id, value] of Object.entries(facts)) {
    if (!FACT_ID.test(id)) throw new UsageError(`fact id "${id}" is not valid: use lower snake case.`);
    if (value === null || value === undefined) throw new UsageError(`fact "${id}" has no value. Omit a fact you cannot answer; it stays unknown.`);
  }
  return facts;
}

/** Every fact id the selected profile or its rule packs can read, plus the starter catalog. */
export function knownFactIds({ profile, packs }) {
  const known = new Set(buildInitialFacts().map(fact => fact.fact_id));
  for (const id of profile?.required_facts || []) known.add(id);
  for (const pack of packs) for (const rule of pack.rules || []) for (const id of rule.applicability?.required_facts || []) known.add(id);
  return known;
}

export function assertKnownFacts(facts, known, profileId) {
  const unknown = Object.keys(facts).filter(id => !known.has(id)).sort();
  if (unknown.length) throw new UsageError(`unknown fact id${unknown.length === 1 ? '' : 's'} for profile ${profileId}: ${unknown.join(', ')}. No selected rule reads ${unknown.length === 1 ? 'it' : 'them'}.`);
}

/** Apply facts and the explicit permission grant to a SCOPE.json document. Returns the changed ids. */
export function applyFacts(scope, facts, { now, origin, grantNetwork = true }) {
  const byId = new Map(scope.facts.map(fact => [fact.fact_id, fact]));
  const merged = { ...facts, allowed_origins: [origin] };
  const changed = [];
  for (const [id, value] of Object.entries(merged)) {
    let fact = byId.get(id);
    if (!fact) {
      fact = { fact_id: id, group: groupFor(id), label: id.replaceAll('_', ' '), value: null, state: 'unknown', evidence_refs: [], provided_by: null, observed_at: null, valid_until: null, question: null, owner_role: null, affected_obligations: [], effect_on_assessment: null };
      scope.facts.push(fact);
      byId.set(id, fact);
    }
    fact.value = value;
    fact.state = 'known';
    fact.provided_by = 'operator';
    fact.observed_at = now;
    changed.push(id);
  }
  scope.permissions = { ...scope.permissions, allowed_origins: [origin], recorded_by: grantNetwork ? 'diligent audit --url' : 'diligent audit --replay', recorded_at: now };
  scope.processing_boundary = grantNetwork
    ? { ...scope.processing_boundary, network_allowed: true, notes: `Network access to ${origin} was granted by the operator by running diligent audit --url. Model dispatch stays off.` }
    : { ...scope.processing_boundary, notes: `Replay of recorded captures for ${origin}. No network access was granted. Model dispatch stays off.` };
  return changed;
}
