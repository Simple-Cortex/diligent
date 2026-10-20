/**
 * Storybook component mode for `diligent audit --storybook <url>`: discovery of stories from a built
 * Storybook, the iframe page of each story, and the INVENTORY.json and SAMPLE.json that stand in for
 * site discovery. Nothing here decides a result; a component audit is never a site claim.
 */
import { createHash } from 'node:crypto';
import { lookup as dnsLookup } from 'node:dns/promises';
import { isIP } from 'node:net';

export class StorybookError extends Error {}

const MAX_BYTES = 20 * 1024 * 1024;
const MAX_REDIRECTS = 3;
const STORY_ID = /^[A-Za-z0-9][A-Za-z0-9_.-]*$/;

/** The Storybook root: the given URL without query or fragment and with a trailing slash. */
export function storybookBase(input) {
  const url = new URL(input);
  url.search = '';
  url.hash = '';
  if (!url.pathname.endsWith('/')) url.pathname = `${url.pathname}/`;
  return url;
}

/** True for loopback, private, link-local, shared, multicast and unspecified addresses. */
export function isPrivateAddress(address) {
  const kind = isIP(address);
  if (kind === 4) {
    const [a, b] = address.split('.').map(Number);
    return a === 0 || a === 10 || a === 127 || a >= 224 || (a === 169 && b === 254) || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127);
  }
  if (kind === 6) {
    const lower = address.toLowerCase();
    const mapped = lower.match(/^::ffff:(\d+\.\d+\.\d+\.\d+)$/);
    if (mapped) return isPrivateAddress(mapped[1]);
    return lower === '::' || lower === '::1' || /^f[cd]/.test(lower) || /^fe[89ab]/.test(lower) || lower.startsWith('ff');
  }
  return true;
}

async function assertPublicHost(url, lookup) {
  const host = url.hostname.replace(/^\[|\]$/g, '');
  if (host.toLowerCase() === 'localhost' || host.toLowerCase().endsWith('.localhost')) throw new StorybookError(`${url.origin} is a local address. Pass --allow-private to audit a Storybook on a private network or this machine.`);
  let addresses;
  try { addresses = isIP(host) ? [{ address: host }] : await lookup(host, { all: true }); } catch (error) {
    throw new StorybookError(`cannot resolve ${host}: ${error.message}`);
  }
  const bad = addresses.find(entry => isPrivateAddress(entry.address));
  if (bad) throw new StorybookError(`${url.origin} resolves to a private address (${bad.address}). Pass --allow-private to audit a Storybook on a private network or this machine.`);
}

/** GET one same-origin JSON file. Redirects stay on the origin. Resolves to { status, text }. */
async function getText(url, { allowPrivate, timeoutMs, fetchImpl, lookup, maxBytes = MAX_BYTES }) {
  let current = url;
  for (let hop = 0; hop <= MAX_REDIRECTS; hop++) {
    if (current.origin !== url.origin) throw new StorybookError(`${url.href} redirected to ${current.origin}, which is outside the audited origin.`);
    if (!allowPrivate) await assertPublicHost(current, lookup);
    let response;
    try {
      response = await fetchImpl(current.href, { redirect: 'manual', signal: AbortSignal.timeout(timeoutMs), headers: { accept: 'application/json' } });
    } catch (error) {
      throw new StorybookError(`could not fetch ${current.href}: ${error.cause?.code ?? error.message}`);
    }
    if (response.status >= 300 && response.status < 400 && response.headers.get('location')) {
      current = new URL(response.headers.get('location'), current);
      continue;
    }
    if (response.status === 404 || response.status === 410) return { status: response.status, text: '' };
    if (!response.ok) throw new StorybookError(`${current.href} answered ${response.status}.`);
    const length = Number(response.headers.get('content-length'));
    if (Number.isFinite(length) && length > maxBytes) throw new StorybookError(`${current.href} is larger than ${maxBytes} bytes.`);
    let text;
    if (response.body) {
      const reader = response.body.getReader();
      const chunks = [];
      let total = 0;
      try {
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          total += value.byteLength;
          if (total > maxBytes) { await reader.cancel(); throw new StorybookError(`${current.href} is larger than ${maxBytes} bytes.`); }
          chunks.push(value);
        }
      } catch (error) {
        if (error instanceof StorybookError) throw error;
        await reader.cancel().catch(() => {});
        throw error;
      }
      const buf = new Uint8Array(total);
      let off = 0;
      for (const chunk of chunks) { buf.set(chunk, off); off += chunk.byteLength; }
      text = new TextDecoder().decode(buf);
    } else {
      text = await response.text();
      if (text.length > maxBytes) throw new StorybookError(`${current.href} is larger than ${maxBytes} bytes.`);
    }
    return { status: response.status, text };
  }
  throw new StorybookError(`${url.href} redirected more than ${MAX_REDIRECTS} times.`);
}

function asStory(raw, key) {
  const id = typeof raw?.id === 'string' ? raw.id : key;
  const title = raw?.title ?? raw?.kind;
  const name = raw?.name ?? raw?.story;
  if (typeof id !== 'string' || typeof title !== 'string' || typeof name !== 'string' || title === '' || name === '') return null;
  if (!STORY_ID.test(id)) throw new StorybookError(`story id ${JSON.stringify(id.slice(0, 80))} is not a plain Storybook id.`);
  return { id, title, name, importPath: typeof raw.importPath === 'string' ? raw.importPath : null };
}

/** Story entries of a parsed index.json (Storybook 7 and 8) or stories.json (Storybook 6.4 and later). */
export function parseStoryIndex(doc, file) {
  const notStorybook = () => new StorybookError(`${file} is not a Storybook index: expected an "entries" object (index.json) or a "stories" object (stories.json).`);
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) throw notStorybook();
  const table = doc.entries ?? doc.stories;
  if (table === null || typeof table !== 'object' || Array.isArray(table)) throw notStorybook();
  const stories = [];
  const seen = new Set();
  let recognised = 0;
  for (const [key, raw] of Object.entries(table)) {
    if (raw === null || typeof raw !== 'object') continue;
    // index.json marks each entry "story" or "docs"; stories.json marks docs pages with parameters.docsOnly.
    const isDocs = raw.type !== undefined ? raw.type !== 'story' : raw.parameters?.docsOnly === true;
    const story = asStory(raw, key);
    if (!story) continue;
    recognised++;
    if (isDocs || seen.has(story.id)) continue;
    seen.add(story.id);
    stories.push(story);
  }
  if (recognised === 0) throw notStorybook();
  if (stories.length === 0) throw new StorybookError(`${file} lists no story entries (only docs pages).`);
  return stories;
}

/**
 * Stories of the Storybook at `baseUrl`, from <base>/index.json, else <base>/stories.json.
 * Same origin only; a private address is refused unless `allowPrivate`. Resolves to
 * [{ id, title, name, importPath }].
 */
export async function discoverStories(baseUrl, { allowPrivate = false, timeoutMs = 30000, fetchImpl = globalThis.fetch, lookup = dnsLookup, maxBytes } = {}) {
  const base = storybookBase(baseUrl);
  const context = { allowPrivate, timeoutMs, fetchImpl, lookup, ...(maxBytes != null ? { maxBytes } : {}) };
  for (const file of ['index.json', 'stories.json']) {
    const { status, text } = await getText(new URL(file, base), context);
    if (status === 404 || status === 410) continue;
    let doc;
    try { doc = JSON.parse(text); } catch { throw new StorybookError(`${new URL(file, base).href} is not valid JSON.`); }
    return parseStoryIndex(doc, file);
  }
  throw new StorybookError(`${base.href} has no index.json or stories.json. Point --storybook at the root of a built Storybook (the folder storybook build writes).`);
}

/** The iframe page of each story. */
export function storyPages(baseUrl, stories) {
  const base = storybookBase(baseUrl);
  return stories.map(story => {
    const url = new URL('iframe.html', base);
    url.search = `id=${encodeURIComponent(story.id)}&viewMode=story`;
    return { ...story, url: url.href };
  });
}

// diligent-xorshift64star/1, the sampler's generator: state is the first 8 bytes of sha256(seed), big-endian.
const MASK = (1n << 64n) - 1n;
const MULTIPLIER = 0x2545F4914F6CDD1Dn;
export function rngFromSeed(seed) {
  let state = createHash('sha256').update(seed).digest().readBigUInt64BE(0);
  if (state === 0n) state = 0x9E3779B97F4A7C15n;
  const next = () => {
    let x = state;
    x ^= x >> 12n;
    x ^= (x << 25n) & MASK;
    x ^= x >> 27n;
    state = x;
    return (x * MULTIPLIER) & MASK;
  };
  const below = n => {
    if (n <= 1) return 0;
    const bound = BigInt(n);
    const zone = MASK - (MASK % bound);
    for (;;) { const x = next(); if (x < zone) return Number(x % bound); }
  };
  return { next, below };
}

const clusterOf = title => createHash('sha256').update(title).digest('hex').slice(0, 12);

/**
 * INVENTORY.json and SAMPLE.json for a Storybook, built the way replayInventory builds them: stories are
 * grouped by title (the component), the structured sample takes the first story of each component, the random
 * sample draws the rest with the seeded sampler, and the two together never exceed `pages`.
 * Resolves to { inventory, sample, selected, total, sampled } where `selected` are story pages in audit order.
 */
export function storybookInventory({ baseUrl, stories, runId, auditId, seed, pages }) {
  const all = storyPages(baseUrl, stories);
  const byTitle = new Map();
  for (const story of all) {
    if (!byTitle.has(story.title)) byTitle.set(story.title, []);
    byTitle.get(story.title).push(story);
  }
  const structured = [...byTitle.values()].map(group => group[0]).slice(0, pages);
  const chosen = new Set(structured.map(story => story.id));
  const pool = all.filter(story => !chosen.has(story.id)).sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const budget = Math.min(Math.max(pages - structured.length, 0), pool.length);
  const rng = rngFromSeed(seed);
  for (let i = 0; i < budget; i++) {
    const j = i + rng.below(pool.length - i);
    [pool[i], pool[j]] = [pool[j], pool[i]];
  }
  const random = pool.slice(0, budget);
  const selected = [...structured, ...random];
  const structuredClusters = new Set(structured.map(story => clusterOf(story.title)));
  const inStructured = random.filter(story => structuredClusters.has(clusterOf(story.title))).length;
  const total = all.length;
  const sampled = selected.length;
  const unsampledComponents = [...byTitle.keys()].filter(title => !selected.some(story => story.title === title));
  const namespace = { 'diligent.storybook': { mode: 'component', storybook_url: storybookBase(baseUrl).href, total_stories: total, sampled_stories: sampled, components: byTitle.size } };
  const inventory = {
    schema_version: '1.0.0', run_id: runId, audit_id: auditId, generated_by: 'diligent audit --storybook',
    routes: selected.map(story => story.url),
    templates: [...byTitle.entries()].map(([title, group]) => ({ template_id: clusterOf(title), name: title, stories: group.length })),
    components: [...byTitle.entries()].map(([title, group]) => ({ component_id: title, name: title, stories: group.map(story => story.id) })),
    documents: [], vendors: [], roles: [], states: [],
    discovery_limits: [
      `storybook: ${total} stories in ${byTitle.size} components listed from the story index; docs entries are skipped and play functions are not run`,
      `${sampled} of ${total} stories sampled`
    ],
    extensions: namespace
  };
  const sample = {
    schema_version: '1.0.0', run_id: runId, audit_id: auditId, method: 'wcag-em-2.0', algorithm: 'diligent-sampler/1', seed,
    prng: 'diligent-xorshift64star/1',
    structured: structured.map(story => ({ url: story.url, cluster_id: clusterOf(story.title), reason: `template:${clusterOf(story.title)}` })),
    random: random.map(story => ({ url: story.url, cluster_id: clusterOf(story.title), in_structured_cluster: structuredClusters.has(clusterOf(story.title)) })),
    processes: [],
    coverage: { discovered: total, fetched: 0, clustered: byTitle.size, sampled_structured: structured.length, sampled_random: random.length, excluded_by_robots: 0, unreachable: 0, off_scope: 0, documents: 0 },
    structured_coverage_estimate: random.length ? inStructured / random.length : null,
    statement: 'A sample never supports a whole-site conformance claim. Results apply to the sampled pages and processes.',
    partial: sampled < total,
    remaining_work: [
      ...(sampled < total ? [`${total - sampled} of ${total} stories were not sampled and are untested`] : []),
      ...(unsampledComponents.length ? [`${unsampledComponents.length} component${unsampledComponents.length === 1 ? ' has' : 's have'} no sampled story: ${unsampledComponents.slice(0, 10).join(', ')}${unsampledComponents.length > 10 ? ', and more' : ''}`] : [])
    ],
    extensions: namespace
  };
  return { inventory, sample, selected, all, total, sampled };
}
