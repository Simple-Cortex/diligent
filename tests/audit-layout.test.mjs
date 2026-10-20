/** sarif, baseline write and diff over a real audit layout: results at the top, RUN.json under runs/<run-id>. */
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { ROOT, run, withTemp } from './helpers.mjs';

const AUDIT = join(ROOT, 'fixtures', 'diff', 'audit-layout');
const RUN_ID = 'run-2026-10-19-ddd001';
const RUN_DIR = join(AUDIT, 'runs', RUN_ID);
const VIEWS = [
  ['--run + --dir <audit dir>', ['--run', RUN_ID, '--dir', AUDIT]],
  ['--dir <audit dir>/runs/<id>', ['--dir', RUN_DIR]]
];

for (const [label, selector] of VIEWS) {
  describe(`audit layout, ${label}`, () => {
    it('sarif emits the results from the audit directory', async () => {
      const out = await run('scripts/sarif.mjs', selector);
      assert.equal(out.code, 0, out.stderr);
      const sarif = JSON.parse(out.stdout);
      const props = sarif.runs[0].invocations[0].properties;
      assert.equal(props.results_total, 3);
      assert.equal(props.results_emitted, 2);
      assert.equal(sarif.runs[0].results.length, 2);
    });

    it('baseline write records result-derived entries, and diff against it finds nothing new', async () => {
      await withTemp(async dir => {
        const file = join(dir, 'baseline.json');
        const written = await run('scripts/baseline.mjs', ['write', ...selector, '--out', file]);
        assert.equal(written.code, 0, written.stderr);
        const baseline = JSON.parse(await readFile(file, 'utf8'));
        assert.equal(baseline.fingerprints.length, 2);
        assert.ok(baseline.fingerprints.every(entry => entry.source === 'result'));

        const same = await run('scripts/diff.mjs', [...selector, '--baseline', file, '--json', '--fail-on', 'new']);
        assert.equal(same.code, 0, same.stderr);
        const parsed = JSON.parse(same.stdout);
        assert.equal(parsed.new.length, 0);
        assert.equal(parsed.known.length, 2);

        baseline.fingerprints.shift();
        await writeFile(file, JSON.stringify(baseline));
        const changed = await run('scripts/diff.mjs', [...selector, '--baseline', file, '--fail-on', 'new']);
        assert.equal(changed.code, 1);
        assert.match(changed.stdout, /New: 1/);
        assert.match(changed.stdout, /result \S+: \S+ @ https:\/\/agency\.example\.gov\//);
      });
    });
  });
}
