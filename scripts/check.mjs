#!/usr/bin/env node
/** Runs the full local check suite and summarizes. A failing step is never hidden. */
import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { PACKAGE_ROOT } from './lib/artifacts.mjs';
import { parseArgs, runMain } from './lib/cli.mjs';

const HELP = `Usage: node scripts/check.mjs

Runs, in order: rules validate, validate-skill, the unit and fixture tests, and
the packed-artifact test. Every step runs even if an earlier one fails, then a
summary lists each step as PASS or FAIL. Exit 0 only if every step passed.`;

const STEPS = [
  ['rules validate', 'scripts/validate-rules.mjs'],
  ['validate-skill', 'scripts/validate-skill.mjs'],
  ['tests', 'scripts/test.mjs'],
  ['packed artifact', null, ['--test', 'tests/packed-artifact.test.mjs']]
];

function run(args) {
  return new Promise(resolvePromise => {
    const child = spawn(process.execPath, args, { cwd: PACKAGE_ROOT, stdio: 'inherit' });
    child.on('error', () => resolvePromise(1));
    child.on('close', code => resolvePromise(code ?? 1));
  });
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2), {});
  if (options.help) { console.log(HELP); return 0; }
  const summary = [];
  for (const [label, script, args] of STEPS) {
    console.log(`\n=== ${label} ===`);
    const code = await run(args || [join(PACKAGE_ROOT, script)]);
    summary.push([label, code]);
  }
  console.log('\n=== summary ===');
  for (const [label, code] of summary) console.log(`${code === 0 ? 'PASS' : 'FAIL'}  ${label}${code === 0 ? '' : ` (exit ${code})`}`);
  return summary.every(([, code]) => code === 0) ? 0 : 1;
}

runMain(main);
