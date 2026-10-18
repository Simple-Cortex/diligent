#!/usr/bin/env node
/**
 * Offline end-to-end replay: audit six recorded captures with the real diligent-rs binary and no
 * Chrome or network, then check the whole artifact chain, the gate exit and the exact set of failing
 * checks. This is the part of the end-to-end proof that runs everywhere the binary exists.
 */
import { spawn } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { mkdtemp, readFile, rename, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadArtifact } from './lib/artifacts.mjs';
import { isMain, parseArgs, runMain } from './lib/cli.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const HELP = `Usage: node scripts/e2e-replay.mjs [--allow-missing-binary] [--keep]

Runs "diligent audit --replay fixtures/e2e/replay.json" with DILIGENT_RS_BIN (default target/debug/diligent-rs)
and the example facts, then checks every artifact, the gate exit and the failing check ids recorded in
fixtures/e2e/replay-expected.json. No Chrome and no network are used.

--allow-missing-binary  exit 0 with SKIPPED when the binary is missing (unit-test jobs only)
--keep                  keep the temporary workspace and print its path`;

function capture(command, args, env) {
  return new Promise(resolve => {
    let stdout = ''; let stderr = '';
    const child = spawn(command, args, { cwd: ROOT, env, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', c => { stdout += c; });
    child.stderr.on('data', c => { stderr += c; });
    child.on('error', error => resolve({ code: 127, stdout, stderr: stderr + error.message }));
    child.on('close', code => resolve({ code: code ?? 1, stdout, stderr }));
  });
}
const rows = async path => (await readFile(path, 'utf8')).split('\n').filter(l => l.trim()).map(l => JSON.parse(l));

/** Returns { status: 'passed' | 'skipped' | 'failed', checks, diagnostics, workspace? }. */
export async function runReplay({ allowMissingBinary = false, keep = false, env = process.env } = {}) {
  const binary = env.DILIGENT_RS_BIN || join(ROOT, 'target', 'debug', 'diligent-rs');
  if (!existsSync(binary) || !statSync(binary).isFile()) {
    const message = `diligent-rs binary (${binary}) not found`;
    return allowMissingBinary ? { status: 'skipped', checks: [], message } : { status: 'failed', checks: [{ name: 'prerequisites', ok: false, detail: message }], diagnostics: [] };
  }
  const expected = JSON.parse(await readFile(join(ROOT, 'fixtures', 'e2e', 'replay-expected.json'), 'utf8'));
  const checks = []; const diagnostics = [];
  const check = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });
  const workspace = await mkdtemp(join(tmpdir(), 'diligent-replay-'));
  try {
    const run = await capture(process.execPath, [
      'bin/diligent.mjs', 'audit', '--replay', 'fixtures/e2e/replay.json', 'replay', '--facts', 'references/facts/us-federal-public-web.example.json', '--root', workspace, '--json'
    ], { ...env, DILIGENT_RS_BIN: binary });
    check('audit exit code equals the expected gate exit', run.code === expected.gate_exit, `exit ${run.code}, expected ${expected.gate_exit}`);
    diagnostics.push(`audit exit ${run.code}`);
    let summary = null;
    try { summary = JSON.parse(run.stdout); } catch { check('audit --json output parses', false, run.stdout.slice(0, 300)); }
    if (summary) {
      for (const stage of summary.stages) diagnostics.push(`stage ${stage.name}: ${stage.state}, exit ${stage.exit_code}${stage.warning ? `, ${stage.warning}` : ''}${stage.detail ? `\n${stage.detail.split('\n').map(l => `    ${l}`).join('\n')}` : ''}`);
      check('every stage ran without failing', summary.stages.every(s => s.state !== 'failed'));
      const skipped = summary.stages.filter(s => s.state === 'skipped').map(s => s.name).sort();
      check('collect and infra were skipped', JSON.stringify(skipped) === JSON.stringify(['collect', 'infra']), skipped.join(','));
    }
    if (run.stderr.trim()) diagnostics.push(`audit stderr (last 20 lines):\n${run.stderr.trim().split('\n').slice(-20).map(l => `    ${l}`).join('\n')}`);
    const dir = join(workspace, '.diligent', 'replay');
    for (const name of ['SCOPE.json', 'APPLICABILITY.json', 'INVENTORY.json', 'SAMPLE.json', 'AUDIT-PLAN.json', 'EVIDENCE.jsonl', 'RESULTS.jsonl', 'SIGNALS.jsonl', 'REVIEW-QUEUE.json', 'ASSESSMENT.json', 'EVENTS.jsonl']) {
      const loaded = await loadArtifact(dir, name);
      check(`${name} exists and validates`, loaded.exists && loaded.ok, loaded.exists ? loaded.errors.slice(0, 2).join('; ') : 'missing');
    }
    const runRecord = summary?.run_id ? await loadArtifact(join(dir, 'runs', summary.run_id), 'RUN.json') : null;
    check('RUN.json records replay mode and the pipeline reached reported', runRecord?.ok && runRecord.data.pipeline_state === 'reported' && runRecord.data.extensions?.audit?.mode === 'replay' && /^[0-9a-f]{64}$/.test(runRecord.data.extensions.audit.replay_sha256 ?? ''));
    if (['RESULTS.jsonl', 'SIGNALS.jsonl', 'EVIDENCE.jsonl', 'REVIEW-QUEUE.json', 'ASSESSMENT.json'].every(n => existsSync(join(dir, n)))) {
      const results = await rows(join(dir, 'RESULTS.jsonl'));
      const counts = { results: results.length, signals: (await rows(join(dir, 'SIGNALS.jsonl'))).length, evidence: (await rows(join(dir, 'EVIDENCE.jsonl'))).length, review_items: JSON.parse(await readFile(join(dir, 'REVIEW-QUEUE.json'), 'utf8')).items.length };
      for (const [key, minimum] of Object.entries(expected.minimum)) check(`at least ${minimum} ${key.replace('_', ' ')}`, counts[key] >= minimum, `found ${counts[key]}`);
      const failing = [...new Set(results.filter(r => r.status === 'fail').map(r => r.check_id))].sort();
      check('the set of failing check ids is exactly the expected one', JSON.stringify(failing) === JSON.stringify([...expected.failing_result_checks].sort()), `found ${failing.join(', ')}`);
      const assessment = JSON.parse(await readFile(join(dir, 'ASSESSMENT.json'), 'utf8'));
      const gateFailing = [...new Set(assessment.failures.map(f => f.check_id))].sort();
      check('the gate lists exactly the expected failing checks', JSON.stringify(gateFailing) === JSON.stringify([...expected.gate_failing_checks].sort()), `found ${gateFailing.join(', ')}`);
      const queue = JSON.parse(await readFile(join(dir, 'REVIEW-QUEUE.json'), 'utf8'));
      check('one review item per needs_review result', queue.items.length === results.filter(r => r.status === 'needs_review').length);
      // SIGNALS.jsonl must have no effect on the gate.
      const before = await readFile(join(dir, 'ASSESSMENT.json'));
      await rename(join(dir, 'SIGNALS.jsonl'), join(dir, 'SIGNALS.jsonl.away'));
      await capture(process.execPath, ['scripts/gate.mjs', '--dir', dir, '--now', assessment.computed_at], env);
      const without = await readFile(join(dir, 'ASSESSMENT.json'));
      await rename(join(dir, 'SIGNALS.jsonl.away'), join(dir, 'SIGNALS.jsonl'));
      await capture(process.execPath, ['scripts/gate.mjs', '--dir', dir, '--now', assessment.computed_at], env);
      check('ASSESSMENT.json is byte-identical with and without SIGNALS.jsonl', before.equals(without) && before.equals(await readFile(join(dir, 'ASSESSMENT.json'))));
    }
    const report = existsSync(join(dir, 'REPORT.md')) ? await readFile(join(dir, 'REPORT.md'), 'utf8') : '';
    for (const heading of ['## 2', '## 4', '## 10. Craft and clarity']) check(`REPORT.md contains "${heading}"`, report.includes(heading));
  } finally {
    if (!keep) await rm(workspace, { recursive: true, force: true });
  }
  return { status: checks.every(c => c.ok) ? 'passed' : 'failed', checks, diagnostics, workspace: keep ? workspace : undefined };
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2), { 'allow-missing-binary': 'boolean', keep: 'boolean' });
  if (options.help) { console.log(HELP); return 0; }
  const result = await runReplay({ allowMissingBinary: options['allow-missing-binary'], keep: options.keep });
  if (result.status === 'skipped') { console.log(`e2e replay: SKIPPED (${result.message})`); return 0; }
  for (const c of result.checks) console.log(`${c.ok ? 'ok     ' : 'FAILED '} ${c.name}${c.ok || !c.detail ? '' : `: ${c.detail}`}`);
  if (result.status === 'failed' && result.diagnostics?.length) console.log(`\naudit diagnostics:\n${result.diagnostics.join('\n')}\n`);
  if (result.workspace) console.log(`workspace kept at ${result.workspace}`);
  console.log(`e2e replay: ${result.status === 'passed' ? 'PASSED' : 'FAILED'}`);
  return result.status === 'passed' ? 0 : 1;
}

if (isMain(import.meta.url)) runMain(main);
