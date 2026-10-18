/**
 * Merging per-page engine outputs into one audit's RESULTS, EVIDENCE and SIGNALS, and building
 * the review queue. Every inconsistency is a hard failure: a merged artifact that quietly
 * dropped or renamed a row would be worse than no artifact.
 */
import { copyFile, lstat, mkdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { atomicWriteJson, atomicWriteJsonl, loadArtifact, pathExists, sha256 } from '../artifacts.mjs';

export class MergeError extends Error {}

const SCHEME_REF = /^[a-z][a-z0-9+.-]*:/i;

async function readJsonl(path, label) {
  if (!(await pathExists(path))) return null;
  const rows = [];
  const lines = (await readFile(path, 'utf8')).split('\n');
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() === '') continue;
    try { rows.push(JSON.parse(lines[i])); } catch (error) { throw new MergeError(`${label} line ${i + 1}: not valid JSON (${error.message}).`); }
  }
  return rows;
}

function safeName(name, label) {
  if (typeof name !== 'string' || name === '' || name === '.' || name === '..' || /[\\/\0]/.test(name) || name.startsWith('.')) {
    throw new MergeError(`${label}: payload name ${JSON.stringify(String(name).slice(0, 80))} is not a plain file name.`);
  }
  return name;
}

/**
 * Where a payload ref points. Returns null for a non-file ref (for example dns:TXT:name),
 * else { name, source } with the file to copy into <audit>/captures/<name>.
 */
function resolvePayload(ref, source) {
  if (typeof ref !== 'string' || ref === '') throw new MergeError(`${source.label}: an evidence row has no payload_ref.`);
  if (!ref.includes('/') && SCHEME_REF.test(ref)) return null;
  const parts = ref.split('/');
  if (parts.length !== 2 || ref.startsWith('/')) throw new MergeError(`${source.label}: payload_ref ${JSON.stringify(ref.slice(0, 120))} is not captures/<name> or evidence/<name>.`);
  const [folder, rawName] = parts;
  const name = safeName(rawName, source.label);
  if (folder === 'captures') return { name, file: join(source.captureDir ?? source.dir, name) };
  if (folder === 'evidence' && source.kind === 'infra') return { name, file: join(source.dir, 'evidence', name) };
  throw new MergeError(`${source.label}: payload_ref ${JSON.stringify(ref.slice(0, 120))} is outside the payload root.`);
}

/**
 * sources: [{ kind: 'page'|'infra', label, dir, captureDir? }] where dir holds RESULTS.jsonl and
 * EVIDENCE.jsonl (and optionally SIGNALS.jsonl). Writes nothing; returns the merged rows.
 */
export async function mergeSources({ auditDir, runId, sources }) {
  const results = []; const evidence = []; const signals = [];
  const resultOwner = new Map(); const signalOwner = new Map(); const evidenceById = new Map();
  const captures = join(auditDir, 'captures');
  let sawSignals = false;
  const copied = new Set();

  for (const source of sources) {
    const rRows = (await readJsonl(join(source.dir, 'RESULTS.jsonl'), `${source.label} RESULTS.jsonl`)) ?? [];
    const eRows = (await readJsonl(join(source.dir, 'EVIDENCE.jsonl'), `${source.label} EVIDENCE.jsonl`)) ?? [];
    const sRows = await readJsonl(join(source.dir, 'SIGNALS.jsonl'), `${source.label} SIGNALS.jsonl`);
    if (sRows) sawSignals = true;

    for (const row of eRows) {
      if (row.run_id !== runId) throw new MergeError(`${source.label}: evidence ${row.evidence_id} carries run_id ${row.run_id}, expected ${runId}.`);
      const payload = resolvePayload(row.payload_ref, source);
      if (payload) {
        let info;
        try { info = await lstat(payload.file); } catch { throw new MergeError(`${source.label}: payload for evidence ${row.evidence_id} is missing (${payload.name}).`); }
        if (!info.isFile()) throw new MergeError(`${source.label}: payload ${payload.name} is not a regular file.`);
        const bytes = await readFile(payload.file);
        if (sha256(bytes) !== row.sha256) throw new MergeError(`${source.label}: payload ${payload.name} does not match the sha256 recorded for evidence ${row.evidence_id}.`);
        await mkdir(captures, { recursive: true });
        const target = join(captures, payload.name);
        if (await pathExists(target)) {
          if (sha256(await readFile(target)) !== row.sha256) throw new MergeError(`${source.label}: captures/${payload.name} already exists with different content.`);
        } else if (!copied.has(payload.name)) {
          await copyFile(payload.file, target);
        }
        copied.add(payload.name);
        row.payload_ref = `captures/${payload.name}`;
      }
      const seen = evidenceById.get(row.evidence_id);
      if (seen) {
        if (seen.sha256 !== row.sha256) throw new MergeError(`${source.label}: evidence ${row.evidence_id} appears twice with different content.`);
        continue;
      }
      evidenceById.set(row.evidence_id, row);
      evidence.push(row);
    }
    for (const row of rRows) {
      if (resultOwner.has(row.result_id)) throw new MergeError(`result_id ${row.result_id} appears in both ${resultOwner.get(row.result_id)} and ${source.label}. Two pages or checks produced the same id; nothing was merged.`);
      resultOwner.set(row.result_id, source.label);
      results.push(row);
    }
    for (const row of sRows ?? []) {
      if (row.run_id !== runId) throw new MergeError(`${source.label}: signal ${row.signal_id} carries run_id ${row.run_id}, expected ${runId}.`);
      if (signalOwner.has(row.signal_id)) throw new MergeError(`signal_id ${row.signal_id} appears in both ${signalOwner.get(row.signal_id)} and ${source.label}.`);
      signalOwner.set(row.signal_id, source.label);
      signals.push(row);
    }
  }
  for (const row of results) for (const ref of row.evidence_refs || []) {
    if (!evidenceById.has(ref)) throw new MergeError(`result ${row.result_id} cites evidence ${ref}, which no source recorded.`);
  }
  for (const row of signals) for (const ref of row.evidence_refs || []) {
    if (!evidenceById.has(ref)) throw new MergeError(`signal ${row.signal_id} cites evidence ${ref}, which no source recorded.`);
  }
  return { results, evidence, signals: sawSignals ? signals : null, payloads: copied.size };
}

/** One blocking human review item per needs_review result. EXCEPTIONS.json is never touched. */
export function buildReviewQueue({ runId, results, rules }) {
  const items = [];
  for (const row of results) {
    if (row.status !== 'needs_review') continue;
    const entry = rules.get(row.obligation_id);
    const observed = typeof row.observed === 'string' ? row.observed : JSON.stringify(row.observed ?? '');
    items.push({
      item_id: `rev-${row.result_id}`,
      kind: 'human',
      task: `result_review: ${row.check_id} on ${row.instance_id}`,
      owner_role: entry?.rule.owner_role ?? 'reviewer',
      blocking_reason: (observed.replace(/\s+/g, ' ').trim() || 'The check could not decide; a person must confirm the result.').slice(0, 400),
      obligation_id: row.obligation_id,
      status: 'open',
      evidence_refs: [...(row.evidence_refs || [])],
      resolution_evidence_refs: []
    });
  }
  return { schema_version: '1.0.0', run_id: runId, items };
}

/** Write the merged artifacts and validate each by reading it back through its schema. */
export async function writeMerged({ auditDir, runId, merged, rules }) {
  await atomicWriteJsonl(join(auditDir, 'RESULTS.jsonl'), merged.results);
  await atomicWriteJsonl(join(auditDir, 'EVIDENCE.jsonl'), merged.evidence);
  if (merged.signals) await atomicWriteJsonl(join(auditDir, 'SIGNALS.jsonl'), merged.signals);
  const queue = buildReviewQueue({ runId, results: merged.results, rules });
  await atomicWriteJson(join(auditDir, 'REVIEW-QUEUE.json'), queue);
  const names = ['RESULTS.jsonl', 'EVIDENCE.jsonl', 'REVIEW-QUEUE.json', ...(merged.signals ? ['SIGNALS.jsonl'] : [])];
  for (const name of names) {
    const loaded = await loadArtifact(auditDir, name);
    if (!loaded.ok) throw new MergeError(`merged ${name} failed its schema: ${loaded.errors.slice(0, 3).join('; ')}`);
  }
  return { reviewItems: queue.items.length };
}
