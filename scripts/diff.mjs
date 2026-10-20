#!/usr/bin/env node
/**
 * diligent diff: compare a run against a baseline.
 *
 * Usage:
 *   diligent diff --run <run-id> --baseline <file> [--json] [--fail-on new|any|none]
 *   diligent diff --dir <run-dir> --baseline <file> [--json] [--fail-on new|any|none]
 */
import { resolve, join } from 'node:path';
import { loadBaseline, diffFindings } from './lib/diff.mjs';
import { locateAuditDir, pathExists } from './lib/artifacts.mjs';
import { parseArgs, runMain, isMain } from './lib/cli.mjs';

const HELP = `Usage: diligent diff --run <run-id> --baseline <file> [--json] [--fail-on new|any|none]
       diligent diff --dir <run-dir> --baseline <file> [--json] [--fail-on new|any|none]

Compare a run's results and findings against a baseline. Partitions into new, known,
resolved, and unverified.

Options:
  --run <id>        run id within an audit directory
  --dir <path>      audit directory (--run) or run directory (direct)
  --baseline <file> baseline file (required)
  --json            emit JSON instead of human-readable output
  --fail-on <mode>  exit 1 when: new (any new findings), any (new or resolved), none (never)
  --help            show this help

Exit codes: 0 success (or --fail-on not matched), 1 --fail-on matched, 2 usage error, 3 error.`;

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), {
    run: 'string',
    dir: 'string',
    baseline: 'string',
    json: 'boolean',
    'fail-on': 'string'
  });
  if (options.help) { console.log(HELP); return 0; }

  if (!options.baseline) {
    console.error('diff: --baseline <file> is required.');
    return 2;
  }

  const failOn = options['fail-on'] || 'none';
  if (!['new', 'any', 'none'].includes(failOn)) {
    console.error('diff: --fail-on must be new, any, or none.');
    return 2;
  }

  let runDir;
  if (options.run) {
    const auditDir = locateAuditDir({ dir: options.dir, auditId: positional[0] });
    runDir = join(auditDir, 'runs', options.run);
  } else if (options.dir) {
    runDir = resolve(options.dir);
  } else {
    console.error('diff: --run <id> or --dir <path> is required.');
    return 2;
  }

  if (!(await pathExists(runDir))) {
    console.error(`diff: run directory not found: ${runDir}`);
    return 3;
  }

  let baseline;
  try {
    baseline = await loadBaseline(options.baseline);
  } catch (error) {
    console.error(`diff: ${error.message}`);
    return 3;
  }

  try {
    const result = await diffFindings(runDir, baseline);

    if (options.json) {
      console.log(JSON.stringify(result, null, 2));
    } else {
      console.log(`New: ${result.new.length}`);
      console.log(`Known: ${result.known.length}`);
      console.log(`Resolved: ${result.resolved.length}`);
      console.log(`Unverified: ${result.unverified.length}`);
      if (result.warnings.length > 0) {
        console.log(`Warnings: ${result.warnings.join(', ')}`);
      }
      if (result.new.length > 0) {
        console.log('\nNew entries:');
        for (const f of result.new) {
          const what = f.source === 'finding' ? f.finding_id : `${f.check_id} @ ${f.instance_id}`;
          console.log(`  - ${f.source} ${f.rule_id}: ${what}`);
        }
      }
    }

    if (failOn === 'new' && result.new.length > 0) return 1;
    if (failOn === 'any' && (result.new.length > 0 || result.resolved.length > 0)) return 1;
    return 0;
  } catch (error) {
    console.error(`diff: ${error.message}`);
    return 3;
  }
}

if (isMain(import.meta.url)) runMain(main);
