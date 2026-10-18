/**
 * Running the diligent-rs companion binary and child Node scripts for `diligent audit`.
 * The binary is probed, never downloaded. Output is captured and capped.
 */
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

export const ENGINE_MISSING_MESSAGE = 'diligent audit needs the diligent-rs companion binary (protocol 1.x); see docs/install.md. init, status, gate and report work without it.';
const OUTPUT_CAP = 8 * 1024 * 1024;

export function engineBinary(env = process.env) {
  return env.DILIGENT_RS_BIN || 'diligent-rs';
}

/** Probe the binary. Returns { ok, binary, protocol_version, binary_version } or { ok: false, reason }. */
export function probeBinary(env = process.env) {
  const binary = engineBinary(env);
  const protocol = spawnSync(binary, ['protocol-version'], { encoding: 'utf8', timeout: 5000, env });
  if (protocol.error) return { ok: false, reason: protocol.error.code === 'ENOENT' ? 'not found' : protocol.error.message };
  if (protocol.status !== 0) return { ok: false, reason: `protocol-version exited ${protocol.status}` };
  const version = protocol.stdout.trim();
  if (!/^1(\.\d+)*$/.test(version)) return { ok: false, reason: `protocol ${JSON.stringify(version.slice(0, 20))} is not 1.x` };
  const release = spawnSync(binary, ['--version'], { encoding: 'utf8', timeout: 5000, env });
  const binaryVersion = release.status === 0 ? (release.stdout.trim().split(/\s+/).at(-1) || null) : null;
  return { ok: true, binary, protocol_version: version, binary_version: binaryVersion };
}

function capture(command, args, { env, cwd, killAfterMs }) {
  return new Promise(resolve => {
    let child;
    try { child = spawn(command, args, { env, cwd, stdio: ['ignore', 'pipe', 'pipe'] }); } catch (error) {
      resolve({ code: 3, stdout: '', stderr: error.message, error }); return;
    }
    let stdout = ''; let stderr = ''; let settled = false;
    const append = (current, chunk) => (current.length < OUTPUT_CAP ? current + chunk : current);
    child.stdout.on('data', chunk => { stdout = append(stdout, chunk.toString('utf8')); });
    child.stderr.on('data', chunk => { stderr = append(stderr, chunk.toString('utf8')); });
    const timer = killAfterMs ? setTimeout(() => { stderr += `\nkilled after ${killAfterMs} ms`; child.kill('SIGKILL'); }, killAfterMs) : null;
    const done = result => { if (settled) return; settled = true; if (timer) clearTimeout(timer); resolve(result); };
    child.on('error', error => done({ code: 3, stdout, stderr: `${stderr}${error.message}`, error }));
    child.on('close', (code, signal) => done({ code: signal ? 3 : code ?? 3, stdout, stderr }));
  });
}

/** Run the companion binary. Never throws; a spawn failure comes back as code 3. */
export function runEngine(args, { env = process.env, cwd, killAfterMs = 15 * 60 * 1000 } = {}) {
  return capture(engineBinary(env), args, { env, cwd, killAfterMs });
}

/** Run a Node script from this package as a child process. */
export function runNode(script, args, { env = process.env, killAfterMs = 5 * 60 * 1000 } = {}) {
  return capture(process.execPath, [script, ...args], { env, killAfterMs });
}

export function pageSlug(url) {
  return createHash('sha256').update(url).digest('hex').slice(0, 12);
}

export function tryJson(text) {
  try { return JSON.parse(text); } catch { return null; }
}
