#!/usr/bin/env node
/**
 * Shadow suite for the gate command.
 *
 * For every case directory under fixtures/gate/ and fixtures/shadow/gate/,
 * copy it to two temp directories, run `node scripts/gate.mjs` and
 * `diligent-rs gate` with the same arguments, and compare exit code, stdout,
 * stderr, ASSESSMENT.json, EVENTS.jsonl, and runs/run-id/RUN.json byte for byte.
 *
 * Two normalizations: the "ts":"..." value in EVENTS.jsonl becomes <ts> on both sides,
 * and each side's temporary audit directory path becomes <audit> in stdout and stderr.
 *
 * Exit 0 when all match or skipped (diligent-rs not found). Exit 1 on any difference.
 */
import { cp, mkdtemp, rm, readFile, readdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { PACKAGE_ROOT } from './lib/artifacts.mjs';

const __dirname = resolve(fileURLToPath(import.meta.url), '..');
const FIXTURES_GATE = join(PACKAGE_ROOT, 'fixtures', 'gate');
const FIXTURES_SHADOW = join(PACKAGE_ROOT, 'fixtures', 'shadow', 'gate');

function resolveDiligentRs() {
  const bin = process.env.DILIGENT_RS_BIN || 'diligent-rs';
  const probe = spawnSync(bin, ['protocol-version'], { encoding: 'utf8', timeout: 2000 });
  if (probe.status === 0) return bin;
  return null;
}

function normalizeTs(text) {
  return text.replace(/"ts"\s*:\s*"[^"]+"/g, '"ts":"<ts>"');
}

function normalizeText(text, tempDir) {
  // The audit directory lives in a per-run temp directory; its path is not part of the contract.
  return text.split(tempDir).join('<audit>');
}

async function runCommand(cmd, args, cwd) {
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

async function readIfExists(path) {
  try {
    return await readFile(path, 'utf8');
  } catch {
    return null;
  }
}

async function runCase(caseDir, caseName, diligentRs, withJson) {
  const tempNode = await mkdtemp(join(tmpdir(), 'diligent-shadow-gate-node-'));
  const tempRust = await mkdtemp(join(tmpdir(), 'diligent-shadow-gate-rust-'));

  try {
    await cp(caseDir, tempNode, { recursive: true });
    await cp(caseDir, tempRust, { recursive: true });

    // Check for case.json with custom args
    let args = [];
    const caseJsonPath = join(caseDir, 'case.json');
    const caseJson = await readIfExists(caseJsonPath);
    if (caseJson) {
      try {
        const parsed = JSON.parse(caseJson);
        if (parsed.args) {
          args = parsed.args;
        }
      } catch {
        // ignore parse errors
      }
    }

    // Default args if none specified
    if (args.length === 0) {
      args = ['--dir', '.', '--now', '2026-01-01T00:00:00.000Z'];
    }

    if (withJson) args = [...args, '--json'];

    // Run Node version (use absolute path to script)
    const nodeScript = join(__dirname, 'gate.mjs');
    const nodeResult = await runCommand('node', [nodeScript, ...args], tempNode);

    // Run Rust version (use absolute path to binary)
    const rustBin = resolve(diligentRs);
    const rustResult = await runCommand(rustBin, ['gate', ...args], tempRust);

    // Compare exit codes
    const diffs = [];
    if (nodeResult.code !== rustResult.code) {
      diffs.push(`exit code: node=${nodeResult.code} rust=${rustResult.code}`);
    }

    // Compare stdout (normalize paths)
    if (normalizeText(nodeResult.stdout, tempNode) !== normalizeText(rustResult.stdout, tempRust)) {
      diffs.push('stdout differs');
    }

    // Compare stderr
    if (normalizeText(nodeResult.stderr, tempNode) !== normalizeText(rustResult.stderr, tempRust)) {
      diffs.push('stderr differs');
    }

    // Compare ASSESSMENT.json
    const nodeAssessment = await readIfExists(join(tempNode, 'ASSESSMENT.json'));
    const rustAssessment = await readIfExists(join(tempRust, 'ASSESSMENT.json'));
    if (nodeAssessment !== rustAssessment) {
      diffs.push('ASSESSMENT.json differs');
    }

    // Compare EVENTS.jsonl with ts normalization
    const nodeEvents = await readIfExists(join(tempNode, 'EVENTS.jsonl'));
    const rustEvents = await readIfExists(join(tempRust, 'EVENTS.jsonl'));
    if (nodeEvents !== null && rustEvents !== null) {
      const normalizedNode = normalizeTs(nodeEvents);
      const normalizedRust = normalizeTs(rustEvents);
      if (normalizedNode !== normalizedRust) {
        diffs.push('EVENTS.jsonl differs (after ts normalization)');
      }
    } else if (nodeEvents !== rustEvents) {
      diffs.push('EVENTS.jsonl presence differs');
    }

    // Compare runs/*/RUN.json
    const nodeRunsDir = join(tempNode, 'runs');
    const rustRunsDir = join(tempRust, 'runs');
    const nodeRuns = await readdir(nodeRunsDir).catch(() => []);
    const rustRuns = await readdir(rustRunsDir).catch(() => []);

    if (nodeRuns.join() !== rustRuns.join()) diffs.push('runs/ listing differs');
    for (const runId of nodeRuns) {
      const nodeRunJson = await readIfExists(join(nodeRunsDir, runId, 'RUN.json'));
      const rustRunJson = await readIfExists(join(rustRunsDir, runId, 'RUN.json'));
      if (nodeRunJson !== rustRunJson) {
        diffs.push(`runs/${runId}/RUN.json differs`);
      }
    }

    return {
      caseName,
      match: diffs.length === 0,
      diffs
    };
  } finally {
    await rm(tempNode, { recursive: true, force: true });
    await rm(tempRust, { recursive: true, force: true });
  }
}

async function main() {
  const diligentRs = resolveDiligentRs();
  if (!diligentRs) {
    console.log('SKIP: diligent-rs not found');
    return 0;
  }

  const cases = [];

  // Collect cases from fixtures/gate/
  try {
    const gateCases = await readdir(FIXTURES_GATE);
    for (const c of gateCases) {
      const caseDir = join(FIXTURES_GATE, c);
      const stat = await import('node:fs/promises').then(fs => fs.stat(caseDir));
      if (stat.isDirectory()) {
        cases.push({ dir: caseDir, name: `gate/${c}` });
      }
    }
  } catch {
    // fixtures/gate/ might not exist
  }

  // Collect cases from fixtures/shadow/gate/
  try {
    const shadowCases = await readdir(FIXTURES_SHADOW);
    for (const c of shadowCases) {
      const caseDir = join(FIXTURES_SHADOW, c);
      const stat = await import('node:fs/promises').then(fs => fs.stat(caseDir));
      if (stat.isDirectory()) {
        cases.push({ dir: caseDir, name: `shadow/gate/${c}` });
      }
    }
  } catch {
    // fixtures/shadow/gate/ might not exist
  }

  if (cases.length === 0) {
    console.log('No shadow cases found');
    return 0;
  }

  const results = [];
  for (const c of cases) {
    results.push(await runCase(c.dir, c.name, diligentRs, false));
    results.push(await runCase(c.dir, `${c.name} --json`, diligentRs, true));
  }

  console.log('Case'.padEnd(50) + 'Result'.padEnd(10) + 'Details');
  console.log('-'.repeat(80));
  let allMatch = true;
  for (const r of results) {
    if (r.match) {
      console.log(r.caseName.padEnd(50) + 'OK'.padEnd(10));
    } else {
      console.log(r.caseName.padEnd(50) + 'DIFF'.padEnd(10) + r.diffs.join(', '));
      allMatch = false;
    }
  }

  return allMatch ? 0 : 1;
}

main().then(code => { process.exitCode = code; }, err => {
  console.error(`shadow-gate: ${err.message}`);
  process.exitCode = 1;
});
