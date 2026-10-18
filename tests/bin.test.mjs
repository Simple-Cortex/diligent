import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chmod, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { ROOT, VERSION, run, withTemp } from './helpers.mjs';

const bin = (args, opts) => run('bin/diligent.mjs', args, opts);

test('--version prints the package version; --json adds the engine envelope', async () => {
  const plain = await bin(['--version']);
  assert.equal(plain.code, 0);
  assert.equal(plain.stdout.trim(), VERSION);
  const json = JSON.parse((await bin(['--version', '--json'])).stdout);
  assert.equal(json.name, 'diligent');
  assert.equal(json.engine.name, 'node-fallback');
  assert.equal(json.engine.executed_by, 'node');
});

test('--help lists the implemented commands and the agent phases, and states zero automated checks', async () => {
  const result = await bin(['--help']);
  assert.equal(result.code, 0);
  for (const word of ['init', 'status', 'gate', 'report', 'rules validate', 'validate-skill', 'install', 'check', 'test']) assert.ok(result.stdout.includes(word), word);
  assert.match(result.stdout, /zero automated website checks/);
  assert.match(result.stdout, /discover, scope, inventory, plan, collect, assess, propose, verify, resume/);
});

for (const phase of ['discover', 'scope', 'inventory', 'plan', 'collect', 'assess', 'propose', 'verify']) {
  test(`${phase} is an agent-performed phase: exit 3 with the documented message`, async () => {
    const result = await bin([phase]);
    assert.equal(result.code, 3);
    assert.equal(result.stderr.trim(), `${phase} is an agent-performed phase in ${VERSION} and has no executable implementation. Read skills/${phase}/SKILL.md.`);
  });
}

test('resume is an agent phase too, and --json gives a machine-readable envelope', async () => {
  const result = await bin(['--json', 'resume']);
  assert.equal(result.code, 3);
  const out = JSON.parse(result.stdout);
  assert.equal(out.status, 'not_implemented');
  assert.equal(out.exit_code, 3);
});

test('an unknown command is a usage error', async () => {
  const result = await bin(['frobnicate']);
  assert.equal(result.code, 2);
  assert.match(result.stderr, /unknown command "frobnicate"/);
  assert.equal((await bin(['rules', 'frobnicate'])).code, 2);
});

test('utilities are dispatched with their exit codes preserved', async () => {
  assert.equal((await bin(['rules', 'validate', '--data-root', join(ROOT, 'fixtures', 'package')])).code, 0);
  assert.equal((await bin(['rules', 'validate', '--data-root', join(ROOT, 'fixtures', 'rules', 'dangling-authority')])).code, 3);
  const missing = await bin(['gate', 'ghost-audit', '--root', ROOT + '/fixtures']);
  assert.equal(missing.code, 3);
  assert.equal((await bin(['init'])).code, 2);
});

test('--json is forwarded to utilities', async () => {
  const result = await bin(['--json', 'rules', 'validate', '--data-root', join(ROOT, 'fixtures', 'package')]);
  assert.equal(result.code, 0);
  assert.equal(JSON.parse(result.stdout).ok, true);
});

test('a native diligent-rs reporting protocol 1 is detected but nothing is delegated', async () => {
  await withTemp(async tmp => {
    const fake = join(tmp, 'diligent-rs');
    await writeFile(fake, '#!/bin/sh\n[ "$1" = "protocol-version" ] && echo 1\n');
    await chmod(fake, 0o755);
    const json = JSON.parse((await bin(['--version', '--json'], { env: { DILIGENT_RS_BIN: fake } })).stdout);
    assert.equal(json.engine.name, 'native-rust');
    assert.equal(json.engine.native_available, true);
    assert.equal(json.engine.executed_by, 'node', 'Node still executes every command in this release');
    await writeFile(fake, '#!/bin/sh\necho 2\n');
    const mismatch = JSON.parse((await bin(['--version', '--json'], { env: { DILIGENT_RS_BIN: fake } })).stdout);
    assert.equal(mismatch.engine.name, 'node-fallback');
  });
});
