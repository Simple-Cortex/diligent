/**
 * Baseline and diff logic.
 *
 * A baseline records the fingerprints of findings and results from a run so that a
 * subsequent run can partition its findings into new, known, resolved, and
 * unverified. This lets a team accept a backlog and block only new problems.
 */
import { readFile } from 'node:fs/promises';
import { loadArtifact, loadRunArtifact, validateData } from './artifacts.mjs';
import { DILIGENT_VERSION } from './cli.mjs';
import { resultFingerprint, extractRuleId } from './fingerprint.mjs';

/**
 * Entries of a run that a baseline records and a diff compares: results with status fail or needs_review
 * (source "result") and findings (source "finding"). A missing FINDINGS.json is fine; an invalid one is an error.
 * Returns { run, entries } with entries in input order.
 */
async function currentEntries(runDir) {
  const runArtifact = await loadArtifact(runDir, 'RUN.json');
  if (!runArtifact.exists) throw new Error('RUN.json not found in run directory');
  if (!runArtifact.ok) throw new Error(`RUN.json invalid: ${runArtifact.errors.join(', ')}`);
  const run = runArtifact.data;

  const resultsArtifact = await loadRunArtifact(runDir, 'RESULTS.jsonl');
  const results = resultsArtifact.exists && resultsArtifact.ok ? (resultsArtifact.data || []) : [];

  const findingsArtifact = await loadRunArtifact(runDir, 'FINDINGS.json');
  if (findingsArtifact.exists && !findingsArtifact.ok) throw new Error(`FINDINGS.json invalid: ${findingsArtifact.errors.join(', ')}`);
  const findings = findingsArtifact.exists ? (findingsArtifact.data.findings || []) : [];

  const entries = [];
  for (const r of results) {
    if (r.status !== 'fail' && r.status !== 'needs_review') continue;
    entries.push({
      source: 'result',
      fingerprint: resultFingerprint(r),
      rule_id: extractRuleId(r),
      check_id: r.check_id,
      instance_id: r.instance_id,
      first_seen_run: run.run_id
    });
  }
  for (const f of findings) {
    entries.push({
      source: 'finding',
      fingerprint: f.fingerprint,
      rule_id: f.rule_id,
      finding_id: f.finding_id,
      first_seen_run: run.run_id
    });
  }
  return { run, entries };
}

/**
 * Write a baseline from a run directory.
 * Returns the baseline object (caller serializes and writes to file).
 */
export async function writeBaseline(runDir) {
  const { run, entries } = await currentEntries(runDir);
  const fingerprints = entries.sort((a, b) => a.fingerprint < b.fingerprint ? -1 : a.fingerprint > b.fingerprint ? 1 : 0);
  return {
    schema_version: '1.0',
    created_from_run: run.run_id,
    engine_version: DILIGENT_VERSION,
    fingerprints
  };
}

/**
 * Load a baseline from a file path. Validates against the baseline schema.
 */
export async function loadBaseline(filePath) {
  const text = await readFile(filePath, 'utf8');
  let baseline;
  try { baseline = JSON.parse(text); } catch (e) {
    throw new Error(`baseline: not valid JSON (${e.message})`);
  }
  const validation = await validateData('baseline', baseline, 'baseline');
  if (!validation.ok) {
    throw new Error(`baseline: invalid: ${validation.errors.join(', ')}`);
  }
  return baseline;
}

/**
 * Determine which rules were executed in a run (from RESULTS.jsonl).
 * Returns a Map of rule_id -> true for rules that have at least one result.
 */
async function executedRules(runDir) {
  const resultsArtifact = await loadRunArtifact(runDir, 'RESULTS.jsonl');
  if (!resultsArtifact.exists) return new Map();
  const records = resultsArtifact.data || [];
  const executed = new Map();
  for (const r of records) {
    const ruleId = extractRuleId(r);
    executed.set(ruleId, true);
  }
  return executed;
}

/**
 * Diff the run's entries (results and findings) against a baseline.
 * Returns { new, known, resolved, unverified, warnings }.
 *
 * - new: in the current run, not in baseline
 * - known: in the current run and in baseline (by fingerprint)
 * - resolved: in baseline, not in the current run, and the rule was executed
 * - unverified: in baseline, not in the current run, and the rule was NOT executed
 */
export async function diffFindings(runDir, baseline) {
  const { entries: current } = await currentEntries(runDir);
  const baselineFps = new Map();
  const warnings = [];

  for (const entry of baseline.fingerprints) {
    if (baselineFps.has(entry.fingerprint)) {
      warnings.push(`baseline fingerprint collision: ${entry.fingerprint}`);
    }
    baselineFps.set(entry.fingerprint, entry);
  }

  const currentFps = new Map();
  for (const f of current) {
    if (currentFps.has(f.fingerprint)) {
      warnings.push(`run fingerprint collision: ${f.fingerprint}`);
    }
    currentFps.set(f.fingerprint, f);
  }

  const newEntries = [];
  const knownEntries = [];
  for (const f of current) {
    if (baselineFps.has(f.fingerprint)) {
      knownEntries.push(f);
    } else {
      newEntries.push(f);
    }
  }

  const executed = await executedRules(runDir);

  const resolved = [];
  const unverified = [];
  for (const [fp, entry] of baselineFps) {
    if (!currentFps.has(fp)) {
      if (executed.has(entry.rule_id)) {
        resolved.push(entry);
      } else {
        unverified.push(entry);
      }
    }
  }

  return {
    new: newEntries,
    known: knownEntries,
    resolved,
    unverified,
    warnings
  };
}
