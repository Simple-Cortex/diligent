#!/usr/bin/env node
/**
 * diligent report: render REPORT.md from ASSESSMENT.json (blueprint 24.2).
 * This is a rendering. It never recomputes or reinterprets the assessment.
 */
import { join } from 'node:path';
import {
  UsageError, advanceRun, appendEvent, atomicWrite, latestRun, loadArtifact, locateAuditDir, pathExists, sha256File
} from './lib/artifacts.mjs';
import { isMain, parseArgs, runMain } from './lib/cli.mjs';

const HELP = `Usage: diligent report <audit-id> [--root <workspace>] [--dir <audit-directory>] [--stdout]

Renders REPORT.md from ASSESSMENT.json (plus FINDINGS.json, APPLICABILITY.json,
SERVICE-BRIEF.json, REPAIRS.json, VERIFICATION.json and the lock files when present).
The report is a rendering of recorded facts: it never recomputes the assessment.

Exit codes: 0 when REPORT.md was written, even if the assessment is fail or
incomplete (the report carries the non-passing status; use "diligent gate" for
CI decisions). 3 when ASSESSMENT.json is missing or invalid.

Required section order (blueprint 24.2): header, then
 1 service brief, 2 mandatory findings, 3 unresolved applicability and missing
 evidence, 4 accessibility coverage and unsupported criteria, 5 UX barriers,
 6 coverage counts, 7 repair plan and verification, 8 exceptions and release
 decision, 9 versions and evidence freshness.`;

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
  return items.map(item => `\`${item.kind}\` ${item.ref}: ${item.reason}`);
}

export function renderReport({ auditId, assessment, run, brief, findings, applicability, repairs, verification, rulesLock, authoritiesLock, evidence, scope }) {
  const out = [];
  const target = brief?.service_name ? `${brief.service_name} (${auditId})` : auditId;
  const profile = run?.profile_id || rulesLock?.profile?.profile_id || 'not recorded';
  out.push(`# Diligent assessment report: ${target}`);
  out.push('');
  out.push(`- **Target:** ${target}`);
  out.push(`- **Snapshot and date:** ${run?.snapshot_refs?.length ? run.snapshot_refs.join(', ') : 'no snapshot recorded'}; assessment computed ${assessment.computed_at}`);
  out.push(`- **Audit modes:** ${run?.modes?.join(', ') || scope?.modes?.join(', ') || 'not recorded'}`);
  out.push(`- **Selected profiles:** ${profile}`);
  out.push(`- **Assessment status:** ${STATUS_TEXT[assessment.overall]}`);
  out.push(`- **Run:** ${assessment.run_id}`);
  out.push('');
  out.push(`> ${assessment.claim_boundary}`);
  out.push('');

  // 1
  out.push(`## 1. ${SECTION_TITLES[0]}`);
  out.push('');
  if (!brief) out.push(unavailable('SERVICE-BRIEF.json'));
  else if (brief.brief_status === 'skeleton') out.push('The service brief is still a skeleton. No user goal is confirmed, so UX findings cannot be tied to a confirmed task.');
  else {
    const confirmed = brief.jobs.filter(job => job.provenance !== 'inferred');
    const inferred = brief.jobs.filter(job => job.provenance === 'inferred');
    out.push(`Service: ${brief.service_name ?? 'unnamed'}. Owner: ${brief.owner ?? 'unknown'}. Subject: ${brief.subject ?? 'unknown'}.`);
    out.push('');
    out.push('Confirmed goals:');
    out.push(bullet(confirmed.map(job => `${job.description} (${job.provenance})`)));
    out.push('');
    out.push('Inferred goals, not confirmed by the owner:');
    out.push(bullet(inferred.map(job => job.description)));
  }
  out.push('');

  // 2
  out.push(`## 2. ${SECTION_TITLES[1]}`);
  out.push('');
  if (assessment.failures.length === 0) out.push('No confirmed failure of an applicable mandatory obligation is recorded. This does not mean the site passes. See sections 3, 4 and 6 for what is unresolved.');
  else {
    out.push(table(['Obligation', 'Check', 'Instance', 'Result'], assessment.failures.map(f => [f.obligation_id, f.check_id, f.instance_id, f.result_id])));
  }
  const mandatory = (findings?.findings || []).filter(f => f.category === 'mandatory_failure');
  if (mandatory.length) {
    out.push('');
    for (const f of mandatory) out.push(`- **${f.finding_id}** (${f.rule_id}, ${f.severity}): ${f.observation} Impact: ${f.impact}`);
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
  out.push(`Unsupported applicable obligations: ${assessment.counts.unsupported} of ${assessment.denominators.unsupported}. Diligent 0.1.0 ships no automated website checks; a skipped or unsupported check is never reported as a pass.`);
  out.push('');

  // 5
  out.push(`## 5. ${SECTION_TITLES[4]}`);
  out.push('');
  const ux = (findings?.findings || []).filter(f => f.category === 'ux_barrier' || f.category === 'recommendation');
  if (!findings) out.push(unavailable('FINDINGS.json'));
  else if (ux.length === 0) out.push('No UX barriers or recommendations are recorded.');
  else out.push(bullet(ux.map(f => `**${f.finding_id}** (${f.category}, ${f.severity}): ${f.observation}`)));
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
  out.push(bullet(assessment.limitations));
  out.push('');

  // 7
  out.push(`## 7. ${SECTION_TITLES[6]}`);
  out.push('');
  if (!repairs && !verification) out.push('No repair plan and no verification exist. Their absence is shown here on purpose; a proposal is never evidence of resolution.');
  else {
    if (repairs) out.push(bullet(repairs.repairs.map(r => `${r.repair_id} for ${r.finding_refs.join(', ') || 'no finding'}: ${r.status}`)));
    else out.push(unavailable('REPAIRS.json'));
    out.push('');
    if (verification) out.push(`Verification: ${verification.status}. New failures: ${verification.new_failures.length}. Remaining review: ${verification.remaining_review.length}.`);
    else out.push(unavailable('VERIFICATION.json'));
  }
  out.push('');

  // 8
  out.push(`## 8. ${SECTION_TITLES[7]}`);
  out.push('');
  out.push(bullet(assessment.exceptions.map(e => `${e.exception_id} on ${e.obligation_id}: ${e.status}. It does not change any result.`)));
  out.push('');
  const rd = assessment.release_disposition;
  out.push(rd ? `Release decision (${rd.release_policy_id}, owner ${rd.owner}): ${rd.status}. ${rd.rationale}` : 'No release decision is recorded. A release decision is separate from the assessment and cannot rewrite a failure.');
  out.push('');

  // 9
  out.push(`## 9. ${SECTION_TITLES[8]}`);
  out.push('');
  const versions = run?.tool_versions ? Object.entries(run.tool_versions).map(([k, v]) => `${k}: ${v}`) : [];
  out.push(bullet([
    ...versions,
    ...(rulesLock ? rulesLock.packs.map(p => `rule pack ${p.pack_id} ${p.pack_version} (${p.rule_count} rules, sha256 ${p.sha256.slice(0, 12)})`) : []),
    ...(authoritiesLock ? [`authority registry ${authoritiesLock.registry_version} (${authoritiesLock.authorities.length} authorities, sha256 ${authoritiesLock.registry_sha256.slice(0, 12)}, ${authoritiesLock.source_mode})`] : []),
    `engine: ${run?.engine ? `${run.engine.name}, executed by ${run.engine.executed_by}` : 'not recorded'}`
  ]));
  out.push('');
  if (rulesLock || authoritiesLock) {
    const research = authoritiesLock ? authoritiesLock.authorities.filter(a => a.source_review_status !== 'reviewed').length : null;
    if (research !== null) out.push(`Authority records not yet human reviewed: ${research} of ${authoritiesLock.authorities.length}.`);
  } else out.push(unavailable('AUTHORITIES.lock.json and RULES.lock.json'));
  out.push('');
  out.push(evidence ? `Evidence records: ${evidence.length}${evidence.length ? `; latest capture ${evidence.map(e => e.captured_at).sort().at(-1)}` : ''}.` : unavailable('EVIDENCE.jsonl'));
  out.push('');
  return `${out.join('\n')}\n`;
}

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), { root: 'string', dir: 'string', stdout: 'boolean' });
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
