/** Engine selection. Never downloads anything; only probes a binary already on PATH. */
import { spawnSync } from 'node:child_process';

/**
 * If `diligent-rs` (or $DILIGENT_RS_BIN) answers `protocol-version` with 1, the
 * native engine is reported. In this release no command is delegated: Node executes
 * everything, and the report says so.
 */
export function detectEngine(env = process.env) {
  const binary = env.DILIGENT_RS_BIN || 'diligent-rs';
  try {
    const probe = spawnSync(binary, ['protocol-version'], { encoding: 'utf8', timeout: 2000 });
    if (probe.status === 0 && probe.stdout.trim() === '1') {
      return { name: 'native-rust', executed_by: 'node', native_available: true, protocol_version: '1' };
    }
  } catch { /* fall through */ }
  return { name: 'node-fallback', executed_by: 'node', native_available: false, protocol_version: null };
}
