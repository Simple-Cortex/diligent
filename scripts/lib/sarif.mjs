/**
 * SARIF 2.1.0 emitter. Converts a Diligent run directory into a SARIF document
 * that GitHub code scanning accepts. Every result traces back to a finding or
 * check result and its evidence.
 *
 * Inputs: RESULTS.jsonl (primary), FINDINGS.json (optional), RUN.json, REVIEW-QUEUE.json.
 * Output: a SARIF 2.1.0 JSON object (deterministic, byte-identical on repeat).
 */
import { loadArtifact, loadRunArtifact, pathExists, PACKAGE_ROOT } from './artifacts.mjs';
import { loadPack } from './data.mjs';
import { DILIGENT_VERSION } from './cli.mjs';
import { resultFingerprint, extractRuleId } from './fingerprint.mjs';
import { join } from 'node:path';

const SARIF_SCHEMA_URL = 'https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json';
const RESEARCH_NOTE = 'This rule is at the research lifecycle: the finding is a lead for human review, not a compliance determination.';
const MAX_OBSERVATION = 1000;

/**
 * Load rule metadata (lifecycle, obligation, severity_default) from shipped packs.
 * Returns a Map of rule_id -> { lifecycle, obligation, severity_default }.
 */
async function loadRuleMeta() {
  const map = new Map();
  const packsDir = join(PACKAGE_ROOT, 'rules', 'packs');
  if (!(await pathExists(packsDir))) return map;
  const { readdir } = await import('node:fs/promises');
  const files = (await readdir(packsDir)).filter(f => f.endsWith('.json')).sort();
  for (const file of files) {
    try {
      const pack = await loadPack(PACKAGE_ROOT, file.replace('.json', ''));
      for (const rule of pack.json.rules || []) {
        if (rule.rule_id) {
          map.set(rule.rule_id, {
            lifecycle: rule.lifecycle || 'research',
            obligation: rule.obligation || rule.rule_id,
            severity_default: rule.severity_default || 'medium'
          });
        }
      }
    } catch { /* skip unloadable packs */ }
  }
  return map;
}

/**
 * Map a severity and lifecycle to a SARIF level.
 * research lifecycle -> "note" always.
 * Otherwise: critical/high -> "error", medium -> "warning", low -> "note".
 */
function sarifLevel(lifecycle, severity) {
  if (lifecycle === 'research') return 'note';
  if (severity === 'critical' || severity === 'high') return 'error';
  if (severity === 'medium') return 'warning';
  return 'note';
}

/**
 * Build one SARIF result from a finding.
 */
function buildFindingResult(finding, ruleIndexById, ruleMeta) {
  const meta = ruleMeta.get(finding.rule_id) || { lifecycle: 'research', obligation: finding.rule_id, severity_default: 'medium' };
  const messageText = (finding.observation || '').slice(0, MAX_OBSERVATION);
  const result = {
    ruleId: finding.rule_id,
    ruleIndex: ruleIndexById.get(finding.rule_id),
    level: sarifLevel(meta.lifecycle, finding.severity),
    message: { text: messageText },
    partialFingerprints: {
      'diligent/fingerprint/v1': finding.fingerprint
    },
    locations: [],
    properties: {
      finding_id: finding.finding_id,
      obligation_id: finding.obligation_id,
      review_state: finding.review_state,
      certainty_basis: finding.certainty_basis,
      evidence_refs: [...(finding.evidence_refs || [])],
      lifecycle: meta.lifecycle
    }
  };

  if (meta.lifecycle === 'research') {
    result.properties.diligent_note = RESEARCH_NOTE;
  }

  for (const loc of (finding.locations || [])) {
    const sarifLoc = {};
    if (loc.url) {
      sarifLoc.physicalLocation = {
        artifactLocation: { uri: loc.url }
      };
    }
    if (loc.selector) {
      sarifLoc.logicalLocations = [{
        fullyQualifiedName: loc.selector
      }];
    }
    if (sarifLoc.physicalLocation || sarifLoc.logicalLocations) {
      result.locations.push(sarifLoc);
    }
  }

  return result;
}

/**
 * Location for a static source-pass result: the scanned file, the start line
 * when the pass knew it, and the CSS path as a logical location. Returns null
 * for any other result.
 */
function sourceLocation(result) {
  const ext = result.extensions?.['diligent.source'];
  if (!ext || typeof ext.source_path !== 'string' || ext.source_path === '') return null;
  const physicalLocation = { artifactLocation: { uri: ext.source_path } };
  if (Number.isInteger(ext.line) && ext.line >= 1) {
    physicalLocation.region = { startLine: ext.line };
  }
  const location = { physicalLocation };
  if (typeof ext.instance === 'string' && ext.instance !== '') {
    location.logicalLocations = [{ fullyQualifiedName: ext.instance }];
  }
  return location;
}

/**
 * Build one SARIF result from a check result (result-derived entry).
 */
function buildResultEntry(result, ruleIndexById, ruleMeta) {
  const ruleId = extractRuleId(result);
  const meta = ruleMeta.get(ruleId) || { lifecycle: 'research', obligation: ruleId, severity_default: 'medium' };
  const fingerprint = resultFingerprint(result);
  const messageText = (result.observed?.reason || result.observed?.header || result.check_id).slice(0, MAX_OBSERVATION);

  const sarifResult = {
    ruleId,
    ruleIndex: ruleIndexById.get(ruleId),
    level: result.status === 'needs_review' ? 'note' : sarifLevel(meta.lifecycle, meta.severity_default),
    message: { text: messageText },
    partialFingerprints: {
      'diligent/result/v1': fingerprint
    },
    locations: [sourceLocation(result) || {
      physicalLocation: {
        artifactLocation: { uri: result.instance_id }
      }
    }],
    properties: {
      check_id: result.check_id,
      result_id: result.result_id,
      instance_id: result.instance_id,
      obligation_id: result.obligation_id,
      status: result.status,
      evidence_refs: [...(result.evidence_refs || [])],
      lifecycle: meta.lifecycle
    }
  };

  if (result.status === 'needs_review') {
    sarifResult.properties.needs_review = true;
  }

  if (meta.lifecycle === 'research') {
    sarifResult.properties.diligent_note = RESEARCH_NOTE;
  }

  return sarifResult;
}

/**
 * Build one SARIF rule descriptor from a rule_id.
 */
function buildRule(ruleId, meta, authorityRefs) {
  const text = meta?.obligation || ruleId;
  return {
    id: ruleId,
    shortDescription: { text },
    fullDescription: { text },
    properties: {
      lifecycle: meta?.lifecycle || 'research',
      authority_refs: authorityRefs.sort()
    }
  };
}

/**
 * Convert a run directory into a SARIF 2.1.0 document.
 * Returns { ok, sarif, errors }.
 */
export async function convertToSarif(runDir, { ruleMeta, baseline } = {}) {
  const errors = [];

  const runArtifact = await loadArtifact(runDir, 'RUN.json');
  if (!runArtifact.exists) return { ok: false, sarif: null, errors: ['RUN.json not found in run directory'] };
  if (!runArtifact.ok) return { ok: false, sarif: null, errors: runArtifact.errors };
  const run = runArtifact.data;

  const reviewArtifact = await loadRunArtifact(runDir, 'REVIEW-QUEUE.json');
  let reviewQueueCount = 0;
  if (reviewArtifact.exists && reviewArtifact.ok) {
    reviewQueueCount = (reviewArtifact.data.items || []).length;
  }

  const metaMap = ruleMeta || await loadRuleMeta();

  // Load results (primary input)
  const resultsArtifact = await loadRunArtifact(runDir, 'RESULTS.jsonl');
  const results = resultsArtifact.exists && resultsArtifact.ok ? (resultsArtifact.data || []) : [];

  // Load findings (optional secondary input)
  const findingsArtifact = await loadRunArtifact(runDir, 'FINDINGS.json');
  const findings = findingsArtifact.exists && findingsArtifact.ok ? (findingsArtifact.data.findings || []) : [];

  // Filter results for fail/needs_review
  const emittedResults = results.filter(r => r.status === 'fail' || r.status === 'needs_review');
  const resultsByStatus = {};
  for (const r of results) {
    resultsByStatus[r.status] = (resultsByStatus[r.status] || 0) + 1;
  }

  // Collect all rule_ids from both sources
  const allRuleIds = new Set();
  for (const r of emittedResults) {
    allRuleIds.add(extractRuleId(r));
  }
  for (const f of findings) {
    allRuleIds.add(f.rule_id);
  }

  const ruleIds = [...allRuleIds].sort();
  const ruleIndexById = new Map();
  const authorityRefsByRule = new Map();

  // Collect authority_refs from findings
  for (const f of findings) {
    if (!authorityRefsByRule.has(f.rule_id)) {
      authorityRefsByRule.set(f.rule_id, new Set());
    }
    for (const ref of f.authority_refs || []) {
      authorityRefsByRule.get(f.rule_id).add(ref);
    }
  }

  const rules = ruleIds.map((id, i) => {
    ruleIndexById.set(id, i);
    return buildRule(id, metaMap.get(id), [...(authorityRefsByRule.get(id) || [])]);
  });

  // Build baseline fingerprint set if baseline provided
  const baselineFps = baseline ? new Map(baseline.fingerprints.map(fp => [fp.fingerprint, fp])) : null;

  // Build results from both sources
  const sarifResults = [];

  // Result-derived entries
  for (const r of emittedResults) {
    const fp = resultFingerprint(r);
    // If baseline provided, check if this result is known, new, or resolved
    if (baselineFps) {
      if (baselineFps.has(fp)) {
        // Known result - mark as unchanged
        const sarifResult = buildResultEntry(r, ruleIndexById, metaMap);
        sarifResult.baselineState = 'unchanged';
        sarifResults.push(sarifResult);
      } else {
        // New result - mark as new
        const sarifResult = buildResultEntry(r, ruleIndexById, metaMap);
        sarifResult.baselineState = 'new';
        sarifResults.push(sarifResult);
      }
    } else {
      // No baseline - emit normally
      sarifResults.push(buildResultEntry(r, ruleIndexById, metaMap));
    }
  }

  // Finding-derived entries
  for (const f of findings) {
    if (baselineFps && baselineFps.has(f.fingerprint)) {
      // Known finding - mark as unchanged
      const sarifResult = buildFindingResult(f, ruleIndexById, metaMap);
      sarifResult.baselineState = 'unchanged';
      sarifResults.push(sarifResult);
    } else if (baselineFps) {
      // New finding - mark as new
      const sarifResult = buildFindingResult(f, ruleIndexById, metaMap);
      sarifResult.baselineState = 'new';
      sarifResults.push(sarifResult);
    } else {
      // No baseline - emit normally
      sarifResults.push(buildFindingResult(f, ruleIndexById, metaMap));
    }
  }

  // Sort by (ruleId, fingerprint, finding_id or result_id)
  sarifResults.sort((a, b) => {
    if (a.ruleId < b.ruleId) return -1;
    if (a.ruleId > b.ruleId) return 1;
    const fpA = a.partialFingerprints['diligent/fingerprint/v1'] || a.partialFingerprints['diligent/result/v1'];
    const fpB = b.partialFingerprints['diligent/fingerprint/v1'] || b.partialFingerprints['diligent/result/v1'];
    if (fpA < fpB) return -1;
    if (fpA > fpB) return 1;
    const idA = a.properties.finding_id || a.properties.result_id || '';
    const idB = b.properties.finding_id || b.properties.result_id || '';
    if (idA < idB) return -1;
    if (idA > idB) return 1;
    return 0;
  });

  const sarif = {
    version: '2.1.0',
    $schema: SARIF_SCHEMA_URL,
    runs: [{
      tool: {
        driver: {
          name: 'Diligent',
          semanticVersion: DILIGENT_VERSION,
          informationUri: 'https://diligent.style',
          rules
        }
      },
      results: sarifResults,
      invocations: [{
        executionSuccessful: run.assessment_complete === true,
        properties: {
          run_id: run.run_id,
          execution_status: run.execution_status,
          review_queue_count: reviewQueueCount,
          results_total: results.length,
          results_emitted: sarifResults.length,
          results_by_status: resultsByStatus
        }
      }]
    }]
  };

  return { ok: true, sarif, errors };
}

/**
 * Structural SARIF 2.1.0 validator. Checks the subset of SARIF constraints
 * the project's schema validator cannot enforce (uri-reference format).
 * Returns { ok, errors }.
 */
export function validateSarif(sarif) {
  const errors = [];
  if (!sarif || typeof sarif !== 'object') { errors.push('sarif must be an object'); return { ok: false, errors }; }
  if (sarif.version !== '2.1.0') errors.push('version must be "2.1.0"');
  if (typeof sarif.$schema !== 'string' || !sarif.$schema.includes('sarif-schema-2.1.0')) errors.push('$schema must reference sarif-schema-2.1.0');
  if (!Array.isArray(sarif.runs) || sarif.runs.length === 0) { errors.push('runs must be a non-empty array'); return { ok: false, errors }; }

  for (let ri = 0; ri < sarif.runs.length; ri++) {
    const run = sarif.runs[ri];
    const p = `runs[${ri}]`;
    if (!run.tool?.driver) { errors.push(`${p}.tool.driver is required`); continue; }
    const driver = run.tool.driver;
    if (typeof driver.name !== 'string') errors.push(`${p}.tool.driver.name must be a string`);
    if (!Array.isArray(driver.rules)) errors.push(`${p}.tool.driver.rules must be an array`);
    if (!Array.isArray(run.results)) errors.push(`${p}.results must be an array`);
    if (!Array.isArray(run.invocations) || run.invocations.length === 0) errors.push(`${p}.invocations must be a non-empty array`);

    if (Array.isArray(driver.rules)) {
      for (let i = 0; i < driver.rules.length; i++) {
        const rule = driver.rules[i];
        if (typeof rule.id !== 'string') errors.push(`${p}.tool.driver.rules[${i}].id must be a string`);
        if (!rule.shortDescription?.text) errors.push(`${p}.tool.driver.rules[${i}].shortDescription.text is required`);
      }
    }

    if (Array.isArray(run.results)) {
      for (let i = 0; i < run.results.length; i++) {
        const result = run.results[i];
        const rp = `${p}.results[${i}]`;
        if (typeof result.ruleId !== 'string') errors.push(`${rp}.ruleId must be a string`);
        if (typeof result.ruleIndex !== 'number') errors.push(`${rp}.ruleIndex must be a number`);
        if (!result.message?.text) errors.push(`${rp}.message.text is required`);
        if (!['error', 'warning', 'note', 'none'].includes(result.level)) errors.push(`${rp}.level must be error, warning, note, or none`);
        const fp = result.partialFingerprints?.['diligent/fingerprint/v1'] || result.partialFingerprints?.['diligent/result/v1'];
        if (!fp) errors.push(`${rp}.partialFingerprints must have diligent/fingerprint/v1 or diligent/result/v1`);
      }
    }

    if (Array.isArray(run.invocations)) {
      const inv = run.invocations[0];
      if (typeof inv?.executionSuccessful !== 'boolean') errors.push(`${p}.invocations[0].executionSuccessful must be a boolean`);
    }
  }

  return { ok: errors.length === 0, errors };
}

/**
 * Serialize a SARIF document to deterministic JSON (sorted keys, 2-space indent).
 */
export function serializeSarif(sarif) {
  return JSON.stringify(sarif, sortedKeys(), 2) + '\n';
}

function sortedKeys() {
  return (key, value) => {
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      return Object.keys(value).sort().reduce((sorted, k) => { sorted[k] = value[k]; return sorted; }, {});
    }
    return value;
  };
}
