/**
 * Tests for the permanent shadow suite for `diligent-rules validate`.
 *
 * Runs scripts/shadow-rules.mjs and asserts exit 0 (all cases match or skipped).
 */
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function runScript(script, args = []) {
  return new Promise((resolvePromise) => {
    let stdout = '';
    let stderr = '';
    const child = spawn('node', [script, ...args], { cwd: __dirname, stdio: ['ignore', 'pipe', 'pipe'] });
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

describe('Shadow rules suite', () => {
  it('runs all shadow cases and exits 0 when all match or skipped', { timeout: 60000 }, async () => {
    const result = await runScript('scripts/shadow-rules.mjs');
    assert.equal(result.code, 0, `shadow-rules failed:\nstdout: ${result.stdout}\nstderr: ${result.stderr}`);
    assert.ok(result.stdout.includes('Case'), 'should print table header');
  });
});
