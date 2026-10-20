import { test } from 'node:test';
import assert from 'node:assert/strict';
import { access, readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { loadArtifact, sha256 } from '../scripts/lib/artifacts.mjs';
import { ROOT, run, withTemp } from './helpers.mjs';

const DATA = join(ROOT, 'fixtures', 'package');
const init = (workspace, args = []) => run('scripts/init.mjs', ['demo', '--root', workspace, '--data-root', DATA, '--profile', 'mini-profile', ...args]);

async function currentRun(auditDir) {
  const [runId] = await readdir(join(auditDir, 'runs'));
  return { runId, dir: join(auditDir, 'runs', runId) };
}

test('init creates every promised artifact and each one validates against its schema', async () => {
  await withTemp(async workspace => {
    const result = await init(workspace);
    assert.equal(result.code, 0, result.stderr);
    const audit = join(workspace, '.diligent', 'demo');
    const { runId, dir: runDir } = await currentRun(audit);
    assert.match(runId, /^run-\d{4}-\d{2}-\d{2}-[0-9a-f]{6}$/);

    const expected = [['SCOPE.json', audit], ['SERVICE-BRIEF.json', audit], ['PROJECT-CONTEXT.json', audit], ['AUTHORITIES.lock.json', audit], ['RULES.lock.json', audit], ['EVENTS.jsonl', audit], ['RUN.json', runDir]];
    for (const [name, dir] of expected) {
      const loaded = await loadArtifact(dir, name);
      assert.equal(loaded.exists, true, `${name} should exist`);
      assert.equal(loaded.ok, true, `${name}: ${loaded.errors.join('; ')}`);
    }
    const project = (await loadArtifact(audit, 'PROJECT-CONTEXT.json')).data;
    assert.equal(project.discovery_status, 'pending');
    const runRecord = (await loadArtifact(runDir, 'RUN.json')).data;
    assert.equal(runRecord.execution_status, 'initialized');
    assert.equal(runRecord.pipeline_state, 'initialized');
    assert.equal(runRecord.engine.name, 'node-fallback');
    assert.deepEqual(runRecord.modes, ['source']);
    assert.equal(runRecord.assessment_complete, false);
    assert.match(runRecord.warnings.join(' '), /zero automated website checks/);
    assert.match(result.stdout, /Next step: run discovery; see skills\/discover/);
    assert.match(result.stdout, /scope facts are unresolved/);
  });
});

test('every scope fact starts unknown with a question, owner role, affected obligations and effect', async () => {
  await withTemp(async workspace => {
    assert.equal((await init(workspace)).code, 0);
    const scope = (await loadArtifact(join(workspace, '.diligent', 'demo'), 'SCOPE.json')).data;
    assert.ok(scope.facts.length >= 40);
    const groups = new Set(scope.facts.map(f => f.group));
    assert.equal(groups.size, 11, 'all eleven section-2 field groups are represented');
    for (const fact of scope.facts) {
      assert.equal(fact.state, 'unknown', fact.fact_id);
      assert.equal(fact.value, null, fact.fact_id);
      assert.ok(fact.question && fact.owner_role && fact.effect_on_assessment, fact.fact_id);
      assert.ok(fact.affected_obligations.length > 0, fact.fact_id);
    }
    assert.equal(new Set(scope.facts.map(f => f.fact_id)).size, scope.facts.length, 'fact ids are unique');
    assert.ok(scope.permissions.prohibited_actions.includes('live_submission'));
    const json = await run('scripts/init.mjs', ['demo2', '--root', workspace, '--data-root', DATA, '--profile', 'mini-profile', '--json']);
    assert.equal(JSON.parse(json.stdout).unresolved_facts, scope.facts.length);
  });
});

test('the locks are frozen copies with hashes and a supported-check matrix derived from channel support', async () => {
  await withTemp(async workspace => {
    assert.equal((await init(workspace)).code, 0);
    const audit = join(workspace, '.diligent', 'demo');
    const authorities = (await loadArtifact(audit, 'AUTHORITIES.lock.json')).data;
    const registryBytes = await readFile(join(DATA, 'authorities', 'registry.json'));
    assert.equal(authorities.registry_sha256, sha256(registryBytes));
    assert.equal(authorities.authorities.length, 2);
    assert.equal(authorities.source_mode, 'shipped_snapshot');
    const rules = (await loadArtifact(audit, 'RULES.lock.json')).data;
    assert.equal(rules.profile.sha256, sha256(await readFile(join(DATA, 'profiles', 'mini-profile.json'))));
    assert.deepEqual(rules.packs.map(p => p.pack_id).sort(), ['mini-core', 'mini-ux']);
    assert.equal(rules.supported_check_matrix.length, 4);
    const fed = rules.supported_check_matrix.find(row => row.rule_id === 'FED-01');
    assert.equal(fed.unsupported, true);
    assert.equal(rules.supported_check_matrix.find(row => row.rule_id === 'A11Y-1.1.1').unsupported, false);
    assert.ok(Object.keys(rules.schema_hashes).length === 34);
    assert.equal(rules.packs.find(p => p.pack_id === 'mini-ux').normative, false);
  });
});

test('EVENTS.jsonl starts with the initialization event and records artifact hashes', async () => {
  await withTemp(async workspace => {
    assert.equal((await init(workspace)).code, 0);
    const audit = join(workspace, '.diligent', 'demo');
    const events = (await loadArtifact(audit, 'EVENTS.jsonl')).data;
    assert.equal(events[0].event, 'audit_initialized');
    assert.equal(events[0].stage, 'initialized');
    const { runId, dir } = await currentRun(audit);
    assert.equal(events[0].run_id, runId);
    assert.equal(events[0].sha256, sha256(await readFile(join(dir, 'RUN.json'))));
    assert.ok(events.length >= 6);
  });
});

test('init refuses to overwrite an existing audit without --force', async () => {
  await withTemp(async workspace => {
    assert.equal((await init(workspace)).code, 0);
    const audit = join(workspace, '.diligent', 'demo');
    await writeFile(join(audit, 'SERVICE-BRIEF.json'), '{"edited":"by a person"}');
    const refused = await init(workspace);
    assert.equal(refused.code, 2);
    assert.match(refused.stderr, /already exists\. Use --force/);
    assert.equal(await readFile(join(audit, 'SERVICE-BRIEF.json'), 'utf8'), '{"edited":"by a person"}', 'a refused init must not touch the files');
  });
});

test('init --force replaces artifacts, starts a new run, and keeps run history and the event log', async () => {
  await withTemp(async workspace => {
    assert.equal((await init(workspace)).code, 0);
    const audit = join(workspace, '.diligent', 'demo');
    await writeFile(join(audit, 'RESULTS.jsonl'), 'stale\n');
    const eventsBefore = (await loadArtifact(audit, 'EVENTS.jsonl')).data.length;
    assert.equal((await init(workspace, ['--force'])).code, 0);
    await assert.rejects(() => access(join(audit, 'RESULTS.jsonl')), 'stale results from the previous run are removed');
    assert.equal((await readdir(join(audit, 'runs'))).length, 2);
    assert.ok((await loadArtifact(audit, 'EVENTS.jsonl')).data.length > eventsBefore);
    assert.equal((await loadArtifact(audit, 'SERVICE-BRIEF.json')).ok, true);
  });
});

test('init validates its arguments', async () => {
  await withTemp(async workspace => {
    const badMode = await init(workspace, ['--mode', 'telepathy']);
    assert.equal(badMode.code, 2);
    assert.match(badMode.stderr, /unknown mode "telepathy"/);
    const badId = await run('scripts/init.mjs', ['Bad_ID', '--root', workspace, '--data-root', DATA, '--profile', 'mini-profile']);
    assert.equal(badId.code, 2);
    const noId = await run('scripts/init.mjs', ['--root', workspace]);
    assert.equal(noId.code, 2);
    const badProfile = await run('scripts/init.mjs', ['demo', '--root', workspace, '--data-root', DATA, '--profile', 'nope']);
    assert.equal(badProfile.code, 2);
    assert.match(badProfile.stderr, /profile "nope" does not exist/);
    await assert.rejects(() => access(join(workspace, '.diligent')), 'a failed init creates nothing');
  });
});

test('init accepts a comma list of modes and records them', async () => {
  await withTemp(async workspace => {
    assert.equal((await init(workspace, ['--mode', 'source,documents'])).code, 0);
    const { dir } = await currentRun(join(workspace, '.diligent', 'demo'));
    assert.deepEqual((await loadArtifact(dir, 'RUN.json')).data.modes, ['source', 'documents']);
  });
});

test('init refuses to start from invalid shipped data and writes nothing', async () => {
  await withTemp(async workspace => {
    const result = await run('scripts/init.mjs', ['demo', '--root', workspace, '--data-root', join(ROOT, 'fixtures', 'rules', 'bad-schema'), '--profile', 'mini-profile']);
    assert.equal(result.code, 3);
    assert.match(result.stderr, /failed validation/);
    await assert.rejects(() => access(join(workspace, '.diligent')));
  });
});

test('init then status reports the initialized pipeline state', async () => {
  await withTemp(async workspace => {
    assert.equal((await init(workspace)).code, 0);
    const status = await run('scripts/status.mjs', ['demo', '--root', workspace, '--json']);
    assert.equal(status.code, 0, status.stderr);
    const out = JSON.parse(status.stdout);
    assert.equal(out.pipeline_state, 'initialized');
    assert.equal(out.run_record, 'valid');
    assert.equal(out.artifacts.find(a => a.name === 'SCOPE.json').state, 'valid');
    assert.equal(out.artifacts.find(a => a.name === 'RESULTS.jsonl').state, 'missing');
    const missing = await run('scripts/status.mjs', ['ghost', '--root', workspace]);
    assert.equal(missing.code, 3);
  });
});

test('status flags an invalid artifact instead of calling it valid', async () => {
  await withTemp(async workspace => {
    assert.equal((await init(workspace)).code, 0);
    await writeFile(join(workspace, '.diligent', 'demo', 'SCOPE.json'), '{"schema_version":"1.0.0"}');
    const out = JSON.parse((await run('scripts/status.mjs', ['demo', '--root', workspace, '--json'])).stdout);
    const scope = out.artifacts.find(a => a.name === 'SCOPE.json');
    assert.equal(scope.state, 'invalid');
    assert.ok(scope.error_count > 0);
  });
});

test('the real shipped data initializes an audit (runs once the shipped data validates)', async () => {
  await withTemp(async workspace => {
    const result = await run('scripts/init.mjs', ['demo', '--root', workspace]);
    assert.equal(result.code, 0, `${result.stdout}\n${result.stderr}`);
    const audit = join(workspace, '.diligent', 'demo');
    const rules = (await loadArtifact(audit, 'RULES.lock.json')).data;
    assert.equal(rules.profile.profile_id, 'us-federal-public-web');
    assert.ok(rules.packs.length >= 7);
    assert.ok(rules.supported_check_matrix.length > 100);
    assert.ok((await loadArtifact(audit, 'AUTHORITIES.lock.json')).data.authorities.length > 20);
  });
});
