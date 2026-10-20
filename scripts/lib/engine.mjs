/** Engine selection. Never downloads anything; only probes a binary already on PATH. */
import { probeBinary } from './orchestrate/engine.mjs';

/** The commands the native binary runs when it is present. Node stays the fallback and the shadow reference. */
export const DELEGATED_COMMANDS = ['gate', 'rules validate'];

/** False when DILIGENT_ENGINE=node forces the Node path for every command. */
export function nativeDelegates(env = process.env) {
  return env.DILIGENT_ENGINE !== 'node';
}

/**
 * The engine record init writes into RUN.json (schemas/run.schema.json allows no other shape).
 * Init itself always runs in Node, so `executed_by` stays "node".
 */
export function detectEngine(env = process.env) {
  const probe = probeBinary(env);
  if (probe.ok) return { name: 'native-rust', executed_by: 'node', native_available: true, protocol_version: probe.protocol_version };
  return { name: 'node-fallback', executed_by: 'node', native_available: false, protocol_version: null };
}

/**
 * The engine envelope of `diligent --version --json`. When a binary reporting protocol 1 is present and
 * DILIGENT_ENGINE is not "node", `gate` and `rules validate` are executed by it and listed under `delegated`.
 */
export function describeEngine(env = process.env) {
  const base = detectEngine(env);
  const native = base.native_available && nativeDelegates(env);
  return { ...base, executed_by: native ? 'native-rust' : 'node', delegated: native ? [...DELEGATED_COMMANDS] : [] };
}
