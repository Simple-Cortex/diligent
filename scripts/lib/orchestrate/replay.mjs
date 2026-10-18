/**
 * Offline replay for `diligent audit --replay`: a list of pages with already recorded capture
 * directories. Nothing here touches the network or a browser. The replay file is operator input,
 * so it is validated before anything is written.
 */
import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { dirname, isAbsolute, resolve } from 'node:path';
import { PACKAGE_ROOT, UsageError } from '../artifacts.mjs';
import { pageSlug } from './engine.mjs';

const MAX_PAGES = 200;

/** The final_url on the first line of a capture's manifest.jsonl, or null when it cannot be read. */
async function recordedUrl(captureDir) {
  try {
    const first = (await readFile(resolve(captureDir, 'manifest.jsonl'), 'utf8')).split('\n', 1)[0];
    return new URL(JSON.parse(first).final_url);
  } catch { return null; }
}

/**
 * Reads and validates a replay file: { origin, pages: [{ url, captures }] }.
 * A relative `captures` path is tried against the replay file's directory, the package root, then the
 * current directory; the first that exists wins. A path that exists nowhere is kept (resolved against the
 * replay file) so the collect stage can fail by name instead of the whole command refusing to start.
 */
export async function loadReplay(file, { cwd = process.cwd() } = {}) {
  let bytes;
  try { bytes = await readFile(file); } catch (error) {
    throw new UsageError(`--replay ${file}: ${error.code === 'ENOENT' ? 'file not found' : error.message}.`);
  }
  let doc;
  try { doc = JSON.parse(bytes.toString('utf8')); } catch (error) { throw new UsageError(`--replay ${file}: not valid JSON (${error.message}).`); }
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) throw new UsageError(`--replay ${file} must hold a JSON object with "origin" and "pages".`);
  let origin;
  try { origin = new URL(doc.origin); } catch { throw new UsageError(`--replay ${file}: "origin" must be an absolute http or https URL.`); }
  if ((origin.protocol !== 'https:' && origin.protocol !== 'http:') || origin.username || origin.password) throw new UsageError(`--replay ${file}: "origin" must be an http or https URL without credentials.`);
  if (!Array.isArray(doc.pages) || doc.pages.length < 1 || doc.pages.length > MAX_PAGES) throw new UsageError(`--replay ${file}: "pages" must list 1 to ${MAX_PAGES} pages.`);
  const seen = new Set();
  const pages = doc.pages.map((entry, i) => {
    if (entry === null || typeof entry !== 'object' || typeof entry.url !== 'string' || typeof entry.captures !== 'string' || entry.captures === '') {
      throw new UsageError(`--replay ${file}: pages[${i}] needs a "url" and a "captures" directory.`);
    }
    let url;
    try { url = new URL(entry.url); } catch { throw new UsageError(`--replay ${file}: pages[${i}].url is not a valid URL.`); }
    if (url.origin !== origin.origin) throw new UsageError(`--replay ${file}: pages[${i}].url is not on ${origin.origin}.`);
    if (seen.has(url.href)) throw new UsageError(`--replay ${file}: ${url.href} is listed twice.`);
    seen.add(url.href);
    const candidates = isAbsolute(entry.captures)
      ? [entry.captures]
      : [resolve(dirname(file), entry.captures), resolve(PACKAGE_ROOT, entry.captures), resolve(cwd, entry.captures)];
    return { url: url.href, slug: pageSlug(url.href), captures: candidates.find(path => existsSync(path)) ?? candidates[0] };
  });
  // Result and instance ids come from the capture's own final_url, so a capture may serve one page only,
  // and the replay url must name the same path and query as the page that was recorded.
  const owner = new Map();
  for (const page of pages) {
    if (owner.has(page.captures)) throw new UsageError(`--replay ${file}: ${owner.get(page.captures)} and ${page.url} name the same capture directory (${page.captures}). A capture directory may appear once.`);
    owner.set(page.captures, page.url);
    const recorded = await recordedUrl(page.captures);
    if (recorded) {
      const named = new URL(page.url);
      if (`${named.pathname}${named.search}` !== `${recorded.pathname}${recorded.search}`) {
        throw new UsageError(`--replay ${file}: ${page.url} does not match the page recorded in ${page.captures}, which is ${recorded.href}. Scheme and host may differ; the path and query must match.`);
      }
    }
  }
  return { origin: origin.origin, pages, sha256: createHash('sha256').update(bytes).digest('hex') };
}

/** INVENTORY.json and SAMPLE.json built from the replay list. No discovery is performed. */
export function replayInventory({ replay, runId, auditId }) {
  const urls = replay.pages.map(page => page.url);
  const inventory = {
    schema_version: '1.0.0', run_id: runId, audit_id: auditId, generated_by: 'diligent audit --replay',
    routes: urls, templates: [], components: [], documents: [], vendors: [], roles: [], states: [],
    discovery_limits: ['replay: no discovery performed']
  };
  const sample = {
    schema_version: '1.0.0', run_id: runId, audit_id: auditId, method: 'wcag-em-2.0', algorithm: 'diligent-sampler/1', seed: 'replay',
    prng: 'diligent-xorshift64star/1',
    structured: urls.map(url => ({ url, cluster_id: createHash('sha256').update(url).digest('hex').slice(0, 12), reason: 'replay' })),
    random: [], processes: [],
    coverage: { discovered: urls.length, fetched: 0, clustered: urls.length, sampled_structured: urls.length, sampled_random: 0, excluded_by_robots: 0, unreachable: 0, off_scope: 0, documents: 0 },
    structured_coverage_estimate: null,
    statement: 'A sample never supports a whole-site conformance claim. Results apply to the sampled pages and processes.',
    partial: true, remaining_work: ['replay: pages outside the replay list were not discovered and are untested']
  };
  return { inventory, sample };
}
