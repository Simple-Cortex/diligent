/** action.yml is a composite Action. Without a YAML parser, check its shape by string search. */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { ROOT } from './helpers.mjs';

const INPUTS = ['url', 'audit-id', 'profile', 'pages', 'facts', 'facts-file', 'baseline', 'fail-on', 'upload-sarif', 'upload-artifacts', 'sarif-path', 'diligent-rs-version', 'diligent-rs-bin', 'allow-private', 'chrome-no-sandbox', 'baseline-out', 'timeout-ms', 'extra-args'];
const OUTPUTS = ['gate-exit', 'overall', 'run-id', 'audit-dir', 'sarif-path', 'baseline-path', 'summary-path', 'diff-exit', 'new-count', 'resolved-count'];

/** The text of one top-level section: from `name:` at column 0 to the next column-0 key. */
function section(text, name) {
  const match = text.match(new RegExp(`^${name}:\\n((?:[ \\t]+.*\\n|\\n)*)`, 'm'));
  assert.ok(match, `action.yml has no top-level ${name}:`);
  return match[1];
}

const action = await readFile(join(ROOT, 'action.yml'), 'utf8');

test('action.yml is a composite action', () => {
  assert.match(section(action, 'runs'), /^ {2}using: composite$/m);
});

test('action.yml declares every input and output', () => {
  const inputs = section(action, 'inputs');
  const outputs = section(action, 'outputs');
  for (const name of INPUTS) assert.match(inputs, new RegExp(`^ {2}${name}:$`, 'm'), `input ${name} is not declared`);
  for (const name of OUTPUTS) assert.match(outputs, new RegExp(`^ {2}${name}:$`, 'm'), `output ${name} is not declared`);
  assert.match(inputs, /^ {2}url:\n(?: {4}.*\n)*? {4}required: true$/m, 'url must be required');
});

test('action.yml pins third-party actions by major tag only', () => {
  const uses = [...action.matchAll(/^\s*uses:\s*(\S+)/gm)].map(match => match[1]);
  assert.deepEqual([...new Set(uses)].sort(), ['actions/setup-node@v4', 'actions/upload-artifact@v4', 'github/codeql-action/upload-sarif@v3']);
});

test('every diligent subcommand the action invokes exists in bin/diligent.mjs UTILITIES', async () => {
  const bin = await readFile(join(ROOT, 'bin/diligent.mjs'), 'utf8');
  const block = bin.match(/const UTILITIES = \{([\s\S]*?)\};/)[1];
  const utilities = new Set([...block.matchAll(/^\s*'?([\w-]+)'?:/gm)].map(match => match[1]));
  const invoked = [...action.matchAll(/bin\/diligent\.mjs"\s+(?:"\$\{args\[@\]\}"|([a-z-]+))/g)].map(match => match[1]).filter(Boolean);
  // Commands built into an args array: args=(audit ...), args=(sarif ...).
  for (const match of action.matchAll(/args=\(([a-z-]+)\s/g)) invoked.push(match[1]);
  assert.ok(invoked.length >= 3, `found too few invocations: ${invoked}`);
  for (const command of new Set(invoked)) assert.ok(utilities.has(command), `${command} is not in UTILITIES`);
  for (const required of ['audit', 'sarif', 'diff', 'baseline']) assert.ok(invoked.includes(required), `action does not invoke ${required}`);
});

test('action.yml sets the Chrome sandbox opt-out only on the audit step, never in GITHUB_ENV', () => {
  assert.doesNotMatch(action, /GITHUB_ENV[^\n]*NO_SANDBOX|NO_SANDBOX[^\n]*GITHUB_ENV/);
  assert.match(action, /^\s+DILIGENT_CHROME_NO_SANDBOX: .*chrome-no-sandbox/m);
});

test('baseline-out writes a baseline for the run and uploads it with the audit artifacts', () => {
  assert.match(action, /^ {2}baseline-out:\n(?: {4}.*\n)*? {4}default: ''$/m);
  assert.match(action, /baseline write --run "\$RUN_ID" --dir "\$AUDIT_DIR" --out "\$target"/);
  assert.match(action, /IN_BASELINE_OUT: \$\{\{ inputs\.baseline-out \}\}/);
  const upload = action.slice(action.indexOf('actions/upload-artifact@v4'));
  assert.match(upload, /steps\.write-baseline\.outputs\.baseline-path/);
});

test('action.yml never interpolates inputs into shell text', () => {
  // Inputs reach scripts through env: only, never inside a run: block.
  const runBodies = action.split(/^\s*run: \|\n/m).slice(1).map(body => body.split(/^\s{4}- /m)[0]);
  for (const body of runBodies) assert.doesNotMatch(body, /\$\{\{\s*inputs\./, 'a run block interpolates an input');
});
