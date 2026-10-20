#!/usr/bin/env node
/**
 * diligent report: render REPORT.md from ASSESSMENT.json (audit specification 24.2).
 * This is a rendering. It never recomputes or reinterprets the assessment.
 */
import { readFile, stat } from 'node:fs/promises';
import { join, resolve, sep } from 'node:path';
import {
  PACKAGE_ROOT, UsageError, advanceRun, appendEvent, atomicWrite, latestRun, loadArtifact, locateAuditDir, pathExists, sha256File, validateData
} from './lib/artifacts.mjs';
import { DILIGENT_VERSION, isMain, parseArgs, runMain } from './lib/cli.mjs';
import { readJsonFile } from './lib/data.mjs';

const HELP = `Usage: diligent report <audit-id> [--root <workspace>] [--dir <audit-directory>] [--data-root <dir>] [--stdout]

Renders REPORT.md from ASSESSMENT.json (plus FINDINGS.json, APPLICABILITY.json,
SERVICE-BRIEF.json, REPAIRS.json, VERIFICATION.json, SIGNALS.jsonl and the lock files
when present). Rule packs are read from the installed package, or from --data-root.
The report is a rendering of recorded facts: it never recomputes the assessment.

Exit codes: 0 when REPORT.md was written, even if the assessment is fail or
incomplete (the report carries the non-passing status; use "diligent gate" for
CI decisions). 3 when ASSESSMENT.json is missing or invalid.

Required section order (audit specification 24.2): header, then
 1 service brief, 2 mandatory findings, 3 unresolved applicability and missing
 evidence, 4 accessibility coverage and unsupported criteria, 5 UX barriers,
 6 coverage counts, 7 repair plan and verification, 8 exceptions and release
 decision, 9 versions and evidence freshness, then 10 craft and clarity signals.
Section 4 also shows accessibility results under three separate WCAG mappings.
Section 10 holds advisory practice signals only. They are not legal obligations
and never enter the assessment status or the gate.`;

export const SECTION_TITLES = [
  'User task and service brief',
  'Mandatory findings',
  'Unresolved applicability and missing organizational evidence',
  'Accessibility coverage, unsupported criteria and manual results',
  'UX barriers and design recommendations',
  'Coverage counts, sampled scope, unreachable paths and vendor boundaries',
  'Repair plan and verification status',
  'Exceptions, accepted risk and release decision',
  'Source, rule, model and tool versions and evidence freshness'
];

const STATUS_TEXT = {
  fail: 'FAIL. At least one applicable mandatory obligation has a confirmed failure.',
  incomplete: 'INCOMPLETE. Required checks, facts, reviews or evidence are missing. This is not a pass.',
  pass_for_declared_scope: 'PASS FOR DECLARED SCOPE. Every required check and review for the declared scope is satisfied. This is not a certification and not a whole-site claim.',
  not_applicable: 'NOT APPLICABLE. Every selected obligation is excluded with a cited predicate.'
};

const SECTION_3_KINDS = new Set(['unknown_applicability', 'pending_effective_date', 'uncited_exclusion', 'duplicate_obligation', 'nothing_selected', 'missing_artifact', 'invalid_artifact', 'run_id_mismatch', 'open_review_item']);
const SECTION_4_KINDS = new Set(['unsupported_obligation', 'no_result', 'orphan_result', 'stale_evidence', 'result_not_tested', 'result_blocked', 'result_error', 'result_needs_review']);

const bullet = items => (items.length ? items.map(item => `- ${item}`).join('\n') : '- None recorded.');
const unavailable = name => `Not available: ${name} does not exist for this audit. Nothing is implied by its absence.`;

function table(headers, rows) {
  const line = cells => `| ${cells.join(' | ')} |`;
  return [line(headers), line(headers.map(() => '---')), ...rows.map(line)].join('\n');
}

function unresolvedLines(items) {
  return items.map(item => `${code(item.kind)} ${code(item.ref)}: ${clip(item.reason, 400)}`);
}

/** Section 10 title. Kept out of SECTION_TITLES so the nine audit specification sections stay unchanged. */
export const SIGNALS_TITLE = 'Craft and clarity (not legal obligations)';
export const SIGNALS_SENTENCE = 'These are not legal obligations. They are practice signals, labelled lab, static or field, for a person to weigh.';

const FAMILY_ORDER = ['content', 'craft', 'form_flow', 'lab_performance'];
const FAMILY_TITLES = {
  content: 'Content and plain language',
  craft: 'Craft',
  form_flow: 'Forms and flows',
  lab_performance: 'Lab performance (lab values: unthrottled, headless)'
};
const MAX_LEAD_INSTANCES = 10;

/** One line of untrusted text: collapse whitespace, drop backticks, cap the length. */
function plain(value, max) {
  const text = String(value ?? '').replace(/\s+/g, ' ').replace(/`/g, "'").trim();
  return text.length > max ? `${text.slice(0, max - 3)}...` : text;
}
/** Untrusted text outside a code span: clipped, and every Markdown and HTML metacharacter backslash-escaped. */
function clip(value, max = 160) {
  return plain(value, max).replace(/[\\*_[\]()<>|!#&~]/g, '\\$&');
}
/** Threshold text: escape dangerous markdown but preserve identifiers and comparison operators. */
function clipThreshold(value, max = 120) {
  // Escape the whole string exactly as clip() does, so no link or emphasis can span an operator.
  // Then restore only the exact tokens " <= ", " >= ", " < " and " > " (spaces included). A "<" or ">"
  // followed by a space opens no tag or quote, and a "=" is inert, so the restored text is plain text.
  return clip(value, max).replace(/ \\(<=?|>=?) /g, ' $1 ');
}
/** Untrusted text inside a code span: clipped, and backticks removed so it cannot break out. */
function code(value, max = 80) { const t = plain(value, max).replace(/\|/g, '\u00a6'); return t ? `\`${t}\`` : '`-`'; }

function formatValue(signal) {
  const { value, unit } = signal;
  let text;
  if (value === null || value === undefined) text = 'no value';
  else if (Array.isArray(value)) text = value.length ? `${value.length} item${value.length === 1 ? '' : 's'}: ${value.slice(0, 5).map(v => clip(typeof v === 'object' ? JSON.stringify(v) : v, 40)).join(', ')}${value.length > 5 ? ', ...' : ''}` : 'none';
  else if (typeof value === 'object') text = clip(JSON.stringify(value), 120);
  else text = clip(value, 80);
  return unit ? `${text} ${clip(unit, 20)}` : text;
}

function thresholdText(signal) {
  const t = signal.threshold;
  if (!t) return 'no threshold';
  return `practice: ${clipThreshold(t.rule, 80)}, aligned with ${clipThreshold(t.source, 120)}, confidence ${plain(t.confidence, 10)}`;
}

const byKey = (a, b) => (a.signal_key === b.signal_key ? (a.instance_id < b.instance_id ? -1 : a.instance_id > b.instance_id ? 1 : 0) : a.signal_key < b.signal_key ? -1 : 1);

/**
 * Section 10 body. `signals` is the array from SIGNALS.jsonl, or null when the file is absent.
 * Callers pass only rows that validated against schemas/signal.schema.json: the threshold text and the
 * family are rendered with relaxed escaping because the schema pins them. `skipped` counts rows that failed
 * validation and were left out. `problem` is a short note when the file could not be read at all.
 * Signals are advisory: this never reads or changes the assessment status.
 */
export function renderSignals(signals, { runId = null, problem = null, skipped = 0 } = {}) {
  const out = [SIGNALS_SENTENCE, ''];
  if (problem) {
    out.push(`SIGNALS.jsonl exists but could not be rendered: ${clip(problem, 300)} The assessment status above is unaffected.`);
    return out;
  }
  if (!signals) { out.push(unavailable('SIGNALS.jsonl')); return out; }
  const skippedNote = skipped > 0 ? `${skipped} row${skipped === 1 ? '' : 's'} in SIGNALS.jsonl failed schema validation and ${skipped === 1 ? 'was' : 'were'} not rendered.` : null;
  if (signals.length === 0) { out.push(skipped > 0 ? `SIGNALS.jsonl holds no valid signals. ${skippedNote}` : 'SIGNALS.jsonl exists but holds no signals.'); return out; }

  const count = state => signals.filter(s => s.state === state).length;
  const summaryLine = `Signals recorded: ${signals.length}. Leads: ${count('lead')}. Within practice: ${count('within_practice')}. Informational: ${count('informational')}. Not tested: ${count('not_tested')}. Signal states never enter the assessment status, the gate or the counts in section 6.`;
  const runs = [...new Set(signals.map(s => s.run_id))];
  if (runId && (runs.length !== 1 || runs[0] !== runId)) {
    out.push('');
    out.push(`Run mismatch: the signals were recorded for ${runs.map(r => clip(r, 60)).join(', ')} but the assessment is for ${clip(runId, 60)}. Treat them as belonging to a different run.`);
  }

  const families = [...FAMILY_ORDER, ...[...new Set(signals.map(s => s.family))].filter(f => !FAMILY_ORDER.includes(f)).sort()];
  for (const family of families) {
    const own = signals.filter(s => s.family === family);
    if (own.length === 0) continue;
    out.push('');
    out.push(`**${FAMILY_TITLES[family] || plain(family, 40)}**`);
    const leads = own.filter(s => s.state === 'lead').sort(byKey);
    out.push('');
    if (leads.length === 0) out.push('Worth a look: none.');
    else {
      out.push(`Worth a look (${leads.length}):`);
      const shown = new Map();
      const lines = [];
      for (const s of leads) {
        const n = (shown.get(s.signal_key) || 0) + 1;
        shown.set(s.signal_key, n);
        if (n <= MAX_LEAD_INSTANCES) lines.push(`${code(s.signal_key, 60)} on ${clip(s.instance_id, 60)} [${clip(s.label, 10)}]: ${formatValue(s)}. ${clip(s.observed, 200)} (${thresholdText(s)}).`);
      }
      for (const [key, n] of shown) if (n > MAX_LEAD_INSTANCES) lines.push(`${code(key, 60)}: ${n - MAX_LEAD_INSTANCES} more instance${n - MAX_LEAD_INSTANCES === 1 ? '' : 's'} in SIGNALS.jsonl.`);
      out.push(bullet(lines));
    }
    const within = own.filter(s => s.state === 'within_practice');
    out.push('');
    out.push(within.length ? `Within practice: ${within.length} (${[...new Set(within.map(s => s.signal_key))].sort().map(k => `${code(k, 60)}`).join(', ')}).` : 'Within practice: none.');
    const info = own.filter(s => s.state === 'informational').sort(byKey);
    if (info.length) {
      out.push('');
      out.push('Informational numbers, with no threshold applied:');
      out.push(bullet(info.slice(0, 50).map(s => `${code(s.signal_key, 60)} on ${clip(s.instance_id, 60)} [${clip(s.label, 10)}]: ${formatValue(s)}.`)));
      if (info.length > 50) out.push(`- ${info.length - 50} more informational signals in SIGNALS.jsonl.`);
    }
    const untested = own.filter(s => s.state === 'not_tested');
    if (untested.length) {
      out.push('');
      out.push(`Not tested: ${untested.length} (${[...new Set(untested.map(s => s.signal_key))].sort().map(k => `${code(k, 60)}`).join(', ')}). A signal that was not tested says nothing either way.`);
    }
  }
  out.push('');
  out.push(skippedNote ? `${summaryLine} ${skippedNote}` : summaryLine);
  const limits = [...new Set(signals.flatMap(s => (Array.isArray(s.limitations) ? s.limitations : [])))].sort();
  if (limits.length) {
    out.push('');
    out.push('Limitations recorded with these signals:');
    out.push(bullet(limits.slice(0, 20).map(l => clip(l, 240))));
    if (limits.length > 20) out.push(`- ${limits.length - 20} more in SIGNALS.jsonl.`);
  }
  return out;
}

const WCAG_ROLES = [
  ['legal_floor', 'Legal floor'],
  ['title_ii_target', 'ADA Title II target'],
  ['recommended', 'Recommended target']
];

/**
 * Index accessibility obligations by the WCAG mappings their rule pack records.
 * Packs named in RULES.lock.json are read through scripts/lib/data.mjs. A pack that cannot be
 * read, or whose bytes differ from the locked hash, is reported in `notes`, never skipped silently.
 * Returns { index: Map<obligation_id, mapping[]>, notes: string[] }.
 */
export async function buildWcagIndex(rulesLock, dataRoot = PACKAGE_ROOT) {
  const index = new Map();
  const notes = [];
  const packsDir = resolve(dataRoot, 'rules', 'packs');
  for (const pack of rulesLock?.packs || []) {
    const id = pack?.pack_id;
    if (typeof id !== 'string' || !/^[a-z0-9-]+$/.test(id)) { notes.push('A locked rule pack has an invalid id, so its WCAG mappings are not shown.'); continue; }
    // The lock's own path field is never used: packs load by id from the fixed packs directory.
    const path = resolve(packsDir, `${id}.json`);
    if (!path.startsWith(`${packsDir}${sep}`)) { notes.push(`Rule pack ${clip(id, 60)} resolves outside the packs directory, so its WCAG mappings are not shown.`); continue; }
    let loaded;
    try {
      if (!(await stat(path)).isFile()) throw new Error('not a file');
      loaded = await readJsonFile(path);
    } catch { notes.push(`Rule pack ${clip(id, 60)} could not be read, so its WCAG mappings are not shown.`); continue; }
    if (loaded.sha256 !== pack.sha256) { notes.push(`Rule pack ${clip(id, 60)} differs from the locked copy, so its WCAG mappings are not shown.`); continue; }
    for (const rule of loaded.json.rules || []) {
      if (Array.isArray(rule.wcag_mappings) && rule.wcag_mappings.length) index.set(rule.obligation_id, rule.wcag_mappings);
    }
  }
  return { index, notes };
}

/**
 * Accessibility results under the three WCAG mappings. Each mapping is its own row with its own
 * counts. The rows are never summed, averaged or blended into one figure.
 */
export function renderWcagMappings(wcag, { assessment, applicability }) {
  const out = [];
  out.push('**Accessibility results by WCAG mapping.** The same criterion can sit in more than one mapping. Each row below is counted on its own and the rows are never added together.');
  out.push('');
  if (!wcag || wcag.index.size === 0) {
    out.push(wcag?.notes?.length ? bullet(wcag.notes) : 'No accessibility rule pack with WCAG mappings is locked for this audit, so no mapping rows are shown.');
    return out;
  }
  const all = [...wcag.index.values()].flat();
  const rolesOf = id => new Set((wcag.index.get(id) || []).map(m => m.role));
  const rows = WCAG_ROLES.map(([role, label]) => {
    const own = all.filter(m => m.role === role);
    const name = own.length ? `WCAG ${[...new Set(own.map(m => m.version))].sort().join(' and ')} ${[...new Set(own.map(m => m.level))].sort().join('/')}` : 'not mapped';
    const selected = (applicability?.obligations || []).filter(o => rolesOf(o.obligation_id).has(role));
    const failures = assessment.failures.filter(f => rolesOf(f.obligation_id).has(role));
    const open = assessment.unresolved.filter(u => rolesOf(u.ref).has(role));
    return [`${label} (${name})`, applicability ? selected.length : 'n/a', failures.length, open.length];
  });
  out.push(table(['Mapping', 'Selected obligations', 'Confirmed failures', 'Unresolved items'], rows.map(r => r.map(String))));
  const failing = WCAG_ROLES.map(([role, label]) => {
    const ids = [...new Set(assessment.failures.filter(f => rolesOf(f.obligation_id).has(role)).map(f => f.obligation_id))].sort();
    return ids.length ? `${label}: ${ids.map(id => clip(id, 80)).join(', ')}` : null;
  }).filter(Boolean);
  if (failing.length) { out.push(''); out.push('Failing obligations per mapping:'); out.push(bullet(failing)); }
  if (wcag.notes.length) { out.push(''); out.push(bullet(wcag.notes)); }
  return out;
}

/** Notice and per-story table for a component audit (diligent audit --storybook). Empty for a site audit. */
export function renderComponentNotice(baseline) {
  const info = baseline?.extensions?.['diligent.storybook'];
  if (!info) return [];
  const out = [];
  out.push(`> **Component audit of ${code(info.storybook_url, 200)}.** ${info.sampled_stories} of ${info.total_stories} stories were audited as components. Whole-page obligations are not tested in this mode (${code(info.whole_page_obligations?.reason ?? 'component mode: whole-page obligation', 120)}). This is not a site claim, and the site-level status above reflects only that.`);
  out.push('>');
  out.push(`> Component gate: ${code(info.gate?.state ?? 'unknown')} (exit ${info.gate?.exit ?? 'n/a'}); ${info.gate?.failed_stories ?? 0} stor${info.gate?.failed_stories === 1 ? 'y' : 'ies'} failed an element-level mandatory check.${info.gate?.problems?.length ? ` ${info.gate.problems.map(p => clip(p, 300)).join('; ')}.` : ''}${info.gate?.notes?.length ? ` ${info.gate.notes.map(n => clip(n, 300)).join(' ')}` : ''}`);
  out.push('');
  out.push('### Stories');
  out.push('');
  out.push(table(['Story', 'Component', 'Failed checks'], (info.stories || []).map(story => [
    code(story.id, 120), clip(story.title, 120), story.failed ? story.failed_checks.map(id => code(id)).join(', ') : (story.assess === 'assessed' ? 'none' : `not assessed (${clip(story.collect === 'collected' ? story.assess : story.collect, 40)})`)
  ])));
  out.push('');
  return out;
}

export function renderReport({ auditId, assessment, run, brief, findings, applicability, repairs, verification, rulesLock, authoritiesLock, evidence, scope, signals = null, signalsProblem = null, signalsSkipped = 0, wcag = null, componentBaseline = null }) {
  const out = [];
  const target = brief?.service_name ? `${clip(brief.service_name, 120)} (${code(auditId)})` : code(auditId);
  const profile = code(run?.profile_id || rulesLock?.profile?.profile_id || 'not recorded');
  out.push(`# Diligent assessment report: ${target}`);
  out.push('');
  out.push(`- **Target:** ${target}`);
  out.push(`- **Snapshot and date:** ${run?.snapshot_refs?.length ? run.snapshot_refs.map(r => code(r, 120)).join(', ') : 'no snapshot recorded'}; assessment computed ${code(assessment.computed_at)}`);
  out.push(`- **Audit modes:** ${(run?.modes || scope?.modes)?.length ? (run?.modes || scope.modes).map(m => code(m)).join(', ') : 'not recorded'}`);
  out.push(`- **Selected profiles:** ${profile}`);
  out.push(`- **Assessment status:** ${STATUS_TEXT[assessment.overall]}`);
  out.push(`- **Run:** ${code(assessment.run_id)}`);
  out.push('');
  out.push(`> ${clip(assessment.claim_boundary, 600)}`);
  out.push('');
  out.push(...renderComponentNotice(componentBaseline));

  // 1
  out.push(`## 1. ${SECTION_TITLES[0]}`);
  out.push('');
  if (!brief) out.push(unavailable('SERVICE-BRIEF.json'));
  else if (brief.brief_status === 'skeleton') out.push('The service brief is still a skeleton. No user goal is confirmed, so UX findings cannot be tied to a confirmed task.');
  else {
    const confirmed = brief.jobs.filter(job => job.provenance !== 'inferred');
    const inferred = brief.jobs.filter(job => job.provenance === 'inferred');
    out.push(`Service: ${clip(brief.service_name ?? 'unnamed')}. Owner: ${clip(brief.owner ?? 'unknown')}. Subject: ${clip(brief.subject ?? 'unknown')}.`);
    out.push('');
    out.push('Confirmed goals:');
    out.push(bullet(confirmed.map(job => `${clip(job.description, 300)} (${clip(job.provenance, 30)})`)));
    out.push('');
    out.push('Inferred goals, not confirmed by the owner:');
    out.push(bullet(inferred.map(job => clip(job.description, 300))));
  }
  out.push('');

  // 2
  out.push(`## 2. ${SECTION_TITLES[1]}`);
  out.push('');
  if (assessment.failures.length === 0) out.push('No confirmed failure of an applicable mandatory obligation is recorded. This does not mean the site passes. See sections 3, 4 and 6 for what is unresolved.');
  else {
    out.push(table(['Obligation', 'Check', 'Instance', 'Result'], assessment.failures.map(f => [f.obligation_id, f.check_id, f.instance_id, f.result_id].map(v => code(v)))));
  }
  const mandatory = (findings?.findings || []).filter(f => f.category === 'mandatory_failure');
  if (mandatory.length) {
    out.push('');
    for (const f of mandatory) out.push(`- **${code(f.finding_id)}** (${code(f.rule_id)}, ${clip(f.severity, 20)}): ${clip(f.observation, 600)} Impact: ${clip(f.impact, 400)}`);
  } else if (!findings) {
    out.push('');
    out.push(unavailable('FINDINGS.json'));
  }
  out.push('');

  // 3
  out.push(`## 3. ${SECTION_TITLES[2]}`);
  out.push('');
  out.push(bullet(unresolvedLines(assessment.unresolved.filter(item => SECTION_3_KINDS.has(item.kind) || !SECTION_4_KINDS.has(item.kind)))));
  if (applicability) {
    const open = applicability.obligations.filter(o => o.state === 'unknown' || o.state === 'pending_effective_date');
    out.push('');
    out.push(`Applicability record: ${applicability.obligations.length} obligations selected, ${open.length} unknown or pending.`);
  } else {
    out.push('');
    out.push(unavailable('APPLICABILITY.json'));
  }
  out.push('');

  // 4
  out.push(`## 4. ${SECTION_TITLES[3]}`);
  out.push('');
  out.push(bullet(unresolvedLines(assessment.unresolved.filter(item => SECTION_4_KINDS.has(item.kind)))));
  out.push('');
  out.push(`Unsupported applicable obligations: ${assessment.counts.unsupported} of ${assessment.denominators.unsupported}. Diligent ${DILIGENT_VERSION} ships no automated website checks; a skipped or unsupported check is never reported as a pass.`);
  out.push('');
  out.push(...renderWcagMappings(wcag, { assessment, applicability }));
  out.push('');

  // 5
  out.push(`## 5. ${SECTION_TITLES[4]}`);
  out.push('');
  const ux = (findings?.findings || []).filter(f => f.category === 'ux_barrier' || f.category === 'recommendation');
  if (!findings) out.push(unavailable('FINDINGS.json'));
  else if (ux.length === 0) out.push('No UX barriers or recommendations are recorded.');
  else out.push(bullet(ux.map(f => `**${code(f.finding_id)}** (${clip(f.category, 30)}, ${clip(f.severity, 20)}): ${clip(f.observation, 600)}`)));
  out.push('');
  out.push('Recommendations and UX hypotheses do not enter the regulatory status. Model-generated hypotheses are not observed user failures.');
  out.push('');

  // 6
  out.push(`## 6. ${SECTION_TITLES[5]}`);
  out.push('');
  const c = assessment.counts;
  const d = assessment.denominators;
  out.push(table(['Measure', 'Count', 'Out of'], [
    ['Selected obligations', c.selected, '-'],
    ['Applicable', c.applicable, d.applicable],
    ['Not applicable (cited)', c.not_applicable, d.not_applicable],
    ['Unresolved applicability', c.unresolved_applicability, d.unresolved_applicability],
    ['Pending effective date', c.pending, d.pending],
    ['Required check instances', c.required_instances, '-'],
    ['Executed instances', c.executed, d.executed],
    ['Reviewed items', c.reviewed, d.reviewed],
    ['Unsupported applicable obligations', c.unsupported, d.unsupported]
  ].map(row => row.map(String))));
  out.push('');
  out.push('Counts are shown with denominators and are never blended into a single score. Sampled scope, unreachable paths and vendor boundaries are listed under limitations:');
  out.push('');
  out.push(bullet(assessment.limitations.map(l => clip(l, 400))));
  out.push('');

  // 7
  out.push(`## 7. ${SECTION_TITLES[6]}`);
  out.push('');
  if (!repairs && !verification) out.push('No repair plan and no verification exist. Their absence is shown here on purpose; a proposal is never evidence of resolution.');
  else {
    if (repairs) out.push(bullet(repairs.repairs.map(r => `${code(r.repair_id)} for ${r.finding_refs.map(x => code(x)).join(', ') || 'no finding'}: ${clip(r.status, 30)}`)));
    else out.push(unavailable('REPAIRS.json'));
    out.push('');
    if (verification) out.push(`Verification: ${clip(verification.status, 30)}. New failures: ${verification.new_failures.length}. Remaining review: ${verification.remaining_review.length}.`);
    else out.push(unavailable('VERIFICATION.json'));
  }
  out.push('');

  // 8
  out.push(`## 8. ${SECTION_TITLES[7]}`);
  out.push('');
  out.push(bullet(assessment.exceptions.map(e => `${code(e.exception_id)} on ${code(e.obligation_id)}: ${clip(e.status, 30)}. It does not change any result.`)));
  out.push('');
  const rd = assessment.release_disposition;
  out.push(rd ? `Release decision (${code(rd.release_policy_id)}, owner ${clip(rd.owner, 80)}): ${clip(rd.status, 30)}. ${clip(rd.rationale, 600)}` : 'No release decision is recorded. A release decision is separate from the assessment and cannot rewrite a failure.');
  out.push('');

  // 9
  out.push(`## 9. ${SECTION_TITLES[8]}`);
  out.push('');
  const versions = run?.tool_versions ? Object.entries(run.tool_versions).map(([k, v]) => `${clip(k, 60)}: ${clip(v, 120)}`) : [];
  out.push(bullet([
    ...versions,
    ...(rulesLock ? rulesLock.packs.map(p => `rule pack ${code(p.pack_id)} ${code(p.pack_version)} (${p.rule_count} rules, sha256 ${code(p.sha256.slice(0, 12))})`) : []),
    ...(authoritiesLock ? [`authority registry ${code(authoritiesLock.registry_version)} (${authoritiesLock.authorities.length} authorities, sha256 ${code(authoritiesLock.registry_sha256.slice(0, 12))}, ${clip(authoritiesLock.source_mode, 40)})`] : []),
    `engine: ${run?.engine ? `${clip(run.engine.name, 60)}, executed by ${clip(run.engine.executed_by, 60)}` : 'not recorded'}`
  ]));
  out.push('');
  if (rulesLock || authoritiesLock) {
    const research = authoritiesLock ? authoritiesLock.authorities.filter(a => a.source_review_status !== 'reviewed').length : null;
    if (research !== null) out.push(`Authority records not yet human reviewed: ${research} of ${authoritiesLock.authorities.length}.`);
  } else out.push(unavailable('AUTHORITIES.lock.json and RULES.lock.json'));
  out.push('');
  out.push(evidence ? `Evidence records: ${evidence.length}${evidence.length ? `; latest capture ${code(evidence.map(e => e.captured_at).sort().at(-1))}` : ''}.` : unavailable('EVIDENCE.jsonl'));
  out.push('');

  // 10
  out.push(`## 10. ${SIGNALS_TITLE}`);
  out.push('');
  out.push(...renderSignals(signals, { runId: assessment.run_id, problem: signalsProblem, skipped: signalsSkipped }));
  out.push('');
  return `${out.join('\n')}\n`;
}

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), { root: 'string', dir: 'string', 'data-root': 'string', stdout: 'boolean' });
  if (options.help) { console.log(HELP); return 0; }
  if (positional.length > 1) throw new UsageError('report takes at most one <audit-id>. Run with --help.');
  const dir = locateAuditDir({ auditId: positional[0], dir: options.dir, root: options.root });
  if (!(await pathExists(dir))) { console.error(`diligent report: audit directory ${dir} does not exist.`); return 3; }

  const assessment = await loadArtifact(dir, 'ASSESSMENT.json');
  if (!assessment.exists) { console.error('diligent report: ASSESSMENT.json is missing. Run "diligent gate" first; the report only renders a recorded assessment.'); return 3; }
  if (!assessment.ok) {
    console.error('diligent report: ASSESSMENT.json is invalid; refusing to render it.');
    for (const line of assessment.errors.slice(0, 10)) console.error(`  ${line}`);
    return 3;
  }
  const optional = {};
  const problems = [];
  for (const [key, name] of Object.entries({
    brief: 'SERVICE-BRIEF.json', findings: 'FINDINGS.json', applicability: 'APPLICABILITY.json', repairs: 'REPAIRS.json',
    verification: 'VERIFICATION.json', rulesLock: 'RULES.lock.json', authoritiesLock: 'AUTHORITIES.lock.json', evidence: 'EVIDENCE.jsonl', scope: 'SCOPE.json'
  })) {
    const artifact = await loadArtifact(dir, name);
    if (artifact.exists && !artifact.ok) problems.push(...artifact.errors.slice(0, 3));
    optional[key] = artifact.exists && artifact.ok ? artifact.data : null;
  }
  if (problems.length) {
    console.error('diligent report: an input artifact is invalid; refusing to render it.');
    for (const line of problems) console.error(`  ${line}`);
    return 3;
  }
  // Signals are advisory. A bad SIGNALS.jsonl is named in section 10 and never blocks the report.
  const signalsFile = await loadArtifact(dir, 'SIGNALS.jsonl').catch(error => ({ exists: true, ok: false, errors: [error.message], data: null }));
  // Rows are validated one by one: only rows that pass the signal schema are rendered, the rest are counted.
  optional.signals = null;
  optional.signalsProblem = null;
  optional.signalsSkipped = 0;
  if (signalsFile.exists && signalsFile.data) {
    const nonBlank = (await readFile(signalsFile.path, 'utf8')).split('\n').filter(line => line.trim() !== '').length;
    optional.signals = [];
    for (const row of signalsFile.data) if ((await validateData('signal', row, 'SIGNALS.jsonl')).ok) optional.signals.push(row);
    optional.signalsSkipped = nonBlank - optional.signals.length;
  } else if (signalsFile.exists) {
    optional.signalsProblem = `the file could not be read (${signalsFile.errors.length} error${signalsFile.errors.length === 1 ? '' : 's'}).`;
  }
  const componentFile = await loadArtifact(dir, 'COMPONENT-BASELINE.json');
  if (componentFile.exists && !componentFile.ok) { console.error('diligent report: COMPONENT-BASELINE.json is invalid; refusing to render it.'); return 3; }
  optional.componentBaseline = componentFile.exists && componentFile.data.run_id === assessment.data.run_id ? componentFile.data : null;
  optional.wcag = await buildWcagIndex(optional.rulesLock, options['data-root'] || PACKAGE_ROOT);
  const latest = await latestRun(dir);
  const run = latest?.data && latest.data.run_id === assessment.data.run_id ? latest.data : null;
  const auditId = run?.audit_id || optional.scope?.audit_id || dir.split('/').filter(Boolean).at(-1);

  const markdown = renderReport({ auditId, assessment: assessment.data, run, ...optional });
  if (options.stdout) { process.stdout.write(markdown); return 0; }
  const path = join(dir, 'REPORT.md');
  await atomicWrite(path, markdown);
  await appendEvent(dir, { run_id: assessment.data.run_id, stage: 'reported', event: 'report_written', artifact: 'REPORT.md', sha256: await sha256File(path) });
  await advanceRun(dir, assessment.data.run_id, 'reported');
  console.log(`Wrote ${path}`);
  console.log(`Assessment status: ${assessment.data.overall}. Exit 0 means the report was written, not that the assessment passed.`);
  return 0;
}

if (isMain(import.meta.url)) runMain(main);
