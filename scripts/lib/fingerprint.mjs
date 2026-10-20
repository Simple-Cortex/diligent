/**
 * Fingerprint computation for result-derived SARIF entries.
 *
 * For automated results (from RESULTS.jsonl), the fingerprint is:
 *   sha256(check_id + "\0" + instance_id + "\0" + instance + "\0" + rule_id) as lowercase hex,
 * where instance is the element selector carried under an engine extension, or empty,
 * and rule_id is extractRuleId(row)
 *
 * This is exposed as partialFingerprints["diligent/result/v1"] in SARIF output.
 * Finding-derived entries keep their existing fingerprint as "diligent/fingerprint/v1".
 */
import { createHash } from 'node:crypto';

/**
 * The element instance of a result row: the first string-valued `instance` under any engine
 * extension key (`diligent.assess`, `diligent.source`, ...), else the empty string. Infra rows have none.
 * @param {object} result - one RESULTS.jsonl row
 * @returns {string}
 */
export function extensionInstance(result) {
  for (const ext of Object.values(result.extensions || {})) {
    if (typeof ext?.instance === 'string') return ext.instance;
  }
  return '';
}

/**
 * Compute a result fingerprint: sha256(check_id NUL instance_id NUL instance NUL rule_id), lowercase hex.
 * One check can emit a row per rule on the same element, so the rule id (see extractRuleId) is part of it.
 * @param {object} result - one RESULTS.jsonl row
 * @returns {string} SHA-256 hex digest
 */
export function resultFingerprint(result) {
  return createHash('sha256')
    .update(`${result.check_id}\0${result.instance_id}\0${extensionInstance(result)}\0${extractRuleId(result)}`)
    .digest('hex');
}

/**
 * Rule id for a check result row.
 * Priority: the first `rule_ids[0]` found under any engine extension key
 * (`diligent.infra`, `diligent.source`, `diligent.assess`, ...), then
 * `obligation_id` minus the `OBL-` prefix, then `check_id` for
 * `OBL-UNMAPPED-*` rows.
 * @param {object} result - one RESULTS.jsonl row
 * @returns {string}
 */
export function extractRuleId(result) {
  for (const ext of Object.values(result.extensions || {})) {
    if (Array.isArray(ext?.rule_ids) && ext.rule_ids[0]) return ext.rule_ids[0];
  }
  if (result.obligation_id?.startsWith('OBL-')) {
    const ruleId = result.obligation_id.slice(4);
    if (!ruleId.startsWith('UNMAPPED-')) return ruleId;
  }
  return result.check_id;
}
