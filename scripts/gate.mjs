#!/usr/bin/env node
/**
 * diligent gate: recompute the regulatory assessment deterministically from
 * frozen artifacts (blueprint 24.1). Exit codes follow blueprint 20.3:
 * 0 pass_for_declared_scope (or not_applicable), 1 fail, 2 incomplete,
 * 3 invalid configuration or schema.
 */
import { join } from 'node:path';
import {
  UsageError, advanceRun, appendEvent, atomicWriteJson, latestRun, loadArtifact, locateAuditDir,
  nowIso, pathExists, sha256File, validateData
} from './lib/artifacts.mjs';
import { parseArgs, runMain } from './lib/cli.mjs';
import { CLAIM_BOUNDARY, aggregate, exitCodeFor } from './lib/status.mjs';

const HELP = `Usage: diligent gate <audit-id> [--root <workspace>] [--dir <audit-directory>] [--json] [--now <iso-time>]

Loads APPLICABILITY.json, RESULTS.jsonl, EXCEPTIONS.json, REVIEW-QUEUE.json and
RULES.lock.json, validates each against its schema, and writes ASSESSMENT.json.

Overall outcomes and exit codes:
  0  pass_for_declared_scope (or not_applicable with cited predicates)
  1  fail: a confirmed failure of an applicable mandatory obligation
  2  incomplete: unknown or pending applicability, untested or unsupported
     obligations, open review items, stale evidence, or a missing input
  3  invalid input or schema (an incomplete ASSESSMENT.json is still written)

Missing APPLICABILITY.json or RESULTS.jsonl is always incomplete, never a pass.
Exceptions are listed separately and never change a result.
Options:
  --dir <path>   use an audit directory directly instead of <root>/.diligent/<audit-id>
  --now <iso>    fixed computation time, for deterministic replays
  --json         print the assessment as JSON`;

const RUN_ID_PATTERN = /^run-\d{4}-\d{2}-\d{2}-[0-9a-f]{6}$/;
const PLACEHOLDER_RUN = 'run-1970-01-01-000000';

function emptyCounts() {
  return {
    counts: { selected: 0, applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, required_instances: 0, executed: 0, reviewed: 0, unsupported: 0 },
    denominators: { applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, executed: 0, reviewed: 0, unsupported: 0 }
  };
}

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), { root: 'string', dir: 'string', json: 'boolean', now: 'string' });
  if (options.help) { console.log(HELP); return 0; }
  if (positional.length > 1) throw new UsageError('gate takes at most one <audit-id>. Run with --help.');
  const dir = locateAuditDir({ auditId: positional[0], dir: options.dir, root: options.root });
  if (!(await pathExists(dir))) {
    console.error(`diligent gate: audit directory ${dir} does not exist. Run "diligent init <audit-id>" first.`);
    return 3;
  }
  const now = options.now || nowIso();

  const names = ['APPLICABILITY.json', 'RESULTS.jsonl', 'EXCEPTIONS.json', 'REVIEW-QUEUE.json', 'RULES.lock.json'];
  const loaded = {};
  for (const name of names) loaded[name] = await loadArtifact(dir, name);

  const limitations = ['Diligent 0.1.0 runs no automated website checks. Results come only from human or documentary input.'];
  const unresolved = [];
  const invalid = names.filter(name => loaded[name].exists && !loaded[name].ok);

  // Run id: prefer a consistent value across the artifacts; flag disagreement.
  const runIds = new Map();
  for (const name of names) {
    const data = loaded[name].data;
    const ids = Array.isArray(data) ? data.map(record => record?.run_id) : [data?.run_id];
    for (const id of ids) if (typeof id === 'string') runIds.set(id, [...(runIds.get(id) || []), name]);
  }
  const run = await latestRun(dir);
  let runId = [...runIds.keys()].find(id => RUN_ID_PATTERN.test(id)) || run?.data?.run_id;
  if (!RUN_ID_PATTERN.test(runId || '')) { runId = PLACEHOLDER_RUN; limitations.push('No valid run id could be read from the inputs; a placeholder run id is recorded.'); }
  const mismatch = runIds.size > 1;

  let assessment;
  let code;
  if (invalid.length > 0 || mismatch) {
    for (const name of invalid) {
      for (const line of loaded[name].errors.slice(0, 5)) limitations.push(line);
      unresolved.push({ kind: 'invalid_artifact', ref: name, reason: `${name} failed schema validation (${loaded[name].errors.length} error${loaded[name].errors.length === 1 ? '' : 's'}).` });
    }
    if (mismatch) {
      unresolved.push({ kind: 'run_id_mismatch', ref: 'artifacts', reason: `Artifacts disagree on the run id: ${[...runIds.entries()].map(([id, files]) => `${id} (${[...new Set(files)].join(', ')})`).join('; ')}.` });
    }
    assessment = {
      overall: 'incomplete', has_incomplete_work: true, ...emptyCounts(), unresolved, failures: [], exceptions: []
    };
    code = 3;
  } else {
    const applicability = loaded['APPLICABILITY.json'];
    const results = loaded['RESULTS.jsonl'];
    const lock = loaded['RULES.lock.json'];
    const missing = [];
    if (!applicability.exists) missing.push(['APPLICABILITY.json', 'Applicability has not been resolved, so no obligation can be assessed.']);
    if (!results.exists) missing.push(['RESULTS.jsonl', 'No check results exist, so nothing has been tested.']);
    if (!lock.exists) missing.push(['RULES.lock.json', 'The rules lock is missing, so supported checks cannot be determined.']);
    for (const [name, reason] of missing) {
      unresolved.push({ kind: 'missing_artifact', ref: name, reason });
      limitations.push(`${name} is missing: ${reason}`);
    }
    if (!loaded['EXCEPTIONS.json'].exists) limitations.push('EXCEPTIONS.json is absent; no exceptions were considered. An absent file is not a record of zero exceptions.');
    if (!loaded['REVIEW-QUEUE.json'].exists) limitations.push('REVIEW-QUEUE.json is absent; open review items could not be considered.');

    const obligations = applicability.data?.obligations || [];
    const matrix = lock.data?.supported_check_matrix || [];
    const rows = new Map();
    for (const row of matrix) {
      if (!rows.has(row.obligation_id)) rows.set(row.obligation_id, []);
      rows.get(row.obligation_id).push(row);
    }
    const unsupportedIds = lock.exists
      ? obligations.filter(o => !rows.has(o.obligation_id) || rows.get(o.obligation_id).every(row => row.unsupported)).map(o => o.obligation_id)
      : [];
    if (lock.exists) {
      const unlocked = obligations.filter(o => o.state === 'applicable' && !rows.has(o.obligation_id)).length;
      if (unlocked) limitations.push(`${unlocked} applicable obligation${unlocked === 1 ? ' is' : 's are'} absent from RULES.lock.json and counted as unsupported.`);
    }

    const result = aggregate(obligations, results.data || [], loaded['EXCEPTIONS.json'].data?.exceptions || [], loaded['REVIEW-QUEUE.json'].data?.items || [], {
      unsupported_obligation_ids: unsupportedIds, now
    });
    result.unresolved = [...unresolved, ...result.unresolved].sort((a, b) => (a.kind === b.kind ? (a.ref < b.ref ? -1 : 1) : a.kind < b.kind ? -1 : 1));
    result.has_incomplete_work = result.unresolved.length > 0;
    if (result.failures.length === 0 && result.unresolved.length > 0) result.overall = 'incomplete';
    if (result.advisory.length) limitations.push(`${result.advisory.length} failing result${result.advisory.length === 1 ? '' : 's'} on non-mandatory obligations ${result.advisory.length === 1 ? 'is' : 'are'} recorded in RESULTS.jsonl and do not enter the regulatory gate.`);
    if (result.exceptions.length) limitations.push(`${result.exceptions.length} exception${result.exceptions.length === 1 ? '' : 's'} recorded. Exceptions never change a result; the technical failure stays visible.`);
    if (result.overall === 'pass_for_declared_scope') limitations.push('The pass applies only to the declared scope and the instances actually tested. It is not a whole-site claim and not a certification.');
    assessment = result;
    code = exitCodeFor(result.overall);
  }

  const findings = await pathExists(join(dir, 'FINDINGS.json'));
  const document = {
    schema_version: '1.0.0', run_id: runId, computed_at: now,
    overall: assessment.overall, has_incomplete_work: assessment.has_incomplete_work,
    counts: assessment.counts, denominators: assessment.denominators,
    unresolved: assessment.unresolved, failures: assessment.failures, exceptions: assessment.exceptions,
    ux_findings_ref: findings ? 'FINDINGS.json' : null,
    release_disposition: null, limitations, claim_boundary: CLAIM_BOUNDARY
  };
  const self = await validateData('assessment', document, 'ASSESSMENT.json');
  if (!self.ok) {
    console.error('diligent gate: computed assessment failed its own schema; nothing was written.');
    for (const line of self.errors.slice(0, 10)) console.error(`  ${line}`);
    return 3;
  }
  const outPath = join(dir, 'ASSESSMENT.json');
  await atomicWriteJson(outPath, document);
  await appendEvent(dir, {
    run_id: runId, stage: 'assessed', event: code === 3 ? 'gate_invalid_input' : 'gate_computed',
    artifact: 'ASSESSMENT.json', sha256: await sha256File(outPath), detail: `overall=${document.overall}`
  });
  if (code !== 3) await advanceRun(dir, runId, 'assessed', { assessment_complete: !document.has_incomplete_work });

  if (options.json) console.log(JSON.stringify(document, null, 2));
  else {
    console.log(`Overall: ${document.overall} (exit ${code})`);
    const c = document.counts;
    console.log(`Selected ${c.selected}; applicable ${c.applicable}/${c.selected}; unresolved applicability ${c.unresolved_applicability}/${c.selected}; pending ${c.pending}/${c.selected}.`);
    console.log(`Executed ${c.executed}/${c.required_instances} instances; reviewed ${c.reviewed}/${document.denominators.reviewed}; unsupported ${c.unsupported}/${c.applicable} applicable.`);
    for (const failure of document.failures.slice(0, 10)) console.log(`FAIL ${failure.obligation_id} (${failure.result_id})`);
    for (const item of document.unresolved.slice(0, 10)) console.log(`- ${item.kind}: ${item.ref}`);
    if (document.unresolved.length > 10) console.log(`- ... and ${document.unresolved.length - 10} more unresolved items`);
    for (const line of limitations.slice(0, 6)) console.log(`Limitation: ${line}`);
    console.log(`Wrote ${outPath}`);
  }
  return code;
}

runMain(main);
