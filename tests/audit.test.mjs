import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { access, cp, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { loadArtifact } from '../scripts/lib/artifacts.mjs';
import { ROOT, run, withTemp } from './helpers.mjs';

const FAKE = join(ROOT, 'tests', 'support', 'fake-engine', 'diligent-rs');
const FACTS = join(ROOT, 'references', 'facts', 'us-federal-public-web.example.json');
const STAGES = ['init', 'scope', 'applicability', 'inventory', 'plan', 'collect', 'assess', 'infra', 'merge', 'gate', 'report'];
const exists = async path => access(path).then(() => true, () => false);

/** Run `diligent audit` with the fake engine. `config` becomes FAKE_ENGINE_CONFIG. */
async function audit(tmp, { url = 'https://example.gov/', args = [], config = null, facts = true, env = {} } = {}) {
  const extra = { DILIGENT_RS_BIN: FAKE, ...env };
  if (config) { const path = join(tmp, 'fake-config.json'); await writeFile(path, JSON.stringify(config)); extra.FAKE_ENGINE_CONFIG = path; }
  const result = await run('bin/diligent.mjs', ['audit', '--url', url, 'demo', '--root', tmp, ...(facts ? ['--facts', FACTS] : []), '--pages', '3', '--json', ...args], { env: extra });
  let summary = null;
  try { summary = JSON.parse(result.stdout); } catch { /* not JSON for exit 2 and 3 usage cases */ }
  return { ...result, summary, dir: join(tmp, '.diligent', 'demo') };
}
const lines = async path => (await readFile(path, 'utf8')).split('\n').filter(Boolean).map(line => JSON.parse(line));

test('without the diligent-rs binary the command exits 3 with the install message and writes nothing', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { env: { DILIGENT_RS_BIN: '/nonexistent/diligent-rs' } });
    assert.equal(result.code, 3);
    assert.match(result.stderr, /diligent audit needs the diligent-rs companion binary \(protocol 1\.x\); see docs\/install\.md\. init, status, gate and report work without it\./);
    assert.equal(await exists(join(tmp, '.diligent')), false);
  });
});

test('the full chain runs against the fake engine, validates, and returns the documented --json shape', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp);
    assert.equal(result.code, 1, `${result.stdout}\n${result.stderr}`);
    const s = result.summary;
    assert.deepEqual(Object.keys(s), ['audit_id', 'run_id', 'directory', 'stages', 'gate_exit', 'report_path', 'corroboration']);
    assert.equal(s.audit_id, 'demo');
    assert.equal(s.gate_exit, 1);
    assert.equal(s.report_path, join(result.dir, 'REPORT.md'));
    assert.deepEqual(s.stages.map(stage => stage.name), STAGES);
    for (const stage of s.stages) {
      assert.deepEqual(Object.keys(stage), ['name', 'state', 'exit_code', 'duration_ms', 'warning', 'detail']);
      assert.notEqual(stage.state, 'failed', `${stage.name}: ${stage.warning}`);
    }
    for (const name of ['SCOPE.json', 'APPLICABILITY.json', 'INVENTORY.json', 'SAMPLE.json', 'AUDIT-PLAN.json', 'EVIDENCE.jsonl', 'RESULTS.jsonl', 'SIGNALS.jsonl', 'REVIEW-QUEUE.json', 'ASSESSMENT.json', 'EVENTS.jsonl']) {
      const loaded = await loadArtifact(result.dir, name);
      assert.ok(loaded.ok, `${name}: ${loaded.errors.join('; ')}`);
    }
    assert.equal(await exists(join(result.dir, 'EXCEPTIONS.json')), false, 'the orchestrator never writes exceptions');
    const run = (await loadArtifact(join(result.dir, 'runs', s.run_id), 'RUN.json')).data;
    assert.equal(run.pipeline_state, 'reported');
    assert.equal(run.execution_status, 'completed');
    assert.deepEqual(run.engine, { name: 'native-rust', executed_by: 'mixed', native_available: true, protocol_version: '1', binary_version: '0.2.0-fake' });
    assert.equal(run.tool_versions['diligent-rs'], '0.2.0-fake');
    assert.deepEqual(run.budgets, { pages: 3, timeout_ms: 30000 });
    assert.deepEqual(run.snapshot_refs, ['https://example.gov/', 'https://example.gov/about']);
    assert.equal(run.extensions.audit.pages.length, 2);
    const events = (await lines(join(result.dir, 'EVENTS.jsonl'))).map(e => e.event);
    for (const name of STAGES) assert.ok(events.includes(`stage_${name}_started`), name);
    assert.ok(events.includes('scope_permissions_recorded'), 'the network grant is a logged act');
    const text = await readFile(join(result.dir, 'REPORT.md'), 'utf8');
    assert.match(text, /## 10\. Craft and clarity/);
  });
});

test('facts are recorded as known operator facts, and the origin grant lands in SCOPE.json', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { args: ['--fact', 'collects_personal_data=true', '--fact', 'adopted_wcag_version=2.1'] });
    const scope = (await loadArtifact(result.dir, 'SCOPE.json')).data;
    const fact = id => scope.facts.find(f => f.fact_id === id);
    assert.equal(fact('public_facing').value, true);
    assert.equal(fact('public_facing').state, 'known');
    assert.equal(fact('public_facing').provided_by, 'operator');
    assert.ok(fact('public_facing').observed_at);
    assert.equal(fact('collects_personal_data').value, true);
    assert.equal(fact('adopted_wcag_version').value, 2.1, 'a --fact value is JSON when it parses, and a --fact overrides the same id in the facts file');
    assert.deepEqual(scope.permissions.allowed_origins, ['https://example.gov']);
    assert.equal(scope.processing_boundary.network_allowed, true);
    assert.deepEqual(fact('allowed_origins').value, ['https://example.gov']);
    assert.equal(fact('ada_title2_covered').value, false);
  });
});

test('an unknown fact id, a bad facts file, --pages 0 and an existing audit id are usage errors that write nothing', async () => {
  await withTemp(async tmp => {
    const bad = join(tmp, 'bad-facts.json');
    await writeFile(bad, JSON.stringify({ not_a_real_fact: true }));
    const unknown = await audit(tmp, { facts: false, args: ['--facts', bad] });
    assert.equal(unknown.code, 2);
    assert.match(unknown.stderr, /unknown fact id for profile us-federal-public-web: not_a_real_fact/);
    const array = join(tmp, 'array.json');
    await writeFile(array, '[1,2]');
    assert.equal((await audit(tmp, { facts: false, args: ['--facts', array] })).code, 2);
    assert.equal((await audit(tmp, { args: ['--fact', 'novalue'] })).code, 2);
    assert.equal((await audit(tmp, { args: ['--pages', '0'] })).code, 2);
    assert.equal((await audit(tmp, { args: ['--corroborate', 'other'] })).code, 2);
    assert.equal(await exists(join(tmp, '.diligent')), false, 'nothing is created before the input is valid');
    assert.equal((await audit(tmp)).code, 1);
    const again = await audit(tmp);
    assert.equal(again.code, 2, 'a second audit with the same id is refused');
    assert.match(again.stderr, /already exists/);
  });
});

test('the merger dedupes identical evidence and keeps every result', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { samePayload: true } });
    assert.equal(result.code, 1, result.stderr);
    const evidence = await lines(join(result.dir, 'EVIDENCE.jsonl'));
    assert.equal(new Set(evidence.map(e => e.evidence_id)).size, evidence.length, 'evidence ids are unique');
    assert.equal(evidence.length, 3, 'one shared page payload plus two infra rows');
    assert.equal((await lines(join(result.dir, 'RESULTS.jsonl'))).length, 7);
    assert.equal((await readdir(join(result.dir, 'captures'))).filter(n => !n.startsWith('manifest-')).length, 2, 'one page payload and one infra payload');
    for (const row of evidence) assert.ok(row.payload_ref.startsWith('captures/') || row.payload_ref.startsWith('dns:'), row.payload_ref);
  });
});

test('REVIEW-QUEUE.json holds one blocking item per needs_review result', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp);
    const results = await lines(join(result.dir, 'RESULTS.jsonl'));
    const review = results.filter(r => r.status === 'needs_review');
    const queue = (await loadArtifact(result.dir, 'REVIEW-QUEUE.json')).data;
    assert.equal(review.length, 2);
    assert.equal(queue.items.length, review.length);
    assert.deepEqual(queue.items.map(i => i.item_id).sort(), review.map(r => `rev-${r.result_id}`).sort());
    for (const item of queue.items) {
      assert.equal(item.status, 'open');
      assert.equal(item.kind, 'human');
      assert.match(item.task, /^result_review: /);
      assert.equal(item.owner_role, 'accessibility_lead', "from the rule's owner_role");
      assert.ok(item.evidence_refs.length > 0);
    }
  });
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { noReview: true } });
    assert.deepEqual((await loadArtifact(result.dir, 'REVIEW-QUEUE.json')).data.items, [], 'no needs_review result, no review item');
  });
});

test('a duplicate result id is a hard merge failure: nothing is merged and the gate does not run', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { dupResultId: true } });
    assert.equal(result.code, 3);
    assert.equal(result.summary.gate_exit, null);
    const merge = result.summary.stages.find(s => s.name === 'merge');
    assert.equal(merge.state, 'failed');
    assert.match(merge.warning, /result_id res-dup-1 appears in both page .* and page /);
    assert.equal(await exists(join(result.dir, 'RESULTS.jsonl')), false);
    assert.equal(await exists(join(result.dir, 'ASSESSMENT.json')), false);
    const run = (await loadArtifact(join(result.dir, 'runs', result.summary.run_id), 'RUN.json')).data;
    assert.equal(run.execution_status, 'failed');
  });
});

test('a foreign run id, a traversing payload_ref and a hash mismatch are each a hard merge failure', async () => {
  const cases = [
    [{ foreignRunId: true }, /carries run_id run-2020-01-01-aaaaaa, expected run-/],
    [{ traversal: true }, /is not captures\/<name> or evidence\/<name>/],
    [{ badHash: true }, /does not match the sha256/]
  ];
  for (const [config, pattern] of cases) {
    await withTemp(async tmp => {
      const result = await audit(tmp, { config });
      assert.equal(result.code, 3, JSON.stringify(config));
      assert.match(result.summary.stages.find(s => s.name === 'merge').warning, pattern);
      assert.equal(await exists(join(result.dir, 'EVIDENCE.jsonl')), false, 'no partial merge is written');
    });
  }
});

test('an http origin skips infra with a warning, and a sample with no pages is stated, not hidden', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { url: 'http://127.0.0.1:9/', args: ['--allow-private'] });
    assert.equal(result.code, 1, result.stderr);
    const infra = result.summary.stages.find(s => s.name === 'infra');
    assert.equal(infra.state, 'skipped');
    assert.match(infra.warning, /not https/);
    assert.ok(!(await lines(join(result.dir, 'RESULTS.jsonl'))).some(r => r.result_id === 'res-infra-1'));
  });
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { emptySample: true } });
    assert.equal(result.code, 2, 'no sampled pages leaves obligations untested, so the gate is incomplete');
    assert.match(result.summary.stages.find(s => s.name === 'plan').warning, /no collectable pages/);
    assert.equal((await lines(join(result.dir, 'RESULTS.jsonl'))).length, 1, 'only the infra result exists');
  });
});

test('a blocked or failing page is recorded and the audit continues', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { collectBlocked: ['/about'] } });
    assert.equal(result.code, 1);
    const collect = result.summary.stages.find(s => s.name === 'collect');
    assert.equal(collect.state, 'partial');
    assert.match(collect.warning, /1 of 2 pages collected; 1 blocked and 0 failed/);
    const run = (await loadArtifact(join(result.dir, 'runs', result.summary.run_id), 'RUN.json')).data;
    const pages = run.extensions.audit.pages;
    assert.deepEqual(pages.map(p => [p.collect, p.assess]), [['collected', 'assessed'], ['blocked', 'not_run']]);
    assert.equal(pages[1].error, 'blocked_origin');
  });
});

test('running the gate twice on an audit gives byte-identical assessments', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp);
    const now = JSON.parse(await readFile(join(result.dir, 'ASSESSMENT.json'), 'utf8')).computed_at;
    await run('scripts/gate.mjs', ['--dir', result.dir, '--now', now]);
    const first = await readFile(join(result.dir, 'ASSESSMENT.json'));
    await run('scripts/gate.mjs', ['--dir', result.dir, '--now', now]);
    assert.deepEqual(await readFile(join(result.dir, 'ASSESSMENT.json')), first);
  });
});

test('audit --help documents the command and bin/diligent.mjs lists it', async () => {
  const help = await run('bin/diligent.mjs', ['audit', '--help']);
  assert.equal(help.code, 0);
  assert.match(help.stdout, /Usage: diligent audit --url/);
  assert.match(help.stdout, /Exit codes:/);
  const top = await run('bin/diligent.mjs', ['--help']);
  assert.match(top.stdout, /audit --url/);
});

test('the profile decides axe corroboration, --corroborate forces it on and --no-corroborate forces it off', async () => {
  await withTemp(async tmp => {
    const dataRoot = join(tmp, 'data');
    await cp(join(ROOT, 'fixtures', 'package'), dataRoot, { recursive: true });
    const base = JSON.parse(await readFile(join(dataRoot, 'profiles', 'mini-profile.json'), 'utf8'));
    await writeFile(join(dataRoot, 'profiles', 'mini-axe.json'), JSON.stringify({ ...base, profile_id: 'mini-axe', corroboration: { browser: ['axe-core'] } }));
    const collectArgs = async (name, profile, args) => {
      const log = join(tmp, `${name}.log`);
      const root = join(tmp, name);
      const result = await audit(root, { facts: false, args: ['--profile', profile, '--data-root', dataRoot, ...args], env: { FAKE_ENGINE_LOG: log } });
      const calls = (await lines(log)).filter(call => call[0] === 'collect');
      assert.ok(calls.length > 0, `${name}: ${result.stderr}`);
      const assessFlags = (await lines(log)).filter(call => call[0] === 'assess').map(call => call.includes('--no-corroborate'));
      return { result, assessFlags, corroborate: calls.map(call => { const i = call.indexOf('--corroborate'); return i >= 0 ? call[i + 1] : null; }) };
    };
    await mkdir(join(tmp, 'on'));
    const fromProfile = await collectArgs('on', 'mini-axe', []);
    assert.ok(fromProfile.corroborate.every(v => v === 'axe'), 'the profile turns it on');
    assert.ok(fromProfile.assessFlags.length > 0 && fromProfile.assessFlags.every(v => v === false), 'assess is not told to skip corroboration');
    assert.deepEqual(fromProfile.result.summary.corroboration, { applied: true, source: 'profile' });
    const run = (await loadArtifact(join(fromProfile.result.dir, 'runs', fromProfile.result.summary.run_id), 'RUN.json')).data;
    assert.deepEqual(run.extensions.audit.corroboration, { applied: true, source: 'profile', profile_requested: true });

    await mkdir(join(tmp, 'off'));
    const off = await collectArgs('off', 'mini-axe', ['--no-corroborate']);
    assert.ok(off.corroborate.every(v => v === null), '--no-corroborate wins over the profile');
    assert.ok(off.assessFlags.length > 0 && off.assessFlags.every(v => v === true), 'every assess call gets --no-corroborate');
    assert.deepEqual(off.result.summary.corroboration, { applied: false, source: 'flag_off' });

    await mkdir(join(tmp, 'forced'));
    const forced = await collectArgs('forced', 'mini-profile', ['--corroborate', 'axe']);
    assert.ok(forced.corroborate.every(v => v === 'axe'));
    assert.ok(forced.assessFlags.every(v => v === false));
    assert.deepEqual(forced.result.summary.corroboration, { applied: true, source: 'flag' });

    await mkdir(join(tmp, 'plain'));
    const plain = await collectArgs('plain', 'mini-profile', []);
    assert.ok(plain.corroborate.every(v => v === null), 'a profile without corroboration stays off');
    assert.ok(plain.assessFlags.length > 0 && plain.assessFlags.every(v => v === true), 'off by default also reaches assess');
    assert.deepEqual(plain.result.summary.corroboration, { applied: false, source: 'none' });

    const both = await audit(join(tmp, 'both'), { args: ['--corroborate', 'axe', '--no-corroborate'] });
    assert.equal(both.code, 2);
    assert.match(both.stderr, /cannot be used together/);
  });
});

test('a collector failure leaves the error kind, message and the first stderr lines in the stage record', async () => {
  await withTemp(async tmp => {
    const result = await audit(tmp, { config: { collectFail: ['/about'], collectBlocked: ['/'] } });
    const collect = result.summary.stages.find(s => s.name === 'collect');
    assert.equal(collect.state, 'partial');
    assert.match(collect.detail, /https:\/\/example\.gov\/: exit 2, kind blocked_origin, message: blocked/);
    assert.match(collect.detail, /https:\/\/example\.gov\/about: exit 3\n {4}fake collector failure/);
    assert.ok(collect.detail.length <= 4096);
    assert.equal(result.summary.stages.find(s => s.name === 'init').detail, null);
  });
});

// --- replay mode -----------------------------------------------------------------------------

/** A fake recorded capture directory: one payload and a manifest the fake engine understands. */
async function fakeCapture(root, name, url) {
  const dir = join(root, 'captures-src', name);
  await mkdir(dir, { recursive: true });
  const payload = JSON.stringify({ fake: 'dom', url });
  const id = createHash('sha256').update(payload).digest('hex');
  await writeFile(join(dir, `${id}.json`), payload);
  await writeFile(join(dir, 'manifest.jsonl'), `${JSON.stringify({ evidence_id: id, kind: 'dom_snapshot', path: `${id}.json`, sha256: id, final_url: url })}\n`);
  return dir;
}

async function replayAudit(tmp, replay, { args = [], rawReplay = null, env = {} } = {}) {
  const file = join(tmp, 'replay.json');
  await writeFile(file, rawReplay ?? JSON.stringify(replay));
  const log = join(tmp, 'engine.log');
  const result = await run('bin/diligent.mjs', ['audit', '--replay', file, 'demo', '--root', tmp, '--facts', FACTS, '--json', ...args], { env: { DILIGENT_RS_BIN: FAKE, FAKE_ENGINE_LOG: log, ...env } });
  let summary = null;
  try { summary = JSON.parse(result.stdout); } catch { /* usage errors print no JSON */ }
  return { ...result, summary, file, log, dir: join(tmp, '.diligent', 'demo') };
}

test('replay mode audits recorded captures offline: discovery, collection and infra are skipped, the rest runs as in a live audit', async () => {
  await withTemp(async tmp => {
    const home = await fakeCapture(tmp, 'home', 'https://example.gov/');
    const about = await fakeCapture(tmp, 'about', 'https://example.gov/about');
    const result = await replayAudit(tmp, { origin: 'https://example.gov/', pages: [{ url: 'https://example.gov/', captures: home }, { url: 'https://example.gov/about', captures: about }] });
    assert.equal(result.code, 1, `${result.stdout}\n${result.stderr}`);
    const stage = name => result.summary.stages.find(s => s.name === name);
    assert.deepEqual(result.summary.stages.map(s => s.name), STAGES);
    assert.equal(stage('inventory').state, 'done');
    assert.match(stage('inventory').warning, /replay: no discovery performed/);
    assert.equal(stage('collect').state, 'skipped');
    assert.match(stage('collect').warning, /replay: captures supplied/);
    assert.equal(stage('infra').state, 'skipped');
    assert.equal(stage('infra').warning, 'replay: network stages skipped');
    for (const name of ['applicability', 'plan', 'assess', 'merge', 'gate', 'report']) assert.equal(stage(name).state, 'done', name);
    // INVENTORY.json and SAMPLE.json come from the replay list.
    const inventory = (await loadArtifact(result.dir, 'INVENTORY.json')).data;
    assert.deepEqual(inventory.routes, ['https://example.gov/', 'https://example.gov/about']);
    assert.deepEqual(inventory.discovery_limits, ['replay: no discovery performed']);
    const sample = (await loadArtifact(result.dir, 'SAMPLE.json')).data;
    assert.deepEqual(sample.structured.map(s => s.reason), ['replay', 'replay']);
    assert.deepEqual(sample.random, []);
    assert.equal(sample.coverage.discovered, 2);
    assert.equal(sample.coverage.sampled_structured, 2);
    // Mode and the replay file's hash are recorded.
    const record = (await loadArtifact(join(result.dir, 'runs', result.summary.run_id), 'RUN.json')).data;
    assert.equal(record.extensions.audit.mode, 'replay');
    assert.equal(record.extensions.audit.replay_sha256, createHash('sha256').update(await readFile(result.file)).digest('hex'));
    assert.deepEqual(record.snapshot_refs, ['https://example.gov/', 'https://example.gov/about']);
    // No network grant, and no network or browser subcommand ran.
    const scope = (await loadArtifact(result.dir, 'SCOPE.json')).data;
    assert.equal(scope.processing_boundary.network_allowed, false);
    const events = (await lines(join(result.dir, 'EVENTS.jsonl'))).map(e => e.event);
    assert.ok(events.includes('scope_replay_recorded'));
    assert.ok(!events.includes('scope_permissions_recorded'));
    const calls = (await lines(result.log)).map(call => call[0]);
    for (const skipped of ['inventory', 'collect', 'infra']) assert.ok(!calls.includes(skipped), `${skipped} must not run`);
    assert.ok(calls.includes('applicability') && calls.includes('assess'));
    // Payloads reach the shared payload root, results validate, and the gate ran.
    assert.ok((await readdir(join(result.dir, 'captures'))).some(name => name.endsWith('.json') && !name.startsWith('manifest-')));
    for (const name of ['RESULTS.jsonl', 'EVIDENCE.jsonl', 'SIGNALS.jsonl', 'REVIEW-QUEUE.json', 'AUDIT-PLAN.json', 'ASSESSMENT.json']) assert.ok((await loadArtifact(result.dir, name)).ok, name);
    assert.equal(result.summary.gate_exit, 1);
  });
});

test('a relative captures path is tried against the replay file directory first', async () => {
  await withTemp(async tmp => {
    await fakeCapture(tmp, 'home', 'https://example.gov/');
    const result = await replayAudit(tmp, { origin: 'https://example.gov', pages: [{ url: 'https://example.gov/', captures: 'captures-src/home' }] });
    assert.equal(result.code, 1, `${result.stdout}\n${result.stderr}`);
  });
});

test('a bad replay file is a usage error that writes nothing', async () => {
  const good = { url: 'https://example.gov/', captures: '/x' };
  const cases = [
    ['not json', null, '{nope'],
    ['an array', null, '[]'],
    ['no origin', { pages: [good] }, null],
    ['an ftp origin', { origin: 'ftp://example.gov/', pages: [good] }, null],
    ['no pages', { origin: 'https://example.gov/', pages: [] }, null],
    ['a page on another origin', { origin: 'https://example.gov/', pages: [{ url: 'https://evil.example/', captures: '/x' }] }, null],
    ['a duplicate url', { origin: 'https://example.gov/', pages: [good, good] }, null],
    ['a page without captures', { origin: 'https://example.gov/', pages: [{ url: 'https://example.gov/' }] }, null]
  ];
  for (const [name, doc, raw] of cases) {
    await withTemp(async tmp => {
      const result = await replayAudit(tmp, doc, { rawReplay: raw });
      assert.equal(result.code, 2, `${name}: ${result.stderr}`);
      assert.equal(await exists(join(tmp, '.diligent')), false, `${name}: nothing is created`);
    });
  }
  await withTemp(async tmp => {
    const missing = await run('bin/diligent.mjs', ['audit', '--replay', join(tmp, 'absent.json'), '--root', tmp], { env: { DILIGENT_RS_BIN: FAKE } });
    assert.equal(missing.code, 2);
    assert.match(missing.stderr, /file not found/);
    const both = await replayAudit(tmp, { origin: 'https://example.gov/', pages: [good] }, { args: ['--url', 'https://example.gov/'] });
    assert.equal(both.code, 2);
    assert.match(both.stderr, /--replay and --url cannot be used together/);
  });
});

test('a missing capture directory fails the collect stage by name with exit 3 and the gate does not run', async () => {
  await withTemp(async tmp => {
    const home = await fakeCapture(tmp, 'home', 'https://example.gov/');
    const result = await replayAudit(tmp, { origin: 'https://example.gov/', pages: [{ url: 'https://example.gov/', captures: home }, { url: 'https://example.gov/gone', captures: join(tmp, 'captures-src', 'gone') }] });
    assert.equal(result.code, 3);
    assert.equal(result.summary.gate_exit, null);
    const collect = result.summary.stages.find(s => s.name === 'collect');
    assert.equal(collect.state, 'failed');
    assert.match(collect.detail, /https:\/\/example\.gov\/gone: no manifest\.jsonl in .*gone/);
    assert.equal(await exists(join(result.dir, 'ASSESSMENT.json')), false);
    assert.ok(!result.summary.stages.some(s => s.name === 'assess'), 'later stages do not run');
  });
});

test('replay mode still needs the binary', async () => {
  await withTemp(async tmp => {
    const home = await fakeCapture(tmp, 'home', 'https://example.gov/');
    const result = await replayAudit(tmp, { origin: 'https://example.gov/', pages: [{ url: 'https://example.gov/', captures: home }] }, { env: { DILIGENT_RS_BIN: '/nonexistent/diligent-rs' } });
    assert.equal(result.code, 3);
    assert.match(result.stderr, /needs the diligent-rs companion binary/);
  });
});

test('replay: one capture directory may serve one page, and the replay url must match the recorded path and query', async () => {
  await withTemp(async tmp => {
    const about = await fakeCapture(tmp, 'about', 'https://example.gov/about?lang=en');
    const same = await replayAudit(tmp, { origin: 'https://example.gov/', pages: [{ url: 'https://example.gov/about?lang=en', captures: about }, { url: 'https://example.gov/other', captures: about }] });
    assert.equal(same.code, 2);
    assert.match(same.stderr, /https:\/\/example\.gov\/about\?lang=en and https:\/\/example\.gov\/other name the same capture directory/);
    assert.equal(await exists(join(tmp, '.diligent')), false);
  });
  await withTemp(async tmp => {
    const about = await fakeCapture(tmp, 'about', 'https://example.gov/about');
    const path = await replayAudit(tmp, { origin: 'https://example.gov/', pages: [{ url: 'https://example.gov/contact', captures: about }] });
    assert.equal(path.code, 2);
    assert.match(path.stderr, /https:\/\/example\.gov\/contact does not match the page recorded in .*about, which is https:\/\/example\.gov\/about\./);
    const query = await replayAudit(tmp, { origin: 'https://example.gov/', pages: [{ url: 'https://example.gov/about?x=1', captures: about }] });
    assert.equal(query.code, 2, 'a query difference is a mismatch too');
    assert.equal(await exists(join(tmp, '.diligent')), false);
  });
  await withTemp(async tmp => {
    // Scheme and host may differ: a capture recorded on a local server is replayed under the real origin.
    const local = await fakeCapture(tmp, 'local', 'http://127.0.0.1:41234/about?lang=en');
    const result = await replayAudit(tmp, { origin: 'https://example.gov/', pages: [{ url: 'https://example.gov/about?lang=en', captures: local }] });
    assert.equal(result.code, 1, `${result.stdout}\n${result.stderr}`);
  });
});
