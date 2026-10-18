#!/usr/bin/env node
/** diligent audit: run the whole chain against a live https site with the diligent-rs companion binary. */
import { UsageError } from './lib/artifacts.mjs';
import { DILIGENT_VERSION, parseArgs, runMain } from './lib/cli.mjs';
import { runAudit } from './lib/orchestrate/run.mjs';

const HELP = `Usage: diligent audit --url <https://host/> [audit-id] [options]
       diligent audit --replay <replay.json> [audit-id] [options]

Runs init, scope, applicability, inventory, plan, collect, assess, infra, merge,
gate and report in one pass. The Node skill orchestrates, the diligent-rs binary
executes the automated stages, and Node stays authoritative for the gate and the
report. The binary is never downloaded; see docs/install.md.

Options:
  --url <url>            site to audit (required unless --replay); its origin is the only origin tested
  --replay <file>        audit recorded captures offline: { "origin": ..., "pages": [{ "url", "captures" }] }. Inventory,
                         collection and the network stages are skipped; applicability, assess, merge, gate and report run as usual.
                         Needs diligent-rs but no Chrome and no network. A relative captures path is tried against the replay
                         file's directory, the package root, then the current directory
  --profile <id>         profile from profiles/ (default us-federal-public-web)
  --root <dir>           workspace root (default: $DILIGENT_ROOT, nearest .diligent/, or cwd)
  --data-root <dir>      read authorities, rules and profiles from <dir>
  --facts <file.json>    object of fact_id: value, recorded as known operator facts
  --fact id=value        one fact (repeatable); a value is JSON when it parses, else text
  --pages <n>            most pages to collect, 1 to 200 (default 10)
  --seed <s>             sampler seed (default: the run id)
  --processes <file>     JSON list of multi-step processes for the sampler
  --corroborate axe      force axe-core corroboration on for each page; it can only agree or send a result to review
  --no-corroborate       force it off, even when the profile's corroboration.browser lists axe-core
  --no-signals           skip the advisory craft and content signals
  --allow-private        let the start host be a private address (local smoke runs)
  --strict-origins       block every request outside the audited origin, subresources included
  --chrome-path <p>      Chrome binary (else DILIGENT_CHROME, else common paths)
  --timeout-ms <n>       per-operation timeout (default 30000)
  --json                 print { audit_id, run_id, directory, stages, gate_exit, report_path, corroboration }; a stage may carry a detail (error kinds and stderr, 4 KB at most)

Axe-core runs when the profile lists it under corroboration.browser or --corroborate axe is given.
Without facts, applicability stays unknown and the gate is incomplete. Facts are
supplied by you, never guessed; see references/facts/ for an example.
Running this command records, as an event, that you granted network access to the origin.

Exit codes: the gate's (0 pass for declared scope, 1 mandatory failure, 2 incomplete),
or 3 when a stage failed before the gate could run, or the diligent-rs binary is missing.
Diligent ${DILIGENT_VERSION}. Diligent is not a government service and does not certify anything.`;

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), {
    url: 'string', replay: 'string', profile: 'string', root: 'string', 'data-root': 'string', facts: 'string', fact: 'string', pages: 'string', seed: 'string',
    processes: 'string', corroborate: 'string', 'no-corroborate': 'boolean', 'no-signals': 'boolean', 'allow-private': 'boolean', 'strict-origins': 'boolean',
    'chrome-path': 'string', 'timeout-ms': 'string', json: 'boolean'
  });
  if (options.help) { console.log(HELP); return 0; }
  if (positional.length > 1) throw new UsageError('audit takes at most one [audit-id]. Run with --help.');
  if (!options.url && !options.replay) throw new UsageError('audit requires --url <https://host/> or --replay <file>. Run with --help.');
  // parseArgs keeps the last value of a repeated option; collect every --fact here.
  const pairs = [];
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--fact') pairs.push(argv[++i]);
    else if (argv[i].startsWith('--fact=')) pairs.push(argv[i].slice('--fact='.length));
  }
  const result = await runAudit({
    url: options.url, replay: options.replay, auditId: positional[0], profile: options.profile, root: options.root, dataRoot: options['data-root'], facts: options.facts,
    fact: pairs, pages: options.pages, seed: options.seed, processes: options.processes, corroborate: options.corroborate, noCorroborate: options['no-corroborate'],
    noSignals: options['no-signals'], allowPrivate: options['allow-private'], strictOrigins: options['strict-origins'],
    chromePath: options['chrome-path'], timeoutMs: options['timeout-ms']
  }, { log: options.json ? () => {} : line => console.error(`diligent audit: ${line}`) });
  if (result.message) { console.error(result.message); return result.exit; }
  if (options.json) { console.log(JSON.stringify(result.summary, null, 2)); return result.exit; }
  const s = result.summary;
  console.log(`Audit ${s.audit_id} (${s.run_id}) at ${s.directory}`);
  for (const stage of s.stages) console.log(`  ${stage.state.padEnd(8)} ${stage.name} (${stage.duration_ms} ms)${stage.warning ? `: ${stage.warning}` : ''}`);
  console.log(s.gate_exit === null ? 'The gate did not run.' : `Gate exit ${s.gate_exit}: ${['pass for declared scope', 'mandatory failure', 'incomplete', 'invalid input'][s.gate_exit] ?? 'unknown'}.`);
  if (s.report_path) console.log(`Report: ${s.report_path}`);
  return result.exit;
}

runMain(main);
