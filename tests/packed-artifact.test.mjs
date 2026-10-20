/** Verifies the publishable tarball, never the source tree, installs and runs. */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { readdir, readFile, rm, stat } from 'node:fs/promises';
import { promisify } from 'node:util';
import { join } from 'node:path';
import { includedFiles, loadManifest } from '../scripts/lib/manifest.mjs';
import { ROOT, tempDir } from './helpers.mjs';

const exec = promisify(execFile);
const node = (args, cwd, env = {}) => exec(process.execPath, args, { cwd, env: { ...process.env, DILIGENT_RS_BIN: '/nonexistent/diligent-rs', ...env } });

test('the packed tarball equals the manifest, installs, and runs from inside', async () => {
  const temp = await tempDir('diligent-pack-');
  try {
    const { stdout } = await exec('npm', ['pack', ROOT, '--json', '--ignore-scripts'], { cwd: temp, env: { ...process.env, npm_config_cache: join(temp, 'cache') } });
    const pack = JSON.parse(stdout)[0];
    const packed = pack.files.map(file => file.path).sort();
    const expected = await includedFiles(ROOT, await loadManifest(ROOT));
    assert.deepEqual(packed, expected, 'tarball contents must equal the runtime-manifest include set');
    for (const forbidden of ['docs/research', 'docs/plans', 'tests/', 'fixtures/', 'evals/', 'crates/', 'Cargo', 'DEVLOG', 'tasks/', '.github/']) {
      assert.ok(!packed.some(path => path.startsWith(forbidden)), `tarball contains ${forbidden}`);
    }

    await exec('tar', ['-xzf', join(temp, pack.filename)], { cwd: temp });
    const pkg = join(temp, 'package');
    for (const required of ['SKILL.md', 'bin/diligent.mjs', 'runtime-manifest.json', 'scripts/install.mjs', 'schemas/assessment.schema.json', 'authorities/registry.json']) await stat(join(pkg, required));

    const version = await node(['bin/diligent.mjs', '--version'], pkg);
    assert.equal(version.stdout.trim(), pack.version);

    // Run the rules validator from inside the tarball: it must find the shipped data and schemas.
    const rules = await node(['bin/diligent.mjs', 'rules', 'validate'], pkg).catch(error => error);
    assert.equal(rules.code ?? 0, 0, `rules validate failed inside the tarball:\n${rules.stdout ?? ''}${rules.stderr ?? ''}`);
    assert.match(rules.stdout, /rules validate: OK/);

    // init + gate + report from inside the tarball, in a scratch workspace.
    const workspace = join(temp, 'workspace');
    const init = await node(['bin/diligent.mjs', 'init', 'demo', '--root', workspace], pkg).catch(error => error);
    assert.equal(init.code ?? 0, 0, `${init.stdout ?? ''}${init.stderr ?? ''}`);
    const gate = await node(['bin/diligent.mjs', 'gate', 'demo', '--root', workspace], pkg).catch(error => error);
    assert.equal(gate.code, 2, 'a fresh audit must gate as incomplete from the tarball');
    const report = await node(['bin/diligent.mjs', 'report', 'demo', '--root', workspace], pkg);
    assert.match(report.stdout, /Wrote /);

    // Install from the tarball and run the installed copy.
    const destination = join(temp, 'installed');
    await node(['scripts/install.mjs', '--dest', destination, '--yes'], pkg);
    const installedVersion = await node(['bin/diligent.mjs', '--version'], destination);
    assert.equal(installedVersion.stdout.trim(), pack.version);
    const installed = (await readdir(destination)).sort();
    for (const name of ['docs', 'tests', 'fixtures', 'evals', 'crates']) {
      if (name === 'docs') {
        assert.deepEqual((await readdir(join(destination, 'docs'))).sort(), ['baseline-diff.md', 'known-limitations.md', 'sarif.md', 'supported-checks.md']);
      } else assert.ok(!installed.includes(name), `installer copied excluded ${name}`);
    }
    const manifest = JSON.parse(await readFile(join(destination, 'runtime-manifest.json'), 'utf8'));
    assert.equal(manifest.version, pack.version);
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
});
