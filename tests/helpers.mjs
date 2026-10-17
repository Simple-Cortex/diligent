import { execFile } from 'node:child_process';
import { cp, mkdtemp, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
export const FIXED_NOW = '2026-10-17T00:00:00.000Z';

/** Run a node script from the package; never throws on a non-zero exit. */
export function run(script, args = [], { cwd = ROOT, env = {} } = {}) {
  return new Promise(resolve => {
    execFile(process.execPath, [join(ROOT, script), ...args], { cwd, env: { ...process.env, DILIGENT_RS_BIN: '/nonexistent/diligent-rs', ...env } }, (error, stdout, stderr) => {
      resolve({ code: error ? (typeof error.code === 'number' ? error.code : 1) : 0, stdout, stderr });
    });
  });
}

/** Fresh temp directory with its real path (macOS /var -> /private/var). */
export async function tempDir(prefix = 'diligent-test-') {
  return realpath(await mkdtemp(join(tmpdir(), prefix)));
}

export async function withTemp(fn, prefix) {
  const dir = await tempDir(prefix);
  try { return await fn(dir); } finally { await rm(dir, { recursive: true, force: true }); }
}

export async function copyTo(source, destination) {
  await cp(source, destination, { recursive: true });
}
