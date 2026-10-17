/** Tiny argument parser shared by the scripts. No dependencies. */
import { realpathSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { UsageError } from './artifacts.mjs';

/** True when the module at `metaUrl` is the process entry point (works across symlinks and platforms). */
export function isMain(metaUrl) {
  if (!process.argv[1]) return false;
  try { return realpathSync(process.argv[1]) === realpathSync(fileURLToPath(metaUrl)); } catch { return false; }
}

/**
 * parseArgs(argv, { flags: { name: 'boolean' | 'string' }, aliases })
 * Returns { positional, options }. Unknown --options are an error.
 */
export function parseArgs(argv, spec = {}) {
  const flags = { help: 'boolean', ...spec };
  const options = {};
  const positional = [];
  for (let i = 0; i < argv.length; i++) {
    const token = argv[i];
    if (token === '-h') { options.help = true; continue; }
    if (token === '--') { positional.push(...argv.slice(i + 1)); break; }
    if (!token.startsWith('--')) { positional.push(token); continue; }
    const [rawName, inline] = token.slice(2).split(/=(.*)/s, 2);
    const kind = flags[rawName];
    if (!kind) throw new UsageError(`unknown option --${rawName}. Run with --help.`);
    if (kind === 'boolean') {
      if (inline !== undefined) throw new UsageError(`option --${rawName} does not take a value.`);
      options[rawName] = true;
    } else {
      const value = inline !== undefined ? inline : argv[++i];
      if (value === undefined || (inline === undefined && value.startsWith('--'))) throw new UsageError(`option --${rawName} requires a value.`);
      options[rawName] = value;
    }
  }
  return { positional, options };
}

/** Run a script main function with uniform error and exit-code handling. */
export async function runMain(main) {
  try {
    const code = await main();
    process.exitCode = typeof code === 'number' ? code : 0;
  } catch (error) {
    if (error instanceof UsageError) {
      console.error(`diligent: ${error.message}`);
      process.exitCode = 2;
    } else {
      console.error(`diligent: ${error.stack || error.message}`);
      process.exitCode = 3;
    }
  }
}
