#!/usr/bin/env node
/**
 * diligent release gate: every check a release needs, in order, stopping at the first failure.
 * It prints one verdict line per step and the failing step's output. Nothing is skipped silently.
 */
import { spawn } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMain, parseArgs, runMain } from './lib/cli.mjs';
import { findChrome } from './e2e-smoke.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const HELP = `Usage: node scripts/release-gate.mjs [--skip-cargo-test] [--skip-rust] [--require-chrome]

Runs, in order, and stops at the first failure:
  npm run validate
  npm test
  npm run test:packed-artifact
  node scripts/validate-skill.mjs
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace                      (skipped by --skip-cargo-test)
  cargo build -p diligent-cli
  node scripts/e2e-replay.mjs                 (offline replay of six recorded captures; needs the binary, not Chrome)
  node scripts/e2e-smoke.mjs                  (live smoke; needs Chrome: set DILIGENT_CHROME)

--skip-cargo-test  leave out cargo test (the Rust CI job already runs it)
--skip-rust        leave out every cargo step, the offline replay and the live smoke
--require-chrome   fail instead of skipping the live smoke when Chrome is not found (use it in CI)

Each skipped step is printed as SKIPPED with the flag that skipped it.
Exit 0 only when every step that ran passed.`;

/** The release steps. `skippedBy` names the flag that skips a step. */
export function releaseSteps(options = {}) {
  const chromeAvailable = options.chromeAvailable ?? Boolean(findChrome());
  const smokeEnv = { DILIGENT_RS_BIN: join(ROOT, 'target', 'debug', 'diligent-rs') };
  const steps = [
    { name: 'validate rules', command: 'npm', args: ['run', 'validate'] },
    { name: 'npm test', command: 'npm', args: ['test'] },
    { name: 'packed artifact', command: 'npm', args: ['run', 'test:packed-artifact'] },
    { name: 'validate skill', command: process.execPath, args: ['scripts/validate-skill.mjs'] },
    { name: 'cargo fmt', command: 'cargo', args: ['fmt', '--all', '--check'], rust: true },
    { name: 'cargo clippy', command: 'cargo', args: ['clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'], rust: true },
    { name: 'cargo test', command: 'cargo', args: ['test', '--workspace'], rust: true, cargoTest: true },
    { name: 'cargo build', command: 'cargo', args: ['build', '-p', 'diligent-cli'], rust: true },
    { name: 'offline replay', command: process.execPath, args: ['scripts/e2e-replay.mjs'], env: smokeEnv, rust: true },
    { name: 'end-to-end smoke', command: process.execPath, args: ['scripts/e2e-smoke.mjs'], env: smokeEnv, rust: true, needsChrome: true }
  ];
  return steps.map(step => ({
    ...step,
    skippedBy: options.skipRust && step.rust ? '--skip-rust'
      : options.skipCargoTest && step.cargoTest ? '--skip-cargo-test'
      : step.needsChrome && !chromeAvailable && !options.requireChrome ? 'Chrome not found; pass --require-chrome to fail instead'
      : null
  }));
}

function execute(step, cwd) {
  return new Promise(resolve => {
    let output = '';
    const child = spawn(step.command, step.args, { cwd, env: { ...process.env, ...(step.env || {}) }, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', chunk => { output += chunk; });
    child.stderr.on('data', chunk => { output += chunk; });
    child.on('error', error => resolve({ code: 127, output: `${output}${error.message}\n` }));
    child.on('close', (code, signal) => resolve({ code: signal ? 1 : code ?? 1, output }));
  });
}

/**
 * Run steps in order. Returns { ok, results }. `run` and `log` are injectable so tests can use
 * fake steps. A failing step stops the run; later steps are reported as NOT RUN.
 */
export async function runSteps(steps, { run = execute, cwd = ROOT, log = line => console.log(line) } = {}) {
  const results = [];
  let failed = false;
  for (const step of steps) {
    if (failed) { results.push({ name: step.name, verdict: 'NOT RUN' }); log(`NOT RUN  ${step.name}`); continue; }
    if (step.skippedBy) { results.push({ name: step.name, verdict: 'SKIPPED' }); log(`SKIPPED  ${step.name} (${step.skippedBy})`); continue; }
    const started = Date.now();
    const outcome = await run(step, cwd);
    const seconds = ((Date.now() - started) / 1000).toFixed(1);
    if (outcome.code === 0) { results.push({ name: step.name, verdict: 'PASS' }); log(`PASS     ${step.name} (${seconds} s)`); continue; }
    failed = true;
    results.push({ name: step.name, verdict: 'FAIL', code: outcome.code, output: outcome.output });
    log(`FAIL     ${step.name} (exit ${outcome.code}, ${seconds} s)`);
    log(`--- output of ${step.name} ---\n${outcome.output.trimEnd()}\n--- end ---`);
  }
  return { ok: !failed, results };
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2), { 'skip-cargo-test': 'boolean', 'skip-rust': 'boolean', 'require-chrome': 'boolean' });
  if (options.help) { console.log(HELP); return 0; }
  const { ok } = await runSteps(releaseSteps({ skipCargoTest: options['skip-cargo-test'], skipRust: options['skip-rust'], requireChrome: options['require-chrome'] }));
  console.log(ok ? 'release gate: PASS' : 'release gate: FAIL');
  return ok ? 0 : 1;
}

if (isMain(import.meta.url)) runMain(main);
