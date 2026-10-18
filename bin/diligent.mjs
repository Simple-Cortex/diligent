#!/usr/bin/env node
/**
 * diligent: CLI entry point. A thin dispatcher; the logic lives in scripts/.
 * Zero dependencies, Node >= 20, no network access.
 */
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { detectEngine } from '../scripts/lib/engine.mjs';
import { DILIGENT_VERSION } from '../scripts/lib/cli.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

const UTILITIES = {
  init: 'init.mjs',
  status: 'status.mjs',
  gate: 'gate.mjs',
  report: 'report.mjs',
  'validate-skill': 'validate-skill.mjs',
  install: 'install.mjs',
  check: 'check.mjs',
  test: 'test.mjs'
};
const AGENT_PHASES = ['discover', 'scope', 'inventory', 'plan', 'collect', 'assess', 'propose', 'verify', 'resume'];

function usage() {
  return `diligent: audit a U.S. government website for accessibility and federal web obligations

Version ${DILIGENT_VERSION} is a contracts-and-gate release. It ships zero automated website checks.

Usage:
  diligent [--json] <command> [arguments]
  diligent --help | --version

CLI commands (implemented):
  init <audit-id>            create .diligent/<audit-id>/ with frozen authority and rule locks
  status <audit-id>          pipeline state and artifact validity
  gate <audit-id>            recompute the assessment deterministically (exit 0/1/2/3)
  report <audit-id>          render REPORT.md from ASSESSMENT.json
  rules validate             validate authorities, rule packs and profiles
  validate-skill             check skill frontmatter, paths and versions
  install                    copy the skill to ~/.agents/skills/diligent
  check                      run all checks
  test                       run the unit and fixture tests

Agent-performed phases (guided by skills/<phase>/SKILL.md, no CLI implementation in ${DILIGENT_VERSION}):
  ${AGENT_PHASES.join(', ')}

Global options:
  --json        machine-readable output where supported
  -h, --help    show help (also: diligent <command> --help)
  -v, --version print the version

Gate exit codes: 0 pass for declared scope, 1 mandatory failure, 2 incomplete, 3 invalid input.
Diligent is not a government service and does not certify anything.
`;
}

async function readVersion() {
  return JSON.parse(await readFile(join(ROOT, 'package.json'), 'utf8')).version;
}

function runScript(script, args) {
  return new Promise(resolvePromise => {
    const child = spawn(process.execPath, [join(ROOT, 'scripts', script), ...args], { stdio: 'inherit' });
    child.on('error', error => { console.error(`diligent: failed to start ${script}: ${error.message}`); resolvePromise(3); });
    child.on('close', (code, signal) => resolvePromise(signal ? 1 : code ?? 1));
  });
}

async function main(argv) {
  const json = argv.includes('--json');
  const args = argv.filter(arg => arg !== '--json');
  const [command, ...rest] = args;

  if (!command || command === '--help' || command === '-h' || command === 'help') {
    process.stdout.write(usage());
    return 0;
  }
  if (command === '--version' || command === '-v') {
    const version = await readVersion();
    if (json) console.log(JSON.stringify({ name: 'diligent', version, engine: detectEngine() }, null, 2));
    else console.log(version);
    return 0;
  }
  if (command === 'rules') {
    if (rest[0] === 'validate') return runScript('validate-rules.mjs', [...rest.slice(1), ...(json ? ['--json'] : [])]);
    if (!rest[0] || rest[0] === '--help' || rest[0] === '-h') { console.log('Usage: diligent rules validate [--data-root <dir>] [--json]'); return rest[0] ? 0 : 2; }
    console.error(`diligent: unknown rules command "${rest[0]}". Use "diligent rules validate".`);
    return 2;
  }
  if (command in UTILITIES) return runScript(UTILITIES[command], [...rest, ...(json ? ['--json'] : [])]);
  if (AGENT_PHASES.includes(command)) {
    const message = `${command} is an agent-performed phase in ${DILIGENT_VERSION} and has no executable implementation. Read ${command === 'resume' ? 'references/workflow.md' : `skills/${command}/SKILL.md`}.`;
    if (json) console.log(JSON.stringify({ command, status: 'not_implemented', exit_code: 3, message, engine: detectEngine() }, null, 2));
    else console.error(message);
    return 3;
  }
  console.error(`diligent: unknown command "${command}". Run "diligent --help".`);
  return 2;
}

main(process.argv.slice(2)).then(code => { process.exitCode = code; }, error => {
  console.error(`diligent: ${error.stack || error.message}`);
  process.exitCode = 3;
});
