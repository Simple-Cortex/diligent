#!/usr/bin/env node
/** Runs every tests/*.test.mjs with node:test (the packed-artifact test has its own script). */
import { readdir } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { PACKAGE_ROOT } from './lib/artifacts.mjs';
import { parseArgs, runMain } from './lib/cli.mjs';

const HELP = `Usage: node scripts/test.mjs

Runs every tests/*.test.mjs with the built-in node:test runner, except
packed-artifact.test.mjs, which is slower and runs via "npm run test:packed-artifact".
Exit code is that of the test runner.`;

async function main() {
  const { options } = parseArgs(process.argv.slice(2), {});
  if (options.help) { console.log(HELP); return 0; }
  const files = (await readdir(join(PACKAGE_ROOT, 'tests')))
    .filter(name => name.endsWith('.test.mjs') && name !== 'packed-artifact.test.mjs').sort()
    .map(name => join('tests', name));
  if (files.length === 0) { console.error('test: no test files found; refusing to report success.'); return 1; }
  return new Promise(resolvePromise => {
    const child = spawn(process.execPath, ['--test', ...files], { cwd: PACKAGE_ROOT, stdio: 'inherit' });
    child.on('error', () => resolvePromise(1));
    child.on('close', code => resolvePromise(code ?? 1));
  });
}

runMain(main);
