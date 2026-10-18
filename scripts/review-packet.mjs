#!/usr/bin/env node
/**
 * diligent review-packet: a markdown packet for the owner to review automated
 * rules, and the promote step that records the review in the packs.
 */
import { execFile } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { PACKAGE_ROOT, UsageError, pathExists } from './lib/artifacts.mjs';
import { isMain, parseArgs, runMain } from './lib/cli.mjs';
import { listJsonFiles, readJsonFile } from './lib/data.mjs';

const HELP = `Usage:
  node scripts/review-packet.mjs generate --out <file> [--catalog <file.json>] [--data-root <dir>]
  node scripts/review-packet.mjs promote --rules <id,id,...> [--data-root <dir>]

generate writes one row per rule that any check informs, with its pack, lifecycle,
authority and clause, modality, baseline test, checks and channels, ACT ids,
fixture counts, precision and recall, and review status, then a checklist to tick.
The check catalog comes from "diligent-rs checks list --json" ($DILIGENT_RS_BIN or
diligent-rs on PATH). When the binary lacks that subcommand, pass --catalog <file.json>
with the same shape; the packet header says which was used.

Fixture counts come from fixtures/a11y/act/manifest.json, fixtures/a11y/authored/manifest.json
and fixtures/source/manifest.json under the data root. Precision and recall come from
docs/a11y-check-metrics.md. A missing input is named in the header and its cells say n/a.

promote sets lifecycle "research" to "draft" and source_review_status to "reviewed" for
the listed rule ids and changes nothing else. It refuses unknown ids, rules that are not
at lifecycle "research", and packs whose formatting it would not preserve. It writes
nothing unless every id is accepted. Run "npm run validate" afterwards.

Exit codes: 0 done, 2 usage error or refusal, 3 unexpected error.`;

const CHECK_SOURCES = [
  ['checks', 'browser'],
  ['source_checks', 'source'],
  ['infra_checks', 'network'],
];

function run(binary, args) {
  return new Promise((resolvePromise, reject) => {
    execFile(binary, args, { timeout: 30000, maxBuffer: 16 * 1024 * 1024 }, (error, stdout) => {
      if (error) reject(error); else resolvePromise(stdout);
    });
  });
}

/** Loads the catalog from a file or from the binary. Returns { catalog, origin }. */
export async function loadCatalog({ catalogPath, env = process.env }) {
  if (catalogPath) {
    const { json } = await readJsonFile(resolve(catalogPath));
    return { catalog: json, origin: `file ${catalogPath} (binary subcommand not used)` };
  }
  const binary = env.DILIGENT_RS_BIN || 'diligent-rs';
  try {
    const out = await run(binary, ['checks', 'list', '--json']);
    return { catalog: JSON.parse(out), origin: `${binary} checks list --json` };
  } catch (error) {
    throw new UsageError(`cannot read the check catalog from "${binary} checks list --json" (${error.code || error.message}). Build diligent-rs with the checks subcommand, set DILIGENT_RS_BIN, or pass --catalog <file.json>.`);
  }
}

/** Rule id to the checks that inform it: [{ id, channel, act }]. */
export function checksByRule(catalog) {
  const map = new Map();
  for (const [key, defaultChannel] of CHECK_SOURCES) {
    for (const check of Array.isArray(catalog?.[key]) ? catalog[key] : []) {
      // Infra checks name one rule in `rule`; the others list `informs`.
      const informed = Array.isArray(check.informs) ? check.informs : (check.rule ? [check.rule] : []);
      for (const rule of informed) {
        if (!map.has(rule)) map.set(rule, []);
        map.get(rule).push({ id: String(check.id), channel: String(check.channel || defaultChannel), act: [...(check.act_ids || check.act || [])].map(String) });
      }
    }
  }
  return map;
}

async function readOptionalJson(path, missing, label) {
  if (!(await pathExists(path))) { missing.push(label); return null; }
  try { return (await readJsonFile(path)).json; } catch (error) { missing.push(`${label} (${error.message})`); return null; }
}

/** Counts fixtures per check id from the three manifests. */
export async function fixtureCounts(dataRoot, missing) {
  const counts = new Map();
  const bump = (id) => { if (id) counts.set(id, (counts.get(id) || 0) + 1); };
  const act = await readOptionalJson(join(dataRoot, 'fixtures/a11y/act/manifest.json'), missing, 'fixtures/a11y/act/manifest.json');
  for (const c of act?.cases || []) if (c.skipped_reason == null) bump(c.check_id);
  const authored = await readOptionalJson(join(dataRoot, 'fixtures/a11y/authored/manifest.json'), missing, 'fixtures/a11y/authored/manifest.json');
  for (const c of authored?.cases || []) bump(c.check_id);
  const source = await readOptionalJson(join(dataRoot, 'fixtures/source/manifest.json'), missing, 'fixtures/source/manifest.json');
  for (const e of source?.entries || []) bump(e.check_id);
  return counts;
}

/** Precision and recall per check id from the metrics table. */
export async function metrics(dataRoot, missing) {
  const path = join(dataRoot, 'docs/a11y-check-metrics.md');
  const out = new Map();
  if (!(await pathExists(path))) { missing.push('docs/a11y-check-metrics.md'); return out; }
  for (const line of (await readFile(path, 'utf8')).split('\n')) {
    const cells = line.split('|').map(c => c.trim());
    const match = /^`([^`]+)`$/.exec(cells[1] || '');
    if (!match || cells.length < 6) continue;
    out.set(match[1], { cases: cells[2], precision: cells[3], recall: cells[4] });
  }
  return out;
}

async function loadRules(dataRoot) {
  const files = await listJsonFiles(join(dataRoot, 'rules', 'packs'));
  const rules = new Map();
  for (const file of files) {
    const { json } = await readJsonFile(file);
    for (const rule of Array.isArray(json.rules) ? json.rules : []) {
      rules.set(rule.rule_id, { pack: json.pack_id, rule });
    }
  }
  return rules;
}

const cell = (text) => String(text ?? '').replace(/\|/g, '\\|').replace(/\r?\n/g, ' ');
const code = (text) => '`' + String(text).replace(/`/g, "'") + '`';

/** Builds the packet text. Pure given its inputs, so the test can call it directly. */
export function buildPacket({ rules, registry, byRule, counts, metricRows, origin, missing, generated }) {
  const titles = new Map((registry?.authorities || []).map(a => [a.authority_id, a.title]));
  const informed = [...byRule.keys()].sort();
  const lines = [];
  lines.push('# Automated rule review packet', '');
  lines.push(`Generated ${generated}. Catalog: ${origin}.`, '');
  if (missing.length) lines.push(`Missing inputs, shown as n/a: ${missing.join('; ')}.`, '');
  lines.push('Tick a rule only after reading its authority text. Then run `node scripts/review-packet.mjs promote --rules <ids>` and `npm run validate`.', '');
  lines.push('| Rule | Pack | Lifecycle | Authority | Clause | Modality | Baseline | Checks (channel) | ACT ids | Fixtures | Precision | Recall | Review status |');
  lines.push('| --- | --- | --- | --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | --- |');
  const unknown = [];
  for (const id of informed) {
    const entry = rules.get(id);
    const checks = byRule.get(id);
    if (!entry) { unknown.push(id); continue; }
    const r = entry.rule;
    const a = r.authority || {};
    const authority = a.authority_id ? `${a.authority_id}${titles.get(a.authority_id) ? ` ${titles.get(a.authority_id)}` : ''}` : 'none';
    const act = [...new Set(checks.flatMap(c => c.act))].sort();
    const fixtures = checks.reduce((n, c) => n + (counts.get(c.id) || 0), 0);
    const stat = (key) => {
      const values = checks.map(c => metricRows.get(c.id)?.[key]).filter(v => v && v !== 'n/a');
      return values.length ? values.join(' / ') : 'n/a';
    };
    lines.push(`| ${[
      code(id), entry.pack, r.lifecycle, cell(authority), cell(a.section || ''), a.modality || '', r.baseline_test_id || '',
      cell(checks.map(c => `${code(c.id)} (${c.channel})`).join(', ')), act.length ? cell(act.map(code).join(', ')) : 'none',
      String(fixtures), stat('precision'), stat('recall'), r.source_review_status,
    ].join(' | ')} |`);
  }
  lines.push('');
  if (unknown.length) lines.push(`Checks inform rule ids that no pack defines: ${unknown.map(code).join(', ')}.`, '');
  lines.push('## Checklist', '');
  for (const id of informed.filter(i => rules.has(i))) {
    const r = rules.get(id).rule;
    lines.push(`- [ ] ${code(id)}: authority text read; modality \`${r.authority?.modality || 'none'}\` is right; checks and fixtures match the obligation; blind spots stated (${(r.blind_spots || []).length}); promote.`);
  }
  lines.push('');
  return lines.join('\n');
}

export async function generate({ out, catalogPath, dataRoot, env = process.env, now = new Date() }) {
  const missing = [];
  const { catalog, origin } = await loadCatalog({ catalogPath, env });
  const rules = await loadRules(dataRoot);
  if (rules.size === 0) throw new UsageError(`no rule packs found under ${join(dataRoot, 'rules', 'packs')}.`);
  const registry = await readOptionalJson(join(dataRoot, 'authorities', 'registry.json'), missing, 'authorities/registry.json');
  const text = buildPacket({
    rules, registry, byRule: checksByRule(catalog), counts: await fixtureCounts(dataRoot, missing),
    metricRows: await metrics(dataRoot, missing), origin, missing, generated: now.toISOString().slice(0, 10),
  });
  await writeFile(out, text);
  return text;
}

/** Sets lifecycle and source_review_status for `ids`. Writes nothing unless every id is accepted. */
export async function promote({ ids, dataRoot }) {
  const files = await listJsonFiles(join(dataRoot, 'rules', 'packs'));
  const packs = [];
  for (const file of files) {
    const bytes = await readFile(file, 'utf8');
    packs.push({ file, bytes, json: JSON.parse(bytes) });
  }
  const found = new Map();
  for (const pack of packs) for (const rule of pack.json.rules || []) found.set(rule.rule_id, { pack, rule });
  const problems = [];
  for (const id of ids) {
    const hit = found.get(id);
    if (!hit) problems.push(`unknown rule id ${id}`);
    else if (hit.rule.lifecycle !== 'research') problems.push(`${id} is at lifecycle "${hit.rule.lifecycle}", not "research"`);
  }
  const touched = new Set(ids.filter(id => found.has(id)).map(id => found.get(id).pack));
  for (const pack of touched) {
    if (JSON.stringify(pack.json, null, 2) + '\n' !== pack.bytes) problems.push(`${pack.file} is not formatted as 2-space JSON with a final newline; promote would change more than two fields`);
  }
  if (problems.length) throw new UsageError(`refusing to promote: ${problems.join('; ')}.`);
  for (const id of ids) {
    const { rule } = found.get(id);
    rule.lifecycle = 'draft';
    rule.source_review_status = 'reviewed';
  }
  for (const pack of touched) await writeFile(pack.file, JSON.stringify(pack.json, null, 2) + '\n');
  return { promoted: ids, files: [...touched].map(p => p.file) };
}

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), {
    out: 'string', catalog: 'string', rules: 'string', 'data-root': 'string',
  });
  if (options.help) { console.log(HELP); return 0; }
  const [command, ...extra] = positional;
  if (extra.length) throw new UsageError(`unexpected argument ${extra[0]}.`);
  const dataRoot = resolve(options['data-root'] || PACKAGE_ROOT);
  if (command === 'generate') {
    if (!options.out) throw new UsageError('generate needs --out <file>.');
    await generate({ out: resolve(options.out), catalogPath: options.catalog, dataRoot });
    console.log(`review-packet: wrote ${options.out}`);
    return 0;
  }
  if (command === 'promote') {
    const ids = (options.rules || '').split(',').map(s => s.trim()).filter(Boolean);
    if (ids.length === 0) throw new UsageError('promote needs --rules <id,id,...>.');
    const result = await promote({ ids: [...new Set(ids)], dataRoot });
    console.log(`review-packet: promoted ${result.promoted.join(', ')} to lifecycle draft, source_review_status reviewed.`);
    console.log('review-packet: now run `npm run validate`.');
    return 0;
  }
  throw new UsageError('first argument must be generate or promote. Run with --help.');
}

if (isMain(import.meta.url)) runMain(main);
