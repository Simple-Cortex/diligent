import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { releaseSteps, runSteps } from '../scripts/release-gate.mjs';
import { ROOT, run } from './helpers.mjs';

const fake = (name, code, extra = {}) => ({ name, command: 'x', args: [], skippedBy: null, ...extra, code });

test('the gate stops at the first failing step, prints its output, and marks later steps NOT RUN', async () => {
  const seen = [];
  const log = [];
  const steps = [fake('one', 0), fake('two', 7), fake('three', 0)];
  const outcome = await runSteps(steps, {
    run: async step => { seen.push(step.name); return { code: step.code, output: step.code ? 'boom: details\n' : '' }; },
    log: line => log.push(line)
  });
  assert.equal(outcome.ok, false);
  assert.deepEqual(seen, ['one', 'two'], 'step three never runs');
  assert.deepEqual(outcome.results.map(r => r.verdict), ['PASS', 'FAIL', 'NOT RUN']);
  assert.equal(outcome.results[1].code, 7);
  assert.match(log[0], /^PASS {5}one/);
  assert.match(log[1], /^FAIL {5}two \(exit 7/);
  assert.match(log[2], /boom: details/);
  assert.equal(log.at(-1), 'NOT RUN  three');
});

test('an all-passing run is ok, and a skipped step is printed with the flag that skipped it', async () => {
  const log = [];
  const outcome = await runSteps([fake('a', 0), fake('b', 0, { skippedBy: '--skip-rust' })], { run: async () => ({ code: 0, output: '' }), log: line => log.push(line) });
  assert.equal(outcome.ok, true);
  assert.deepEqual(outcome.results.map(r => r.verdict), ['PASS', 'SKIPPED']);
  assert.ok(log.includes('SKIPPED  b (--skip-rust)'));
});

test('the real step list is in the documented order and the skip flags touch only their steps', () => {
  const names = releaseSteps().map(s => s.name);
  assert.deepEqual(names, ['vendor names', 'validate rules', 'npm test', 'packed artifact', 'validate skill', 'cargo fmt', 'cargo clippy', 'cargo test', 'cargo build', 'offline replay', 'end-to-end smoke']);
  assert.ok(releaseSteps({ chromeAvailable: true }).every(s => s.skippedBy === null));
  assert.deepEqual(releaseSteps({ skipCargoTest: true, chromeAvailable: true }).filter(s => s.skippedBy).map(s => s.name), ['cargo test']);
  assert.deepEqual(releaseSteps({ skipRust: true }).filter(s => !s.skippedBy).map(s => s.name), ['vendor names', 'validate rules', 'npm test', 'packed artifact', 'validate skill']);
  const clippy = releaseSteps().find(s => s.name === 'cargo clippy');
  assert.deepEqual(clippy.args, ['clippy', '--workspace', '--all-targets', '--', '-D', 'warnings']);
});

test('release-gate --help lists the steps and exits 0', async () => {
  const help = await run('scripts/release-gate.mjs', ['--help']);
  assert.equal(help.code, 0);
  assert.match(help.stdout, /cargo clippy --workspace --all-targets -- -D warnings/);
  assert.match(help.stdout, /--skip-cargo-test/);
});

const PROCEDURE_PRESENT = existsSync(join(ROOT, 'docs', 'public-release-procedure.md'));

test('the gate step list matches the commands documented in public-release-procedure.md section 6', { skip: PROCEDURE_PRESENT ? false : 'skipped: docs/public-release-procedure.md is absent (it is a private document)' }, async () => {
  const doc = await readFile(join(ROOT, 'docs', 'public-release-procedure.md'), 'utf8');
  const section6 = doc.split('## 6.')[1]?.split('## 7.')[0] ?? '';
  assert.ok(section6.length > 0, 'section 6 exists');
  const codeBlocks = [...section6.matchAll(/```bash\n([\s\S]*?)```/g)].map(m => m[1]);
  assert.ok(codeBlocks.length >= 1, 'section 6 has at least one code block');
  const firstBlock = codeBlocks[0];
  const docCommands = firstBlock.split('\n').map(l => l.replace(/#.*$/, '').trim()).filter(Boolean);
  const steps = releaseSteps();
  const nonRustSteps = steps.filter(s => !s.rust && s.name !== 'vendor names');
  for (const step of nonRustSteps) {
    const stepCommand = step.command === 'npm' ? `npm ${step.args.join(' ')}` : `${step.command} ${step.args.join(' ')}`;
    const found = docCommands.some(cmd => cmd.includes(step.args[0]) || step.args.some(arg => cmd.includes(arg)));
    assert.ok(found, `gate step "${step.name}" (${stepCommand}) is documented in section 6 first code block`);
  }
  assert.match(section6, /npm run release:gate/, 'section 6 documents npm run release:gate for the full gate');
  assert.match(section6, /cargo fmt/, 'section 6 mentions cargo fmt');
  assert.match(section6, /cargo clippy/, 'section 6 mentions cargo clippy');
  assert.match(section6, /cargo test/, 'section 6 mentions cargo test');
  assert.match(section6, /end-to-end smoke|smoke/, 'section 6 mentions the smoke test');
});

test('the live smoke is skipped by name when Chrome is missing, unless Chrome is required; the offline replay is not', () => {
  const noChrome = releaseSteps({ chromeAvailable: false });
  assert.deepEqual(noChrome.filter(s => s.skippedBy).map(s => s.name), ['end-to-end smoke']);
  assert.match(noChrome.find(s => s.name === 'end-to-end smoke').skippedBy, /Chrome not found/);
  assert.equal(noChrome.find(s => s.name === 'offline replay').skippedBy, null);
  assert.equal(releaseSteps({ chromeAvailable: false, requireChrome: true }).find(s => s.name === 'end-to-end smoke').skippedBy, null);
  const cargoTestSkipped = releaseSteps({ skipCargoTest: true, chromeAvailable: false });
  assert.equal(cargoTestSkipped.find(s => s.name === 'offline replay').skippedBy, null, 'skipping cargo test does not skip the replay');
});
