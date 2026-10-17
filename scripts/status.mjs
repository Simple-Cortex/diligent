#!/usr/bin/env node
/** diligent status: pipeline state and artifact health for one audit. */
import { dirname } from 'node:path';
import { UsageError, ARTIFACTS, latestRun, loadArtifact, locateAuditDir, pathExists } from './lib/artifacts.mjs';
import { parseArgs, runMain } from './lib/cli.mjs';

const HELP = `Usage: diligent status <audit-id> [--root <workspace>] [--dir <audit-directory>] [--json]

Prints the pipeline state from RUN.json and, for every artifact, whether it is
missing, valid, or invalid against its schema. Invalid files are listed with
their first errors; nothing is reported valid without being validated.
Exit 0 when the audit exists, 3 when it does not.`;

const NEXT = {
  initialized: 'run discovery; see skills/discover',
  discovered: 'resolve scope and applicability; see skills/scope',
  scoped: 'inventory the permitted assets; see skills/inventory',
  planned: 'collect evidence; see skills/collect',
  collecting: 'assess results; see skills/assess',
  assessed: 'propose repairs or write the report; see skills/propose and skills/report',
  proposed: 'verify repairs in a separate context; see skills/verify',
  verified: 'write the report; see skills/report',
  reported: 'review the report; re-run gate after any change'
};

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), { root: 'string', dir: 'string', json: 'boolean' });
  if (options.help) { console.log(HELP); return 0; }
  if (positional.length > 1) throw new UsageError('status takes at most one <audit-id>. Run with --help.');
  const dir = locateAuditDir({ auditId: positional[0], dir: options.dir, root: options.root });
  if (!(await pathExists(dir))) { console.error(`diligent status: audit directory ${dir} does not exist.`); return 3; }

  const latest = await latestRun(dir);
  let run = null;
  let runCheck = null;
  if (latest) {
    runCheck = await loadArtifact(dirname(latest.path), 'RUN.json');
    run = runCheck.ok ? runCheck.data : null;
  }
  const artifacts = [];
  for (const name of Object.keys(ARTIFACTS)) {
    const result = await loadArtifact(dir, name);
    artifacts.push({ name, state: !result.exists ? 'missing' : result.ok ? 'valid' : 'invalid', errors: result.errors.slice(0, 3), error_count: result.errors.length });
  }
  for (const extra of ['REPORT.md', 'COMPONENT-GUIDANCE.md']) {
    artifacts.push({ name: extra, state: (await pathExists(`${dir}/${extra}`)) ? 'present' : 'missing', errors: [], error_count: 0 });
  }
  const summary = {
    directory: dir,
    run_id: run?.run_id ?? latest?.runId ?? null,
    run_record: !latest ? 'missing' : runCheck.ok ? 'valid' : 'invalid',
    run_errors: runCheck && !runCheck.ok ? runCheck.errors.slice(0, 3) : [],
    pipeline_state: run?.pipeline_state ?? null,
    execution_status: run?.execution_status ?? null,
    assessment_complete: run?.assessment_complete ?? null,
    engine: run?.engine?.name ?? null,
    next_step: run ? NEXT[run.pipeline_state] : 'run "diligent init <audit-id>" to create a valid run record',
    artifacts
  };
  if (options.json) { console.log(JSON.stringify(summary, null, 2)); return 0; }
  console.log(`Audit directory: ${dir}`);
  console.log(`Run: ${summary.run_id ?? 'none'} (record ${summary.run_record})`);
  console.log(`Pipeline state: ${summary.pipeline_state ?? 'unknown'}; execution ${summary.execution_status ?? 'unknown'}; assessment complete: ${summary.assessment_complete ?? 'unknown'}`);
  for (const line of summary.run_errors) console.log(`  ${line}`);
  for (const a of artifacts) {
    if (a.state === 'missing') continue;
    console.log(`  ${a.state === 'invalid' ? 'INVALID' : a.state === 'present' ? 'present' : 'valid  '} ${a.name}${a.state === 'invalid' ? ` (${a.error_count} errors)` : ''}`);
    for (const line of a.errors) console.log(`      ${line}`);
  }
  console.log(`Missing: ${artifacts.filter(a => a.state === 'missing').map(a => a.name).join(', ') || 'none'}`);
  console.log(`Next: ${summary.next_step}`);
  return 0;
}

runMain(main);
