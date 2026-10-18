/**
 * Deterministic assessment aggregation (blueprint 10.3 and 24.1).
 * Pure functions: no I/O, no clocks (the caller passes `now`).
 */

import { DILIGENT_VERSION } from './cli.mjs';

export const CLAIM_BOUNDARY =
  'This is an engineering assessment for the declared scope only. It is not a government certification, ' +
  'not a legal conclusion, and not a claim about the whole site. ' + `Diligent ${DILIGENT_VERSION} runs no automated website checks.`;

const OPEN_RESULT_STATES = new Set(['not_tested', 'blocked', 'error', 'needs_review']);
const EXECUTED_STATES = new Set(['pass', 'fail', 'needs_review']);

function byKindThenRef(a, b) {
  if (a.kind !== b.kind) return a.kind < b.kind ? -1 : 1;
  return a.ref < b.ref ? -1 : a.ref > b.ref ? 1 : 0;
}

/** Drop results that a later result explicitly supersedes. */
export function activeResults(results) {
  const superseded = new Set(results.map(result => result.supersedes_result_id).filter(Boolean));
  return results.filter(result => !superseded.has(result.result_id));
}

function hasCitedExclusion(obligation) {
  const exclusion = obligation.exclusion;
  return Boolean(exclusion && typeof exclusion.citation === 'string' && exclusion.citation.trim() &&
    Array.isArray(exclusion.predicate_facts) && exclusion.predicate_facts.length > 0);
}

/**
 * @param {object[]} applicability obligation records (APPLICABILITY.json .obligations)
 * @param {object[]} results check results (RESULTS.jsonl records)
 * @param {object[]} exceptions exception records (EXCEPTIONS.json .exceptions)
 * @param {object[]} reviewQueue review items (REVIEW-QUEUE.json .items)
 * @param {object} coverage { unsupported_obligation_ids?: string[], stale_result_ids?: string[], required_instances?: number|null, now?: string }
 */
export function aggregate(applicability, results, exceptions, reviewQueue, coverage = {}) {
  const unresolved = [];
  const failures = [];
  const advisory = [];
  const add = (kind, ref, reason) => unresolved.push({ kind, ref, reason });

  const byId = new Map();
  for (const obligation of applicability) {
    if (byId.has(obligation.obligation_id)) add('duplicate_obligation', obligation.obligation_id, 'The obligation appears more than once in the applicability record.');
    else byId.set(obligation.obligation_id, obligation);
  }
  const selected = byId.size;

  let applicable = 0;
  let notApplicable = 0;
  let pending = 0;
  let unresolvedApplicability = 0;
  const applicableIds = new Set();
  for (const obligation of byId.values()) {
    switch (obligation.state) {
      case 'applicable':
        applicable++;
        applicableIds.add(obligation.obligation_id);
        break;
      case 'not_applicable':
        if (hasCitedExclusion(obligation)) notApplicable++;
        else {
          unresolvedApplicability++;
          add('uncited_exclusion', obligation.obligation_id, 'Marked not applicable without a cited authority and predicate facts. A guess is not a waiver.');
        }
        break;
      case 'pending_effective_date':
        pending++;
        add('pending_effective_date', obligation.obligation_id, 'The obligation is not yet effective. It stays pending even if the site already meets its technical target.');
        break;
      default:
        unresolvedApplicability++;
        add('unknown_applicability', obligation.obligation_id, 'Applicability is unknown. Required facts are missing or disputed.');
    }
  }

  const active = activeResults(results);
  const staleIds = new Set(coverage.stale_result_ids || []);
  const resultsByObligation = new Map();
  for (const result of active) {
    if (!byId.has(result.obligation_id)) {
      add('orphan_result', result.result_id, `The result references obligation ${result.obligation_id}, which is not in the applicability record.`);
      continue;
    }
    if (!resultsByObligation.has(result.obligation_id)) resultsByObligation.set(result.obligation_id, []);
    resultsByObligation.get(result.obligation_id).push(result);
  }

  const instanceKeys = new Set();
  const executedKeys = new Set();
  for (const id of [...applicableIds].sort()) {
    const obligation = byId.get(id);
    const own = resultsByObligation.get(id) || [];
    if (own.length === 0) add('no_result', id, 'No check result exists for this applicable obligation. It is untested.');
    for (const result of own) {
      const key = `${result.obligation_id}|${result.instance_id}`;
      instanceKeys.add(key);
      if (EXECUTED_STATES.has(result.status)) executedKeys.add(key);
      if (result.status === 'fail') {
        if (obligation.mandatory) failures.push({ result_id: result.result_id, obligation_id: id, instance_id: result.instance_id, check_id: result.check_id });
        else advisory.push(result.result_id);
      } else if (OPEN_RESULT_STATES.has(result.status)) {
        add(`result_${result.status}`, result.result_id, `The check result for ${id} is ${result.status}. A result that is not pass or fail is unresolved.`);
      }
      if (result.stale === true || staleIds.has(result.result_id)) {
        add('stale_evidence', result.result_id, `The evidence behind this result for ${id} is stale.`);
      }
    }
  }

  const unsupportedIds = [...new Set(coverage.unsupported_obligation_ids || [])].filter(id => applicableIds.has(id)).sort();
  for (const id of unsupportedIds) add('unsupported_obligation', id, 'No supported channel can test this applicable obligation in this release.');

  let reviewed = 0;
  for (const item of reviewQueue) {
    if (item.status === 'resolved') reviewed++;
    else add('open_review_item', item.item_id, `Review item is ${item.status}: ${item.blocking_reason}`);
  }

  if (selected === 0) add('nothing_selected', 'applicability', 'No obligations are selected, so nothing was assessed.');

  const today = (coverage.now || '').slice(0, 10);
  const exceptionRecords = exceptions.map(exception => {
    let status = 'active';
    if (exception.expires_on && today && exception.expires_on < today) status = 'expired';
    else if (!exception.approval_evidence_refs || exception.approval_evidence_refs.length === 0) status = 'unverified';
    return { exception_id: exception.exception_id, obligation_id: exception.obligation_id, status, changes_result: false };
  }).sort((a, b) => (a.exception_id < b.exception_id ? -1 : 1));

  unresolved.sort(byKindThenRef);
  failures.sort((a, b) => (a.result_id < b.result_id ? -1 : 1));

  let overall;
  if (failures.length > 0) overall = 'fail';
  else if (unresolved.length > 0) overall = 'incomplete';
  else if (notApplicable === selected) overall = 'not_applicable';
  else overall = 'pass_for_declared_scope';

  const requiredInstances = Number.isInteger(coverage.required_instances) ? coverage.required_instances : instanceKeys.size;
  return {
    overall,
    has_incomplete_work: unresolved.length > 0,
    counts: {
      selected,
      applicable,
      not_applicable: notApplicable,
      unresolved_applicability: unresolvedApplicability,
      pending,
      required_instances: requiredInstances,
      executed: executedKeys.size,
      reviewed,
      unsupported: unsupportedIds.length
    },
    denominators: {
      applicable: selected,
      not_applicable: selected,
      unresolved_applicability: selected,
      pending: selected,
      executed: requiredInstances,
      reviewed: reviewQueue.length,
      unsupported: applicable
    },
    unresolved,
    failures,
    exceptions: exceptionRecords,
    advisory: advisory.sort()
  };
}

/** Process exit code for a gate outcome (blueprint 20.3). */
export function exitCodeFor(overall) {
  switch (overall) {
    case 'pass_for_declared_scope':
    case 'not_applicable': return 0;
    case 'fail': return 1;
    case 'incomplete': return 2;
    default: return 3;
  }
}
