/** Workspace resolution, artifact I/O, atomic writes and the event log. */
import { createHash, randomBytes } from 'node:crypto';
import { appendFile, mkdir, readFile, readdir, rename, rm, stat, writeFile, open } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { dirname, join, resolve, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { validate } from './schema.mjs';

export const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const AUDIT_ID_PATTERN = /^[a-z0-9]+(-[a-z0-9]+)*$/;
export const SCHEMA_VERSION = '1.0.0';

/** Artifact filename -> schema name. `jsonl` artifacts hold one record per line. */
export const ARTIFACTS = Object.freeze({
  'SCOPE.json': { schema: 'scope' },
  'SERVICE-BRIEF.json': { schema: 'service-brief' },
  'PROJECT-CONTEXT.json': { schema: 'project-context' },
  'AGENCY-PROFILE.json': { schema: 'agency-profile' },
  'REGULATORY-DISCOVERY.json': { schema: 'regulatory-discovery' },
  'DESIGN-SYSTEM.json': { schema: 'design-system' },
  'RESOURCE-CATALOG.json': { schema: 'resource-catalog' },
  'AUTHORITIES.lock.json': { schema: 'authorities-lock' },
  'RULES.lock.json': { schema: 'rules-lock' },
  'APPLICABILITY.json': { schema: 'applicability' },
  'INVENTORY.json': { schema: 'inventory' },
  'SAMPLE.json': { schema: 'sample' },
  'AUDIT-PLAN.json': { schema: 'audit-plan' },
  'EVIDENCE.jsonl': { schema: 'evidence-record', jsonl: true },
  'RESULTS.jsonl': { schema: 'check-result', jsonl: true },
  'SIGNALS.jsonl': { schema: 'signal', jsonl: true },
  'FINDINGS.json': { schema: 'findings' },
  'REVIEW-QUEUE.json': { schema: 'review-queue' },
  'REPAIRS.json': { schema: 'repairs' },
  'EXCEPTIONS.json': { schema: 'exceptions' },
  'VERIFICATION.json': { schema: 'verification' },
  'COMPONENT-BASELINE.json': { schema: 'component-baseline' },
  'PROJECT-RULES.json': { schema: 'project-rules' },
  'COMPONENT-BACKLOG.json': { schema: 'component-backlog' },
  'ASSESSMENT.json': { schema: 'assessment' },
  'EVENTS.jsonl': { schema: 'events', jsonl: true }
});

export function sha256(data) {
  return createHash('sha256').update(data).digest('hex');
}

export async function sha256File(path) {
  return sha256(await readFile(path));
}

export function nowIso() {
  return new Date().toISOString();
}

/** Workspace root: --root, DILIGENT_ROOT, nearest ancestor containing .diligent/, else cwd. */
export function resolveRoot({ root = null, env = process.env, cwd = process.cwd() } = {}) {
  if (root) return resolve(root);
  if (env.DILIGENT_ROOT) return resolve(env.DILIGENT_ROOT);
  let dir = resolve(cwd);
  for (;;) {
    if (existsSync(join(dir, '.diligent'))) return dir;
    const parent = dirname(dir);
    if (parent === dir) return resolve(cwd);
    dir = parent;
  }
}

export function assertAuditId(auditId) {
  if (typeof auditId !== 'string' || !AUDIT_ID_PATTERN.test(auditId)) {
    throw new UsageError(`audit id "${auditId ?? ''}" is not valid: use kebab-case letters and digits, for example "public-site".`);
  }
  return auditId;
}

export class UsageError extends Error {}

export function auditDirFor(root, auditId) {
  return join(root, '.diligent', assertAuditId(auditId));
}

/** Write a file atomically: temp file in the same directory, flush, then rename. */
export async function atomicWrite(path, content) {
  await mkdir(dirname(path), { recursive: true });
  const tmp = join(dirname(path), `.${basename(path)}.tmp-${process.pid}-${randomBytes(4).toString('hex')}`);
  try {
    const handle = await open(tmp, 'w');
    try { await handle.writeFile(content); await handle.sync(); } finally { await handle.close(); }
    await rename(tmp, path);
  } catch (error) {
    await rm(tmp, { force: true });
    throw error;
  }
}

export async function atomicWriteJson(path, value) {
  await atomicWrite(path, `${JSON.stringify(value, null, 2)}\n`);
}

export async function atomicWriteJsonl(path, records) {
  await atomicWrite(path, records.map(record => JSON.stringify(record)).join('\n') + (records.length ? '\n' : ''));
}

const schemaCache = new Map();
export async function loadSchema(name, packageRoot = PACKAGE_ROOT) {
  const key = `${packageRoot}::${name}`;
  if (!schemaCache.has(key)) {
    const path = join(packageRoot, 'schemas', `${name}.schema.json`);
    schemaCache.set(key, JSON.parse(await readFile(path, 'utf8')));
  }
  return schemaCache.get(key);
}

export async function pathExists(path) {
  try { await stat(path); return true; } catch (error) { if (error.code === 'ENOENT') return false; throw error; }
}

function formatErrors(label, errors, prefix = '') {
  return errors.map(error => `${label}${prefix} ${error.path}: ${error.message}`);
}

/** Validate in-memory data against a named schema. Returns { ok, errors: string[] }. */
export async function validateData(schemaName, data, label, { packageRoot = PACKAGE_ROOT, prefix = '' } = {}) {
  const schema = await loadSchema(schemaName, packageRoot);
  const result = validate(schema, data);
  return { ok: result.ok, errors: formatErrors(label, result.errors, prefix) };
}

/**
 * Load an artifact for a run directory. A real audit keeps RUN.json under <audit>/runs/<run-id>/ and the
 * result artifacts in <audit>/, while fixtures keep everything in one directory. RUN.json always comes from
 * runDir. Any other artifact comes from runDir when it exists there, else from the audit directory when
 * runDir is <audit>/runs/<run-id>.
 */
export async function loadRunArtifact(runDir, name, options) {
  const local = await loadArtifact(runDir, name, options);
  if (name === 'RUN.json' || local.exists) return local;
  if (basename(dirname(resolve(runDir))) === 'runs') return loadArtifact(dirname(dirname(resolve(runDir))), name, options);
  return local;
}

/**
 * Read and validate one artifact from an audit directory.
 * Returns { name, path, exists, ok, errors, data }. `data` is an array for JSONL artifacts.
 */
export async function loadArtifact(dir, name, { packageRoot = PACKAGE_ROOT } = {}) {
  const spec = ARTIFACTS[name] || (name === 'RUN.json' ? { schema: 'run' } : null);
  if (!spec) throw new Error(`Unknown artifact ${name}`);
  const path = join(dir, name);
  if (!(await pathExists(path))) return { name, path, exists: false, ok: false, errors: [], data: null };
  let text;
  try { text = await readFile(path, 'utf8'); } catch (error) {
    return { name, path, exists: true, ok: false, errors: [`${name}: cannot read file: ${error.message}`], data: null };
  }
  if (spec.jsonl) {
    const records = [];
    const errors = [];
    const lines = text.split('\n');
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].trim() === '') continue;
      let record;
      try { record = JSON.parse(lines[i]); } catch {
        errors.push(`${name} line ${i + 1}: not valid JSON`);
        continue;
      }
      const result = await validateData(spec.schema, record, name, { packageRoot, prefix: ` line ${i + 1}` });
      errors.push(...result.errors);
      records.push(record);
    }
    return { name, path, exists: true, ok: errors.length === 0, errors, data: records };
  }
  let data;
  try { data = JSON.parse(text); } catch {
    return { name, path, exists: true, ok: false, errors: [`${name}: not valid JSON`], data: null };
  }
  const result = await validateData(spec.schema, data, name, { packageRoot });
  return { name, path, exists: true, ok: result.ok, errors: result.errors, data };
}

/** Append one event line to EVENTS.jsonl. */
export async function appendEvent(dir, { run_id, stage, event, artifact = null, sha256: hash = null, detail }) {
  const record = { schema_version: SCHEMA_VERSION, ts: nowIso(), run_id, stage, event, artifact, sha256: hash };
  if (detail) record.detail = detail;
  await mkdir(dir, { recursive: true });
  await appendFile(join(dir, 'EVENTS.jsonl'), `${JSON.stringify(record)}\n`);
  return record;
}

export function newRunId(date = new Date()) {
  return `run-${date.toISOString().slice(0, 10)}-${randomBytes(3).toString('hex')}`;
}

/** Most recently started run under .diligent/<audit-id>/runs/. Returns { runId, path, data } or null. */
export async function latestRun(dir) {
  const runsDir = join(dir, 'runs');
  if (!(await pathExists(runsDir))) return null;
  const candidates = [];
  for (const entry of await readdir(runsDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const path = join(runsDir, entry.name, 'RUN.json');
    if (!(await pathExists(path))) continue;
    try {
      const data = JSON.parse(await readFile(path, 'utf8'));
      candidates.push({ runId: entry.name, path, data, startedAt: String(data.started_at ?? '') });
    } catch { candidates.push({ runId: entry.name, path, data: null, startedAt: '' }); }
  }
  candidates.sort((a, b) => (a.startedAt < b.startedAt ? 1 : a.startedAt > b.startedAt ? -1 : 0));
  return candidates[0] || null;
}

/** Locate the audit directory from CLI-style options. */
export function locateAuditDir({ auditId, dir, root, env, cwd }) {
  if (dir) return resolve(dir);
  if (!auditId) throw new UsageError('an <audit-id> (or --dir <audit-directory>) is required.');
  return auditDirFor(resolveRoot({ root, env, cwd }), auditId);
}

export const PIPELINE = Object.freeze(['initialized','discovered','scoped','planned','collecting','assessed','proposed','verified','reported']);

/**
 * Move a run's pipeline_state forward (never backward) and apply a patch.
 * Leaves RUN.json untouched when it is missing or invalid.
 */
export async function advanceRun(dir, runId, state, patch = {}) {
  const path = join(dir, 'runs', runId, 'RUN.json');
  if (!(await pathExists(path))) return false;
  let run;
  try { run = JSON.parse(await readFile(path, 'utf8')); } catch { return false; }
  if (PIPELINE.indexOf(state) > PIPELINE.indexOf(run.pipeline_state)) run.pipeline_state = state;
  Object.assign(run, patch);
  if (!(await validateData('run', run, 'RUN.json')).ok) return false;
  await atomicWriteJson(path, run);
  return true;
}

export async function writeText(path, text) {
  await atomicWrite(path, text);
}

export { writeFile };
