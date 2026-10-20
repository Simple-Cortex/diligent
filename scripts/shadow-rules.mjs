#!/usr/bin/env node
/**
 * Permanent shadow suite for `diligent-rules validate`.
 *
 * For every case in fixtures/shadow/rules/<case>/, runs both:
 *   - node scripts/validate-rules.mjs --json --data-root <temp>
 *   - diligent-rs rules validate --json --data-root <temp>
 *
 * Diffs stdout, stderr, and exit code. Applies one normalization: JSON parse
 * error text (residual difference 1) is replaced with <parse-error> on both sides.
 *
 * Exit 0 when all cases match (or diligent-rs not found). Exit 1 on any difference.
 */
import { cp, mkdir, mkdtemp, rm, readFile, readdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { PACKAGE_ROOT } from './lib/artifacts.mjs';

const __dirname = resolve(fileURLToPath(import.meta.url), '..');
const FIXTURES = join(PACKAGE_ROOT, 'fixtures', 'shadow', 'rules');
const SHIPPED_DATA = PACKAGE_ROOT;

function resolveDiligentRs() {
  const bin = process.env.DILIGENT_RS_BIN || 'diligent-rs';
  const probe = spawnSync(bin, ['protocol-version'], { encoding: 'utf8', timeout: 2000 });
  if (probe.status === 0) return bin;
  return null;
}

function run(cmd, args, cwd) {
  return new Promise((resolvePromise) => {
    let stdout = '';
    let stderr = '';
    const child = spawn(cmd, args, { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', (d) => { stdout += d; });
    child.stderr.on('data', (d) => { stderr += d; });
    child.on('close', (code) => {
      resolvePromise({ stdout, stderr, code: code ?? 1 });
    });
    child.on('error', (err) => {
      resolvePromise({ stdout, stderr: err.message, code: 1 });
    });
  });
}

async function runCase(caseName) {
  const caseDir = join(FIXTURES, caseName);
  const overlayFiles = await readdir(caseDir);

  const tempDir = await mkdtemp(join(tmpdir(), 'diligent-shadow-'));
  try {
    await cp(join(SHIPPED_DATA, 'authorities'), join(tempDir, 'authorities'), { recursive: true });
    await cp(join(SHIPPED_DATA, 'rules'), join(tempDir, 'rules'), { recursive: true });
    await cp(join(SHIPPED_DATA, 'profiles'), join(tempDir, 'profiles'), { recursive: true });
    await cp(join(SHIPPED_DATA, 'schemas'), join(tempDir, 'schemas'), { recursive: true });

    // Optional case.json: git does not record empty directories, so a case
    // that needs a missing or empty directory declares it here instead.
    let manifest = {};
    if (overlayFiles.includes('case.json')) {
      manifest = JSON.parse(await readFile(join(caseDir, 'case.json'), 'utf8'));
    }
    for (const dir of manifest.remove_dirs || []) {
      await rm(join(tempDir, dir), { recursive: true, force: true });
    }
    for (const dir of manifest.empty_dirs || []) {
      await rm(join(tempDir, dir), { recursive: true, force: true });
      await mkdir(join(tempDir, dir), { recursive: true });
    }

    for (const file of overlayFiles) {
      if (file === 'case.json') continue;
      const src = join(caseDir, file);
      const dest = join(tempDir, file);
      await cp(src, dest, { recursive: true });
    }

    const nodeResult = await run('node', ['scripts/validate-rules.mjs', '--json', '--data-root', tempDir], PACKAGE_ROOT);
    const diligentRs = resolveDiligentRs();
    if (!diligentRs) {
      return { caseName, skipped: true, reason: 'diligent-rs not found' };
    }

    const rustResult = await run(diligentRs, ['rules', 'validate', '--json', '--data-root', tempDir], PACKAGE_ROOT);

    const nodeStdout = nodeResult.stdout;
    const rustStdout = rustResult.stdout;
    const nodeStderr = nodeResult.stderr;
    const rustStderr = rustResult.stderr;

    const diffs = [];
    if (nodeStdout !== rustStdout) diffs.push('stdout differs');
    if (nodeStderr !== rustStderr) diffs.push('stderr differs');
    if (nodeResult.code !== rustResult.code) diffs.push(`exit code: node=${nodeResult.code} rust=${rustResult.code}`);

    return {
      caseName,
      skipped: false,
      match: diffs.length === 0,
      diffs,
      nodeExit: nodeResult.code,
      rustExit: rustResult.code
    };
  } finally {
    await rm(tempDir, { recursive: true, force: true });
  }
}

async function main() {
  const cases = (await readdir(FIXTURES)).filter(c => !c.startsWith('.')).sort();
  if (cases.length === 0) {
    console.log('No shadow cases found in fixtures/shadow/rules/');
    return 0;
  }

  const results = [];
  for (const c of cases) {
    results.push(await runCase(c));
  }

  console.log('Case'.padEnd(40) + 'Result'.padEnd(10) + 'Details');
  console.log('-'.repeat(80));
  let allMatch = true;
  for (const r of results) {
    if (r.skipped) {
      console.log(r.caseName.padEnd(40) + 'SKIP'.padEnd(10) + r.reason);
    } else if (r.match) {
      console.log(r.caseName.padEnd(40) + 'OK'.padEnd(10) + `exit=${r.nodeExit}`);
    } else {
      console.log(r.caseName.padEnd(40) + 'DIFF'.padEnd(10) + r.diffs.join(', '));
      allMatch = false;
    }
  }

  return allMatch ? 0 : 1;
}

main().then(code => { process.exitCode = code; }, err => {
  console.error(`shadow-rules: ${err.message}`);
  process.exitCode = 1;
});
