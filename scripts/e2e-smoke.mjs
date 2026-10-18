#!/usr/bin/env node
/**
 * End-to-end smoke: serve a small local site, run `diligent audit` against it with the real
 * diligent-rs binary and Chrome, and check the whole artifact chain. It fails loudly when the
 * binary or Chrome is missing. --allow-missing-chrome turns that into SKIPPED, for unit-test jobs only.
 */
import { spawn } from 'node:child_process';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { mkdtemp, readFile, readdir, rename, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadArtifact } from './lib/artifacts.mjs';
import { isMain, parseArgs, runMain } from './lib/cli.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const HELP = `Usage: node scripts/e2e-smoke.mjs [--allow-missing-chrome] [--keep]

Serves fixtures/inventory/small-site/site.json plus one failing-contrast page on 127.0.0.1, runs
"diligent audit" against it with DILIGENT_RS_BIN (default target/debug/diligent-rs) and Chrome
(DILIGENT_CHROME or a common path), and checks every artifact, the expected gate exit and the
minimum counts in fixtures/e2e/expected.json.

--allow-missing-chrome  exit 0 with SKIPPED when Chrome or the binary is missing (unit-test jobs only)
--keep                  keep the temporary workspace and print its path`;

const CHROME_PATHS = [
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/usr/bin/google-chrome', '/usr/bin/google-chrome-stable', '/usr/bin/chromium', '/usr/bin/chromium-browser', '/snap/bin/chromium'
];

export function findChrome(env = process.env) {
  if (env.DILIGENT_CHROME) return existsSync(env.DILIGENT_CHROME) ? env.DILIGENT_CHROME : null;
  for (const path of CHROME_PATHS) if (existsSync(path)) return path;
  for (const dir of (env.PATH || '').split(delimiter)) {
    for (const name of ['google-chrome', 'chromium', 'chrome']) { const path = join(dir, name); if (dir && existsSync(path)) return path; }
  }
  return null;
}

/** Routes for the smoke site, built from the inventory fixture. `${ORIGIN}` is filled in by the server. */
export function smokeRoutes() {
  const site = JSON.parse(readFileSync(join(ROOT, 'fixtures', 'inventory', 'small-site', 'site.json'), 'utf8'));
  const routes = {};
  for (const [url, entry] of Object.entries(site)) {
    const parsed = new URL(url);
    routes[parsed.pathname] = { status: entry.status ?? 200, headers: entry.headers || {}, body: String(entry.body ?? '').replaceAll('https://example.gov', '${ORIGIN}') };
  }
  routes['/contrast-fail'] = { body: readFileSync(join(ROOT, 'fixtures', 'a11y', 'authored', 'BRW-TEXT-CONTRAST', 'large-bold-18-66px-fail.html'), 'utf8') };
  routes['/'].body = routes['/'].body
    .replace('</main>', '<a href="/contrast-fail">Contrast</a> <p style="color:#aaaaaa;background:#ffffff">Low contrast text on the home page.</p></main>');
  return routes;
}

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

const countLines = async path => (await readFile(path, 'utf8')).split('\n').filter(line => line.trim()).length;

/** Returns { status: 'passed' | 'skipped' | 'failed', checks: [{ name, ok, detail }], workspace? }. */
export async function runSmoke({ allowMissingChrome = false, keep = false, env = process.env } = {}) {
  const binary = env.DILIGENT_RS_BIN || join(ROOT, 'target', 'debug', 'diligent-rs');
  const chrome = findChrome(env);
  const missing = [];
  if (!existsSync(binary) || !statSync(binary).isFile()) missing.push(`diligent-rs binary (${binary})`);
  if (!chrome) missing.push('Chrome (set DILIGENT_CHROME)');
  if (missing.length) {
    const message = `${missing.join(' and ')} not found`;
    return allowMissingChrome ? { status: 'skipped', checks: [], message } : { status: 'failed', checks: [{ name: 'prerequisites', ok: false, detail: message }] };
  }
  const expected = JSON.parse(await readFile(join(ROOT, 'fixtures', 'e2e', 'expected.json'), 'utf8'));
  const checks = [];
  const diagnostics = [];
  const check = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });
  const workspace = await mkdtemp(join(tmpdir(), 'diligent-smoke-'));
  // The static server lives with the tests and is not part of the installed package.
  const { startStaticServer } = await import('../tests/support/static-server.mjs');
  const server = await startStaticServer(smokeRoutes());
  try {
    const run = await capture(process.execPath, [
      'bin/diligent.mjs', 'audit', '--url', `${server.origin}/`, 'smoke', '--allow-private', '--strict-origins', '--pages', '4',
      '--facts', 'references/facts/us-federal-public-web.example.json', '--root', workspace, '--json'
    ], { ...env, DILIGENT_RS_BIN: binary, DILIGENT_CHROME: chrome });
    check('audit exit code equals the expected gate exit', run.code === expected.gate_exit, `exit ${run.code}, expected ${expected.gate_exit}${run.code === expected.gate_exit ? '' : `\n${run.stderr.slice(-1500)}`}`);
    const dir = join(workspace, '.diligent', 'smoke');
    let summary = null;
    try { summary = JSON.parse(run.stdout); } catch { check('audit --json output parses', false, run.stdout.slice(0, 300)); }
    diagnostics.push(`audit exit ${run.code}`);
    if (summary) for (const stage of summary.stages) diagnostics.push(`stage ${stage.name}: ${stage.state}, exit ${stage.exit_code}${stage.warning ? `, ${stage.warning}` : ''}${stage.detail ? `\n${stage.detail.split('\n').map(l => `    ${l}`).join('\n')}` : ''}`);
    if (run.stderr.trim()) diagnostics.push(`audit stderr (last 20 lines):\n${run.stderr.trim().split('\n').slice(-20).map(l => `    ${l}`).join('\n')}`);
    if (summary) check('every stage ran without failing', summary.stages.every(s => s.state !== 'failed'), summary.stages.filter(s => s.state === 'failed').map(s => `${s.name}: ${s.warning}`).join('; '));

    for (const name of ['SCOPE.json', 'SERVICE-BRIEF.json', 'PROJECT-CONTEXT.json', 'AUTHORITIES.lock.json', 'RULES.lock.json', 'APPLICABILITY.json', 'INVENTORY.json', 'SAMPLE.json', 'AUDIT-PLAN.json', 'EVIDENCE.jsonl', 'RESULTS.jsonl', 'SIGNALS.jsonl', 'REVIEW-QUEUE.json', 'ASSESSMENT.json', 'EVENTS.jsonl']) {
      const loaded = await loadArtifact(dir, name);
      check(`${name} exists and validates`, loaded.exists && loaded.ok, loaded.exists ? loaded.errors.slice(0, 2).join('; ') : 'missing');
    }
    const runDir = summary?.run_id ? join(dir, 'runs', summary.run_id) : null;
    const runRecord = runDir ? await loadArtifact(runDir, 'RUN.json') : null;
    check('RUN.json is valid and the pipeline reached reported', runRecord?.ok && runRecord.data.pipeline_state === 'reported', runRecord?.data?.pipeline_state ?? 'no run record');

    if (['RESULTS.jsonl', 'SIGNALS.jsonl', 'EVIDENCE.jsonl', 'REVIEW-QUEUE.json'].every(name => existsSync(join(dir, name)))) {
      const counts = { results: await countLines(join(dir, 'RESULTS.jsonl')), signals: await countLines(join(dir, 'SIGNALS.jsonl')), evidence: await countLines(join(dir, 'EVIDENCE.jsonl')) };
      counts.review_items = JSON.parse(await readFile(join(dir, 'REVIEW-QUEUE.json'), 'utf8')).items.length;
      for (const [key, minimum] of Object.entries(expected.minimum)) check(`at least ${minimum} ${key.replace('_', ' ')}`, counts[key] >= minimum, `found ${counts[key]}`);
    }
    const report = existsSync(join(dir, 'REPORT.md')) ? await readFile(join(dir, 'REPORT.md'), 'utf8') : '';
    for (const heading of ['## 2', '## 4', '## 10. Craft and clarity']) check(`REPORT.md contains "${heading}"`, report.includes(heading));

    // SIGNALS.jsonl must have no effect on the gate: run it again with the file moved away.
    if (existsSync(join(dir, 'ASSESSMENT.json')) && existsSync(join(dir, 'SIGNALS.jsonl'))) {
      const before = await readFile(join(dir, 'ASSESSMENT.json'));
      const now = JSON.parse(before.toString('utf8')).computed_at;
      await rename(join(dir, 'SIGNALS.jsonl'), join(dir, 'SIGNALS.jsonl.away'));
      await capture(process.execPath, ['scripts/gate.mjs', '--dir', dir, '--now', now], env);
      const without = await readFile(join(dir, 'ASSESSMENT.json'));
      await rename(join(dir, 'SIGNALS.jsonl.away'), join(dir, 'SIGNALS.jsonl'));
      await capture(process.execPath, ['scripts/gate.mjs', '--dir', dir, '--now', now], env);
      const withFile = await readFile(join(dir, 'ASSESSMENT.json'));
      check('ASSESSMENT.json is byte-identical with and without SIGNALS.jsonl', before.equals(without) && before.equals(withFile));
    }
    if (existsSync(join(dir, 'captures'))) check('captures/ holds evidence payloads', (await readdir(join(dir, 'captures'))).length > 0);
  } finally {
    await server.close();
    if (!keep) await rm(workspace, { recursive: true, force: true });
  }
  return { status: checks.every(c => c.ok) ? 'passed' : 'failed', checks, diagnostics, workspace: keep ? workspace : undefined };
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2), { 'allow-missing-chrome': 'boolean', keep: 'boolean' });
  if (options.help) { console.log(HELP); return 0; }
  const result = await runSmoke({ allowMissingChrome: options['allow-missing-chrome'], keep: options.keep });
  if (result.status === 'skipped') { console.log(`e2e smoke: SKIPPED (${result.message})`); return 0; }
  for (const c of result.checks) console.log(`${c.ok ? 'ok     ' : 'FAILED '} ${c.name}${c.ok || !c.detail ? '' : `: ${c.detail}`}`);
  if (result.status === 'failed' && result.diagnostics?.length) console.log(`\naudit diagnostics:\n${result.diagnostics.join('\n')}\n`);
  if (result.workspace) console.log(`workspace kept at ${result.workspace}`);
  console.log(`e2e smoke: ${result.status === 'passed' ? 'PASSED' : 'FAILED'}`);
  return result.status === 'passed' ? 0 : 1;
}

if (isMain(import.meta.url)) runMain(main);
