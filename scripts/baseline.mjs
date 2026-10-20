#!/usr/bin/env node
/**
 * diligent baseline write: create a baseline from a run directory.
 *
 * Usage:
 *   diligent baseline write --run <run-id> --out <file> [--dir <audit-dir>]
 *   diligent baseline write --dir <run-dir> --out <file>
 */
import { writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { writeBaseline } from './lib/diff.mjs';
import { locateAuditDir, pathExists } from './lib/artifacts.mjs';
import { parseArgs, runMain, isMain } from './lib/cli.mjs';

const HELP = `Usage: diligent baseline write --run <run-id> --out <file> [--dir <audit-dir>]
       diligent baseline write --dir <run-dir> --out <file>

Create a baseline file from a run directory. The baseline records fingerprints
of all findings so a subsequent run can diff against it.

Options:
  --run <id>     run id within an audit directory
  --dir <path>   audit directory (--run) or run directory (direct)
  --out <file>   output file (required)
  --help         show this help

Exit codes: 0 success, 2 usage error, 3 missing or invalid run directory.`;

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), {
    run: 'string',
    dir: 'string',
    out: 'string'
  });
  if (options.help) { console.log(HELP); return 0; }

  if (positional[0] !== 'write') {
    console.error('baseline: unknown subcommand. Use "baseline write".');
    return 2;
  }

  if (!options.out) {
    console.error('baseline: --out <file> is required.');
    return 2;
  }

  let runDir;
  if (options.run) {
    const auditDir = locateAuditDir({ dir: options.dir, auditId: positional[1] });
    runDir = join(auditDir, 'runs', options.run);
  } else if (options.dir) {
    runDir = resolve(options.dir);
  } else {
    console.error('baseline: --run <id> or --dir <path> is required.');
    return 2;
  }

  if (!(await pathExists(runDir))) {
    console.error(`baseline: run directory not found: ${runDir}`);
    return 3;
  }

  try {
    const baseline = await writeBaseline(runDir);
    await writeFile(options.out, JSON.stringify(baseline, null, 2) + '\n', 'utf8');
    return 0;
  } catch (error) {
    console.error(`baseline: ${error.message}`);
    return 3;
  }
}

if (isMain(import.meta.url)) runMain(main);
