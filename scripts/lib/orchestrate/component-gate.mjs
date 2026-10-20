/**
 * The component gate for `diligent audit --storybook`. It decides the exit code of a component audit from
 * APPLICABILITY.json, RESULTS.jsonl and SAMPLE.json, and writes COMPONENT-BASELINE.json. It never touches
 * ASSESSMENT.json, which the site gate still writes and which says what the site-level evidence amounts to.
 * A component audit is not a site claim.
 */
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { appendEvent, atomicWriteJson, loadArtifact, sha256File, validateData } from '../artifacts.mjs';

export const COMPONENT_REASON = 'component mode: whole-page obligation';
export const COMPONENT_CLAIM = 'Component audit of sampled stories. Whole-page obligations are not tested. Not a site claim.';
const EXIT = { pass: 0, fail: 1, incomplete: 2 };

/** The final_url on the first line of a capture manifest, or null. */
async function recordedUrl(captureDir) {
  try { return JSON.parse((await readFile(join(captureDir, 'manifest.jsonl'), 'utf8')).split('\n', 1)[0]).final_url ?? null; } catch { return null; }
}

/** Ids of the checks whose scope is "element", read from `diligent-rs checks list --json`. */
export function elementCheckIds(catalog) {
  const checks = catalog?.checks;
  if (!Array.isArray(checks) || checks.some(check => check.scope !== 'element' && check.scope !== 'document')) return null;
  return new Set(checks.filter(check => check.scope === 'element').map(check => check.id));
}

/**
 * Decide the component gate and write COMPONENT-BASELINE.json.
 * stories: every story of the Storybook [{ id, title, name, url }]; selected: the sampled ones, in audit order;
 * pageRecords: the collect and assess outcome per sampled story page; elementIds: Set of element-level check ids.
 * Resolves to { state, exit, stories, failed, path, errors? }.
 */
export async function runComponentGate({ dir, runId, auditId, storybookUrl, stories, selected, pageRecords, elementIds, now }) {
  const applicability = (await loadArtifact(dir, 'APPLICABILITY.json')).data;
  const results = (await loadArtifact(dir, 'RESULTS.jsonl')).data ?? [];
  const sample = (await loadArtifact(dir, 'SAMPLE.json')).data;
  const mandatory = new Set((applicability?.obligations || []).filter(o => o.state === 'applicable' && o.mandatory).map(o => o.obligation_id));
  const recordByUrl = new Map(pageRecords.map(rec => [rec.url, rec]));

  const perStory = [];
  const seenFinal = new Map();
  const evaluatedObligations = new Set();
  for (const story of selected) {
    const rec = recordByUrl.get(story.url);
    const finalUrl = (rec?.captureDir ? await recordedUrl(rec.captureDir) : null) ?? story.url;
    const rows = results.filter(row => typeof row.instance_id === 'string' && row.instance_id.startsWith(`${finalUrl}::`));
    const failing = rows.filter(row => row.status === 'fail' && mandatory.has(row.obligation_id) && elementIds.has(row.check_id));
    for (const row of rows) if (elementIds.has(row.check_id) && mandatory.has(row.obligation_id)) evaluatedObligations.add(row.obligation_id);
    if (seenFinal.has(finalUrl)) seenFinal.get(finalUrl).push(story.id); else seenFinal.set(finalUrl, [story.id]);
    perStory.push({
      id: story.id, title: story.title, name: story.name, url: story.url, final_url: finalUrl,
      collect: rec?.collect ?? 'not_run', assess: rec?.assess ?? 'not_run',
      failed: failing.length > 0, failed_checks: [...new Set(failing.map(row => row.check_id))].sort(),
      failed_result_ids: failing.map(row => row.result_id), result_ids: rows.map(row => row.result_id)
    });
  }
  const failed = perStory.filter(story => story.failed);
  const duplicates = [...seenFinal.entries()].filter(([, ids]) => ids.length > 1);
  const unassessed = perStory.filter(story => story.collect !== 'collected' || story.assess !== 'assessed');
  const problems = [];
  if (perStory.length === 0) problems.push('no story was sampled');
  if (unassessed.length) problems.push(`${unassessed.length} sampled stor${unassessed.length === 1 ? 'y was' : 'ies were'} not collected and assessed: ${unassessed.slice(0, 5).map(story => story.id).join(', ')}${unassessed.length > 5 ? ', and more' : ''}`);
  if (duplicates.length) problems.push(`${duplicates.length} final URL${duplicates.length === 1 ? ' is' : 's are'} shared by several stories (${duplicates[0][1].join(', ')}), so their results cannot be told apart`);
  if (!failed.length && evaluatedObligations.size === 0) problems.push('no applicable mandatory obligation was evaluated by an element-level check; supply facts so applicability is known');
  const state = failed.length ? 'fail' : problems.length ? 'incomplete' : 'pass';
  const notes = [];

  const groups = new Map();
  for (const story of stories) {
    if (!groups.has(story.title)) groups.set(story.title, []);
    groups.get(story.title).push(story);
  }
  const components = [...groups.entries()].map(([title, group]) => {
    const own = perStory.filter(story => story.title === title);
    return {
      component_id: title, name: title.split('/').at(-1).trim() || title, library: 'storybook', version: null,
      configurations: own.map(story => story.id),
      states_covered: own.filter(story => story.assess === 'assessed').map(story => story.name),
      result_refs: own.flatMap(story => story.result_ids)
    };
  });
  const baseline = {
    schema_version: '1.0.0', run_id: runId, audit_id: auditId, components,
    extensions: {
      'diligent.storybook': {
        mode: 'component', storybook_url: storybookUrl, claim: COMPONENT_CLAIM,
        total_stories: stories.length, sampled_stories: selected.length, seed: sample?.seed ?? null,
        whole_page_obligations: { state: 'not_tested', reason: COMPONENT_REASON },
        gate: { state, exit: EXIT[state], failed_stories: failed.length, problems, notes, element_obligations_evaluated: evaluatedObligations.size },
        stories: perStory.map(({ result_ids, ...story }) => ({ ...story, result_count: result_ids.length })),
        computed_at: now
      }
    }
  };
  const check = await validateData('component-baseline', baseline, 'COMPONENT-BASELINE.json');
  if (!check.ok) return { state: 'invalid', exit: 3, stories: perStory, failed: failed.length, path: null, errors: check.errors };
  const path = join(dir, 'COMPONENT-BASELINE.json');
  await atomicWriteJson(path, baseline);
  await appendEvent(dir, {
    run_id: runId, stage: 'assessed', event: `component_gate_${state}`, artifact: 'COMPONENT-BASELINE.json', sha256: await sha256File(path),
    detail: `${perStory.length} stories, ${failed.length} failed, exit ${EXIT[state]}. Not a site claim.`
  });
  return { state, exit: EXIT[state], stories: perStory, failed: failed.length, path, problems, notes };
}

export const componentGateLine = ({ state, stories, failed, exit }) => `Component gate: ${state} (${stories} stories, ${failed} failed, exit ${exit}). Not a site claim.`;
