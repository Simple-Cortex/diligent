/**
 * `diligent audit`: the Node skill orchestrates, the diligent-rs binary executes, and Node
 * stays authoritative for the gate and the report. Every stage appends events, writes its
 * artifacts atomically and records its outcome; nothing here decides a result.
 */
import { copyFile, mkdir, readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  PACKAGE_ROOT, UsageError, advanceRun, appendEvent, atomicWriteJson, loadArtifact, nowIso, pathExists, resolveRoot, sha256File, validateData
} from '../artifacts.mjs';
import { ENGINE_MISSING_MESSAGE, pageSlug, probeBinary, runEngine, runNode, tryJson } from './engine.mjs';
import { applyFacts, assertKnownFacts, knownFactIds, readFactsInput } from './facts.mjs';
import { MergeError, mergeSources, writeMerged } from './merge.mjs';
import { buildAuditPlan, loadProfileData, selectPages } from './plan.mjs';
import { loadReplay, replayInventory } from './replay.mjs';

const DETAIL_CAP = 4096;
/** First `lines` lines of a child's stderr, one line each, for a stage record. */
const stderrHead = (text, lines = 20) => String(text ?? '').split('\n').map(l => l.trimEnd()).filter(Boolean).slice(0, lines).map(l => `    ${l}`).join('\n');
/** One diagnostic block for a failed engine call: exit code, error kind and message from --json, then stderr. */
function failureDetail(label, result) {
  const error = tryJson(result.stdout)?.error;
  const head = `${label}: exit ${result.code}${error?.kind ? `, kind ${error.kind}` : ''}${error?.message ? `, message: ${String(error.message).replace(/\s+/g, ' ').slice(0, 300)}` : ''}`;
  const tail = stderrHead(result.stderr);
  return tail ? `${head}\n${tail}` : head;
}
const capDetail = parts => (parts.length ? parts.join('\n').slice(0, DETAIL_CAP) : null);

const SCRIPTS = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const EVENT_STAGE = {
  init: 'initialized', scope: 'scoped', applicability: 'scoped', inventory: 'scoped', plan: 'planned',
  collect: 'collecting', assess: 'collecting', infra: 'collecting', merge: 'collecting', gate: 'assessed', report: 'reported'
};

export function defaultAuditId(url) {
  const host = new URL(url).hostname.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  return host || 'audit';
}

export function validateOptions(options) {
  let url;
  try { url = new URL(options.url); } catch { throw new UsageError(`--url "${options.url ?? ''}" is not a valid URL.`); }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') throw new UsageError('--url must be an http or https URL.');
  if (url.username || url.password) throw new UsageError('--url must not contain credentials.');
  const pages = options.pages === undefined ? 10 : Number(options.pages);
  if (!Number.isInteger(pages) || pages < 1 || pages > 200) throw new UsageError('--pages must be an integer from 1 to 200.');
  const timeoutMs = options.timeoutMs === undefined ? 30000 : Number(options.timeoutMs);
  if (!Number.isInteger(timeoutMs) || timeoutMs < 1000 || timeoutMs > 600000) throw new UsageError('--timeout-ms must be an integer from 1000 to 600000.');
  if (options.corroborate !== undefined && options.corroborate !== 'axe') throw new UsageError('--corroborate supports only "axe".');
  if (options.corroborate !== undefined && options.noCorroborate) throw new UsageError('--corroborate and --no-corroborate cannot be used together.');
  return { url, pages, timeoutMs };
}

/**
 * Run the whole chain. Resolves to { exit, summary, message? }. Throws UsageError for bad input
 * before anything is written.
 */
export async function runAudit(options, { log = () => {}, env = process.env } = {}) {
  // Replay mode audits recorded captures offline: no discovery, no collection, no network stages.
  if (options.replay && options.url) throw new UsageError('--replay and --url cannot be used together.');
  const replay = options.replay ? await loadReplay(resolve(options.replay)) : null;
  const { url, pages: requestedPages, timeoutMs } = validateOptions(replay ? { ...options, url: `${replay.origin}/` } : options);
  const pageBudget = replay ? replay.pages.length : requestedPages;
  const origin = url.origin;
  const startUrl = url.href;
  const dataRoot = options.dataRoot ? resolve(options.dataRoot) : PACKAGE_ROOT;
  const profileId = options.profile || 'us-federal-public-web';

  const engine = probeBinary(env);
  if (!engine.ok) return { exit: 3, message: ENGINE_MISSING_MESSAGE, summary: null };

  let profileData;
  try { profileData = await loadProfileData(dataRoot, profileId); } catch (error) {
    if (error.code === 'ENOENT') throw new UsageError(`profile "${profileId}" does not exist under ${join(dataRoot, 'profiles')}.`);
    throw error;
  }
  // The profile may ask for axe-core; --corroborate axe forces it on and --no-corroborate forces it off.
  const profileAsksAxe = (profileData.profile.corroboration?.browser || []).includes('axe-core');
  const corroboration = options.noCorroborate
    ? { applied: false, source: 'flag_off', profile_requested: profileAsksAxe }
    : options.corroborate
      ? { applied: true, source: 'flag', profile_requested: profileAsksAxe }
      : { applied: profileAsksAxe, source: profileAsksAxe ? 'profile' : 'none', profile_requested: profileAsksAxe };
  const facts = await readFactsInput({ file: options.facts ? resolve(options.facts) : null, pairs: options.fact || [] });
  assertKnownFacts(facts, knownFactIds(profileData), profileId);
  const processesFile = options.processes ? resolve(options.processes) : null;
  if (processesFile && !(await pathExists(processesFile))) throw new UsageError(`--processes ${processesFile} does not exist.`);

  const auditId = options.auditId || defaultAuditId(startUrl);
  const replayCaptures = new Map((replay?.pages || []).map(page => [page.url, page.captures]));
  const root = resolveRoot({ root: options.root, env });
  const dir = join(root, '.diligent', auditId);
  const work = join(dir, 'work');
  const stages = [];
  const warnings = [];
  const pageRecords = [];
  let runId = null;
  let failed = false;
  let gateExit = null;
  let reportPath = null;
  let sample = null;
  let infraRan = false;
  let reviewItems = 0;

  const record = async (name, fn) => {
    const started = Date.now();
    log(`stage ${name}: started`);
    if (runId) await appendEvent(dir, { run_id: runId, stage: EVENT_STAGE[name], event: `stage_${name}_started` });
    const announced = Boolean(runId);
    let outcome;
    try { outcome = (await fn()) || {}; } catch (error) {
      if (error instanceof UsageError) throw error;
      outcome = { state: 'failed', exit_code: 3, warning: error.message };
    }
    // init creates the run id, so its start event can only be written afterwards.
    if (runId && !announced) await appendEvent(dir, { run_id: runId, stage: EVENT_STAGE[name], event: `stage_${name}_started` });
    const entry = { name, state: outcome.state || 'done', exit_code: outcome.exit_code ?? 0, duration_ms: Date.now() - started, warning: outcome.warning ?? null, detail: outcome.detail ?? null };
    stages.push(entry);
    if (entry.warning) warnings.push(`${name}: ${entry.warning}`);
    if (runId) await appendEvent(dir, { run_id: runId, stage: EVENT_STAGE[name], event: `stage_${name}_${entry.state}`, detail: entry.warning ? entry.warning.slice(0, 500) : `exit ${entry.exit_code}` });
    log(`stage ${name}: ${entry.state}${entry.warning ? ` (${entry.warning})` : ''}`);
    if (entry.state === 'failed') failed = true;
    return entry;
  };
  const patchRun = patch => advanceRun(dir, runId, 'initialized', patch);

  // 1. init
  await record('init', async () => {
    const result = await runNode(join(SCRIPTS, 'init.mjs'), [auditId, '--profile', profileId, '--mode', 'page', '--root', root, '--data-root', dataRoot, '--json'], { env });
    const summary = tryJson(result.stdout);
    if (result.code !== 0 || !summary?.run_id) {
      if (result.code === 2) throw new UsageError(result.stderr.replace(/^diligent: /, '').trim() || 'init rejected the arguments');
      return { state: 'failed', exit_code: 3, warning: `init failed (exit ${result.code}): ${result.stderr.trim().slice(0, 300)}` };
    }
    runId = summary.run_id;
    const existing = await loadArtifact(join(dir, 'runs', runId), 'RUN.json');
    await patchRun({
      tool_versions: { ...(existing.data?.tool_versions || {}), 'diligent-rs': engine.binary_version ?? 'unknown' },
      execution_status: 'running',
      engine: { name: 'native-rust', executed_by: 'mixed', native_available: true, protocol_version: engine.protocol_version, binary_version: engine.binary_version },
      budgets: { pages: pageBudget, timeout_ms: timeoutMs },
      requested_scope: { modes: ['page'], profile_id: profileId, detail: replay ? `diligent audit --replay (${replay.pages.length} recorded pages on ${origin})` : `diligent audit --url ${startUrl}` }
    });
    return {};
  });
  if (failed) return finish();

  // 2. scope: facts and the explicit permission grant
  await record('scope', async () => {
    const scopePath = join(dir, 'SCOPE.json');
    const scope = JSON.parse(await readFile(scopePath, 'utf8'));
    const now = nowIso();
    const changed = applyFacts(scope, facts, { now, origin, grantNetwork: !replay });
    const check = await validateData('scope', scope, 'SCOPE.json');
    if (!check.ok) return { state: 'failed', exit_code: 3, warning: `SCOPE.json would be invalid: ${check.errors.slice(0, 2).join('; ')}` };
    await atomicWriteJson(scopePath, scope);
    await appendEvent(dir, { run_id: runId, stage: 'scoped', event: replay ? 'scope_replay_recorded' : 'scope_permissions_recorded', artifact: 'SCOPE.json', sha256: await sha256File(scopePath), detail: `${replay ? `replay mode: no network access was granted for ${origin}` : `operator granted network access to ${origin}`}; facts recorded: ${changed.filter(id => id !== 'allowed_origins').join(', ') || 'none'}` });
    await advanceRun(dir, runId, 'scoped');
    if (Object.keys(facts).length === 0) return { warning: 'no facts were supplied (--facts or --fact), so applicability stays unknown and the gate will be incomplete' };
    return {};
  });
  if (failed) return finish();

  // 3. applicability
  await record('applicability', async () => {
    const out = join(dir, 'APPLICABILITY.json');
    const result = await runEngine(['applicability', '--scope', join(dir, 'SCOPE.json'), '--profile', profileId, '--data-root', dataRoot, '--run-id', runId, '--out', out, '--json'], { env });
    if (result.code !== 0) return { state: 'failed', exit_code: result.code, detail: capDetail([failureDetail('applicability', result)]), warning: `applicability exited ${result.code}: ${result.stderr.trim().slice(0, 300)}` };
    const loaded = await loadArtifact(dir, 'APPLICABILITY.json');
    if (!loaded.ok) return { state: 'failed', exit_code: 3, warning: `APPLICABILITY.json is invalid: ${loaded.errors.slice(0, 2).join('; ')}` };
    return {};
  });
  if (failed) return finish();

  // 4. inventory
  await record('inventory', async () => {
    if (replay) {
      const { inventory, sample: replaySample } = replayInventory({ replay, runId, auditId });
      for (const [name, schema, doc] of [['INVENTORY.json', 'inventory', inventory], ['SAMPLE.json', 'sample', replaySample]]) {
        const check = await validateData(schema, doc, name);
        if (!check.ok) return { state: 'failed', exit_code: 3, warning: `${name} built from the replay list is invalid: ${check.errors.slice(0, 2).join('; ')}` };
        await atomicWriteJson(join(dir, name), doc);
      }
      sample = replaySample;
      return { state: 'done', warning: 'replay: no discovery performed, so the sample is exactly the replay list' };
    }
    const out = join(work, 'inventory');
    const args = ['inventory', '--url', startUrl, '--out', out, '--run-id', runId, '--audit-id', auditId, '--json'];
    if (options.seed) args.push('--seed', options.seed);
    if (processesFile) args.push('--processes', processesFile);
    if (options.allowPrivate) args.push('--allow-private');
    const result = await runEngine(args, { env });
    if (result.code !== 0 && result.code !== 2) return { state: 'failed', exit_code: result.code, detail: capDetail([failureDetail('inventory', result)]), warning: `inventory exited ${result.code}: ${result.stderr.trim().slice(0, 300)}` };
    for (const name of ['INVENTORY.json', 'SAMPLE.json']) {
      if (!(await pathExists(join(out, name)))) return { state: 'failed', exit_code: 3, warning: `inventory wrote no ${name}` };
      await copyFile(join(out, name), join(dir, name));
      const loaded = await loadArtifact(dir, name);
      if (!loaded.ok) return { state: 'failed', exit_code: 3, warning: `${name} is invalid: ${loaded.errors.slice(0, 2).join('; ')}` };
    }
    sample = (await loadArtifact(dir, 'SAMPLE.json')).data;
    return result.code === 2 ? { state: 'partial', exit_code: 2, warning: 'the inventory is partial (budget, sitemap limits or an incomplete process); coverage is lower than the sample suggests' } : {};
  });
  if (failed) return finish();

  // 5. plan
  const selection = selectPages(sample, pageBudget, origin);
  await record('plan', async () => {
    const applicability = (await loadArtifact(dir, 'APPLICABILITY.json')).data;
    const plan = buildAuditPlan({ runId, auditId, now: nowIso(), applicability, rules: profileData.rules, pages: selection.pages, sample, budgets: { pages: pageBudget, timeout_ms: timeoutMs } });
    const check = await validateData('audit-plan', plan, 'AUDIT-PLAN.json');
    if (!check.ok) return { state: 'failed', exit_code: 3, warning: `AUDIT-PLAN.json would be invalid: ${check.errors.slice(0, 2).join('; ')}` };
    const path = join(dir, 'AUDIT-PLAN.json');
    await atomicWriteJson(path, plan);
    await appendEvent(dir, { run_id: runId, stage: 'planned', event: 'artifact_written', artifact: 'AUDIT-PLAN.json', sha256: await sha256File(path) });
    await advanceRun(dir, runId, 'planned');
    const notes = [];
    if (selection.skipped.length) notes.push(`${selection.skipped.length} sampled URL${selection.skipped.length === 1 ? ' is' : 's are'} outside ${origin} and will not be collected`);
    if (selection.pages.length === 0) notes.push('the sample holds no collectable pages, so no page is tested');
    return notes.length ? { warning: notes.join('; ') } : {};
  });
  if (failed) return finish();

  // 6. collect
  await advanceRun(dir, runId, 'collecting');
  await record('collect', async () => {
    if (replay) {
      const missing = [];
      for (const page of selection.pages) {
        const captures = replayCaptures.get(page.url);
        const rec = { url: page.url, slug: page.slug, collect: 'supplied', assess: 'pending', captureDir: captures };
        pageRecords.push(rec);
        if (!captures || !(await pathExists(join(captures, 'manifest.jsonl')))) { rec.collect = 'failed'; rec.error = 'capture directory missing'; missing.push(`${page.url}: no manifest.jsonl in ${captures}`); }
      }
      if (missing.length) return { state: 'failed', exit_code: 3, detail: capDetail(missing), warning: `${missing.length} replay page${missing.length === 1 ? ' has' : 's have'} no capture directory with a manifest.jsonl` };
      return { state: 'skipped', warning: 'replay: captures supplied, no collection performed' };
    }
    let ok = 0; let blocked = 0; let broken = 0;
    const details = [];
    for (const page of selection.pages) {
      const rec = { url: page.url, slug: page.slug, collect: 'pending', assess: 'pending' };
      pageRecords.push(rec);
      const args = ['collect', '--url', page.url, '--out', join(work, 'captures', page.slug), '--allow-origin', origin, '--no-screenshots', '--timeout-ms', String(timeoutMs), '--json'];
      if (options.strictOrigins) args.push('--strict-origins');
      if (corroboration.applied) args.push('--corroborate', 'axe');
      if (options.chromePath) args.push('--chrome-path', options.chromePath);
      const result = await runEngine(args, { env });
      if (result.code === 0) { rec.collect = 'collected'; ok++; continue; }
      const kind = tryJson(result.stdout)?.error?.kind ?? null;
      details.push(failureDetail(page.url, result));
      if (result.code === 2) { rec.collect = 'blocked'; rec.error = kind ?? 'blocked'; blocked++; warnings.push(`collect: ${page.url} was blocked (${rec.error})`); } else { rec.collect = 'failed'; rec.error = kind ?? `exit ${result.code}`; broken++; warnings.push(`collect: ${page.url} failed (${rec.error}); ${result.stderr.trim().slice(0, 200)}`); }
    }
    if (selection.pages.length > 0 && ok === 0) return { state: 'partial', exit_code: 3, detail: capDetail(details), warning: `no page could be collected (${blocked} blocked, ${broken} failed). Is Chrome installed? Set DILIGENT_CHROME or pass --chrome-path` };
    return blocked + broken ? { state: 'partial', exit_code: 2, detail: capDetail(details), warning: `${ok} of ${selection.pages.length} pages collected; ${blocked} blocked and ${broken} failed pages are untested` } : {};
  });

  if (failed) return finish();

  // 7. assess
  await record('assess', async () => {
    let ok = 0; let broken = 0;
    const details = [];
    for (const rec of pageRecords) {
      if (rec.collect !== 'collected' && rec.collect !== 'supplied') { rec.assess = 'not_run'; continue; }
      const out = join(work, 'assess', rec.slug);
      const args = ['assess', '--capture', rec.captureDir ?? join(work, 'captures', rec.slug), '--out', out, '--run-id', runId, '--inventory', join(dir, 'INVENTORY.json'), '--json'];
      if (options.noSignals) args.push('--no-signals');
      // Corroboration off means off: a capture that carries an axe_result must not be corroborated.
      if (!corroboration.applied) args.push('--no-corroborate');
      const result = await runEngine(args, { env });
      if ([0, 1, 2].includes(result.code) && (await pathExists(join(out, 'RESULTS.jsonl')))) { rec.assess = 'assessed'; ok++; } else { rec.assess = 'failed'; rec.error = `assess exit ${result.code}`; broken++; details.push(failureDetail(rec.url, result)); warnings.push(`assess: ${rec.url} failed (exit ${result.code}); ${result.stderr.trim().slice(0, 200)}`); }
    }
    return broken ? { state: 'partial', exit_code: 2, detail: capDetail(details), warning: `${broken} page${broken === 1 ? '' : 's'} could not be assessed` } : {};
  });

  // 8. infra (https only)
  await record('infra', async () => {
    if (replay) return { state: 'skipped', warning: 'replay: network stages skipped' };
    if (url.protocol !== 'https:') return { state: 'skipped', warning: `infra checks were skipped: ${origin} is not https, so HTTPS redirect, certificate, HSTS and header checks do not apply and are untested` };
    const out = join(work, 'infra');
    const args = ['infra', '--url', origin, '--out', out, '--run-id', runId, '--timeout-ms', String(timeoutMs), '--json'];
    if (options.allowPrivate) args.push('--allow-private');
    const result = await runEngine(args, { env });
    if ([0, 1, 2].includes(result.code) && (await pathExists(join(out, 'RESULTS.jsonl')))) { infraRan = true; return {}; }
    return { state: 'partial', exit_code: result.code, detail: capDetail([failureDetail(origin, result)]), warning: `infra exited ${result.code} and wrote no results: ${result.stderr.trim().slice(0, 200)}` };
  });

  // 9. merge
  await record('merge', async () => {
    const sources = pageRecords.filter(rec => rec.assess === 'assessed').map(rec => ({ kind: 'page', label: `page ${rec.url}`, dir: join(work, 'assess', rec.slug), captureDir: rec.captureDir ?? join(work, 'captures', rec.slug) }));
    if (infraRan) sources.push({ kind: 'infra', label: 'infra', dir: join(work, 'infra') });
    const merged = await mergeSources({ auditDir: dir, runId, sources });
    ({ reviewItems } = await writeMerged({ auditDir: dir, runId, merged, rules: profileData.rules }));
    for (const rec of pageRecords.filter(r => r.assess === 'assessed')) {
      const manifest = join(rec.captureDir ?? join(work, 'captures', rec.slug), 'manifest.jsonl');
      if (await pathExists(manifest)) { await mkdir(join(dir, 'captures'), { recursive: true }); await copyFile(manifest, join(dir, 'captures', `manifest-${rec.slug}.jsonl`)); }
    }
    return sources.length === 0 ? { warning: 'no source produced results, so RESULTS.jsonl is empty and every applicable obligation is untested' } : {};
  });
  if (failed) return finish();

  await patchRun({ snapshot_refs: pageRecords.filter(r => r.collect === 'collected' || r.collect === 'supplied').map(r => r.url) });

  // 10. gate, 11. report: child processes so the scripts keep their own exit handling
  await record('gate', async () => {
    const result = await runNode(join(SCRIPTS, 'gate.mjs'), ['--dir', dir], { env });
    gateExit = result.code;
    return result.code === 3 ? { state: 'failed', exit_code: 3, warning: `gate exited 3: ${result.stderr.trim().slice(0, 300)}` } : { exit_code: result.code };
  });
  if (!failed) {
    await record('report', async () => {
      const result = await runNode(join(SCRIPTS, 'report.mjs'), ['--dir', dir, '--data-root', dataRoot], { env });
      if (result.code !== 0) return { state: 'failed', exit_code: result.code, warning: `report exited ${result.code}: ${result.stderr.trim().slice(0, 300)}` };
      reportPath = join(dir, 'REPORT.md');
      return {};
    });
  }
  return finish();

  async function finish() {
    if (runId) {
      await advanceRun(dir, runId, 'initialized', {
        execution_status: failed ? 'failed' : 'completed', ended_at: nowIso(),
        warnings: [`Automated checks ran through diligent-rs ${engine.binary_version ?? 'unknown version'}. Anything not listed in RESULTS.jsonl is untested.`, ...warnings].slice(0, 200),
        extensions: { audit: { mode: replay ? 'replay' : 'live', ...(replay ? { replay_sha256: replay.sha256 } : {}), corroboration, warnings, pages: pageRecords, stages, review_items: reviewItems } }
      });
    }
    const summary = { audit_id: auditId, run_id: runId, directory: dir, stages, gate_exit: gateExit, report_path: reportPath, corroboration: { applied: corroboration.applied, source: corroboration.source } };
    return { exit: gateExit ?? 3, summary, warnings };
  }
}
