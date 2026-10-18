import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chmod, cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { loadArtifact } from '../scripts/lib/artifacts.mjs';
import { ROOT, run, withTemp } from './helpers.mjs';

const FAKE = join(ROOT, 'tests', 'support', 'fake-engine', 'diligent-rs');
const FACTS = join(ROOT, 'references', 'facts', 'us-federal-public-web.example.json');
const exists = async path => { try { await readFile(path); return true; } catch { return false; }; };

async function audit(tmp, { url = 'https://example.gov/', args = [], config = null, facts = true, env = {}, fakeEngine = FAKE } = {}) {
  const extra = { DILIGENT_RS_BIN: fakeEngine, ...env };
  if (config) { const path = join(tmp, 'fake-config.json'); await writeFile(path, JSON.stringify(config)); extra.FAKE_ENGINE_CONFIG = path; }
  const result = await run('bin/diligent.mjs', ['audit', '--url', url, 'demo', '--root', tmp, ...(facts ? ['--facts', FACTS] : []), '--pages', '3', '--json', ...args], { env: extra });
  let summary = null;
  try { summary = JSON.parse(result.stdout); } catch { /* not JSON for exit 2/3 */ }
  return { ...result, summary, dir: join(tmp, '.diligent', 'demo') };
}

/** Copy the fake engine to a temp dir and patch its assess command to emit a result citing a phantom evidence id. */
async function phantomEvidenceEngine(tmp) {
  const dir = join(tmp, 'fake-phantom');
  await mkdir(dir, { recursive: true });
  const src = await readFile(FAKE, 'utf8');
  const patched = src.replace(
    "jsonl(join(out, 'RESULTS.jsonl'), results);",
    "results[0].evidence_refs = ['ev-phantom-nobody-recorded'];\n  jsonl(join(out, 'RESULTS.jsonl'), results);"
  );
  const target = join(dir, 'diligent-rs');
  await writeFile(target, patched);
  await chmod(target, 0o755);
  return target;
}

/** Copy the fake engine and add a foreignSignal config option that only affects SIGNALS run_id. */
async function foreignSignalEngine(tmp) {
  const dir = join(tmp, 'fake-foreign-signal');
  await mkdir(dir, { recursive: true });
  const src = await readFile(FAKE, 'utf8');
  const patched = src.replace(
    "run_id: config.foreignRunId ? 'run-2020-01-01-aaaaaa' : runId, extensions: {}",
    "run_id: (config.foreignSignalRunId ? 'run-2020-01-01-aaaaaa' : (config.foreignRunId ? 'run-2020-01-01-aaaaaa' : runId)), extensions: {}"
  );
  const target = join(dir, 'diligent-rs');
  await writeFile(target, patched);
  await chmod(target, 0o755);
  return target;
}

test('adv-merge: a RESULTS row citing an evidence id nobody recorded fails the merge with exit 3', async () => {
  await withTemp(async tmp => {
    const engine = await phantomEvidenceEngine(tmp);
    const result = await audit(tmp, { fakeEngine: engine });
    assert.equal(result.code, 3);
    const merge = result.summary.stages.find(s => s.name === 'merge');
    assert.equal(merge.state, 'failed');
    assert.match(merge.warning, /ev-phantom-nobody-recorded/);
    assert.equal(await exists(join(result.dir, 'RESULTS.jsonl')), false, 'nothing is merged on a phantom reference');
    assert.equal(await exists(join(result.dir, 'ASSESSMENT.json')), false, 'the gate does not run');
  });
});

test('adv-merge: two pages whose assess outputs share a result_id exit 3', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { dupResultId: true } });
    assert.equal(result.code, 3);
    const merge = result.summary.stages.find(s => s.name === 'merge');
    assert.equal(merge.state, 'failed');
    assert.match(merge.warning, /result_id res-dup-1 appears in both/);
    assert.equal(result.summary.gate_exit, null);
  });
});

test('adv-merge: an EVIDENCE row with payload_ref ../../etc/passwd is rejected and not copied', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { traversal: true } });
    assert.equal(result.code, 3);
    const merge = result.summary.stages.find(s => s.name === 'merge');
    assert.equal(merge.state, 'failed');
    assert.match(merge.warning, /is not captures\/<name> or evidence\/<name>/);
    assert.equal(await exists(join(result.dir, 'EVIDENCE.jsonl')), false);
  });
});

test('adv-merge: an EVIDENCE row whose payload hash does not match the file is rejected', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { badHash: true } });
    assert.equal(result.code, 3);
    const merge = result.summary.stages.find(s => s.name === 'merge');
    assert.equal(merge.state, 'failed');
    assert.match(merge.warning, /does not match the sha256/);
  });
});

test('adv-merge: a foreign run_id in a SIGNALS row is rejected by the merger', async () => {
  await withTemp(async tmp => {
    const engine = await foreignSignalEngine(tmp);
    const result = await audit(tmp, { fakeEngine: engine, config: { foreignSignalRunId: true } });
    assert.equal(result.code, 3, `${result.stdout}\n${result.stderr}`);
    const merge = result.summary.stages.find(s => s.name === 'merge');
    assert.equal(merge.state, 'failed');
    assert.match(merge.warning, /signal .* carries run_id run-2020-01-01-aaaaaa, expected run-/);
  });
});

test('adv-sample: a sample with zero pages completes the chain but the gate is incomplete', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { emptySample: true } });
    assert.equal(result.code, 2, 'no sampled pages leaves obligations untested');
    assert.equal(result.summary.gate_exit, 2);
    assert.match(result.summary.stages.find(s => s.name === 'plan').warning, /no collectable pages/);
  });
});

test('adv-usage: --pages 0 and --pages 201 are usage errors that exit 2', async () => {
  await withTemp(async tmp => {
    const zero = await audit(tmp, { args: ['--pages', '0'] });
    assert.equal(zero.code, 2);
    assert.match(zero.stderr, /--pages must be an integer from 1 to 200/);
    assert.equal(await exists(join(tmp, '.diligent')), false, 'nothing is created before the input is valid');
  });
  await withTemp(async tmp => {
    const over = await audit(tmp, { args: ['--pages', '201'] });
    assert.equal(over.code, 2);
    assert.match(over.stderr, /--pages must be an integer from 1 to 200/);
  });
});

test('adv-usage: --fact unknown_id=1 exits 2 before anything is written', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { facts: false, args: ['--fact', 'unknown_id=1'] });
    assert.equal(result.code, 2);
    assert.match(result.stderr, /unknown fact id/);
    assert.equal(await exists(join(tmp, '.diligent')), false);
  });
});

test('adv-usage: a --facts file that is not JSON exits 2', async () => {
  await withTemp(async tmp => {
    const bad = join(tmp, 'bad-facts.json');
    await writeFile(bad, 'not json at all');
    const result = await audit(tmp, { facts: false, args: ['--facts', bad] });
    assert.equal(result.code, 2);
  });
});

test('adv-idempotency: the orchestrator run twice into the same root refuses the second run', async () => {
  await withTemp(async tmp => {
    const first = await audit(tmp);
    assert.equal(first.code, 1);
    const second = await audit(tmp);
    assert.equal(second.code, 2, 'a second audit with the same id is refused');
    assert.match(second.stderr, /already exists/);
  });
});

test('adv-engine: DILIGENT_RS_BIN pointing at a script that prints protocol-version 2 is refused with exit 3', async () => {
  await withTemp(async tmp => {
    const dir = join(tmp, 'fake-v2');
    await mkdir(dir, { recursive: true });
    const script = join(dir, 'diligent-rs');
    await writeFile(script, '#!/usr/bin/env node\nif (process.argv[2] === "protocol-version") { console.log("2"); process.exit(0); }\nconsole.error("unknown"); process.exit(1);\n');
    await chmod(script, 0o755);
    const result = await audit(tmp, { fakeEngine: script });
    assert.equal(result.code, 3);
    assert.match(result.stderr, /diligent audit needs the diligent-rs companion binary \(protocol 1\.x\)/);
    assert.equal(await exists(join(tmp, '.diligent')), false, 'nothing is created when the binary is wrong');
  });
});

test('adv-collect: the fake engine exiting 3 on collect for one page records the failure, continues, and the gate still runs', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { collectFail: ['/about'] } });
    const collect = result.summary.stages.find(s => s.name === 'collect');
    assert.equal(collect.state, 'partial');
    assert.match(collect.warning, /1 of 2 pages collected; 0 blocked and 1 failed/);
    assert.ok(result.summary.gate_exit !== null, 'the gate still runs');
    const runData = (await loadArtifact(join(result.dir, 'runs', result.summary.run_id), 'RUN.json')).data;
    const pages = runData.extensions.audit.pages;
    assert.equal(pages[1].collect, 'failed');
    assert.equal(pages[1].assess, 'not_run');
  });
});
