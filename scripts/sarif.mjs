#!/usr/bin/env node
/**
 * diligent sarif: emit SARIF 2.1.0 from a run directory.
 *
 * Usage:
 *   diligent sarif --run <run-id> [--dir <audit-dir>] [--out <file>] [--baseline <file>]
 *   diligent sarif --dir <run-dir> [--out <file>] [--baseline <file>]
 *
 * Exit 0 on success, 2 on usage error, 3 when the run directory is missing
 * or an artifact fails schema validation.
 */
import { writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { convertToSarif, serializeSarif, validateSarif } from './lib/sarif.mjs';
import { loadBaseline } from './lib/diff.mjs';
import { locateAuditDir, pathExists } from './lib/artifacts.mjs';
import { parseArgs, runMain, isMain } from './lib/cli.mjs';

const HELP = `Usage: diligent sarif --run <run-id> [--dir <audit-dir>] [--out <file>] [--baseline <file>]
       diligent sarif --dir <run-dir> [--out <file>] [--baseline <file>]

Emit SARIF 2.1.0 from a Diligent run directory.

Options:
  --run <id>        run id within an audit directory
  --dir <path>      audit directory (--run) or run directory (direct)
  --out <file>      output file (default: stdout)
  --baseline <file> baseline file; marks known results as "unchanged", new as "new", omits resolved
  --help            show this help

Exit codes: 0 success, 2 usage error, 3 missing or invalid run directory.`;

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), {
    run: 'string',
    dir: 'string',
    out: 'string',
    baseline: 'string'
  });
  if (options.help) { console.log(HELP); return 0; }

  let runDir;
  if (options.run) {
    const auditDir = locateAuditDir({ dir: options.dir, auditId: positional[0] });
    runDir = join(auditDir, 'runs', options.run);
  } else if (options.dir) {
    runDir = resolve(options.dir);
  } else {
    console.error('sarif: --run <id> or --dir <path> is required.');
    return 2;
  }

  if (!(await pathExists(runDir))) {
    console.error(`sarif: run directory not found: ${runDir}`);
    return 3;
  }

  let baseline = null;
  if (options.baseline) {
    try {
      baseline = await loadBaseline(options.baseline);
    } catch (error) {
      console.error(`sarif: ${error.message}`);
      return 3;
    }
  }

  const { ok, sarif, errors } = await convertToSarif(runDir, { baseline });
  if (!ok) {
    for (const err of errors) console.error(`sarif: ${err}`);
    return 3;
  }

  const validation = validateSarif(sarif);
  if (!validation.ok) {
    for (const err of validation.errors) console.error(`sarif: internal: ${err}`);
    return 3;
  }

  const output = serializeSarif(sarif);
  if (options.out) {
    await writeFile(options.out, output, 'utf8');
  } else {
    process.stdout.write(output);
  }
  return 0;
}

if (isMain(import.meta.url)) runMain(main);
