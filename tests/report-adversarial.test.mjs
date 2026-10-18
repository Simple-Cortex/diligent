import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { SECTION_TITLES, SIGNALS_TITLE, renderReport } from '../scripts/report.mjs';
import { sha256 } from '../scripts/lib/artifacts.mjs';
import { ROOT, run, withTemp, copyTo } from './helpers.mjs';

const GATE = join(ROOT, 'fixtures', 'gate');
const RUN = 'run-2026-10-17-abc123';
const section10 = text => text.slice(text.indexOf(`## 10. ${SIGNALS_TITLE}`));

const ASSESSMENT = { schema_version: '1.0.0', run_id: RUN, computed_at: '2026-10-17T00:00:00.000Z', overall: 'incomplete', has_incomplete_work: true,
  counts: { selected: 0, applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, required_instances: 0, executed: 0, reviewed: 0, unsupported: 0 },
  denominators: { applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, executed: 0, reviewed: 0, unsupported: 0 },
  unresolved: [], failures: [], exceptions: [], ux_findings_ref: null, release_disposition: null, limitations: [], claim_boundary: 'Boundary.' };

function signal(over = {}) {
  return {
    schema_version: '1.0.0', signal_id: `sig-${Math.random().toString(36).slice(2, 14)}`, signal_key: 'content.readability.fk_grade', family: 'content',
    instance_id: 'page-1', state: 'lead', value: 11.2, unit: 'grade', observed: 'Flesch-Kincaid grade 11.2 over 420 words.',
    threshold: { rule: 'fk_grade <= 8', source: 'plainlanguage.gov Federal Plain Language Guidelines', confidence: 'medium' },
    label: 'static', normative: false, evidence_refs: [], limitations: ['Readability formulas are approximate.'],
    executed_at: '2026-10-17T00:00:00.000Z', engine_version: '0.2.0', run_id: RUN, extensions: {}, ...over
  };
}

async function gated(name, fn) {
  await withTemp(async tmp => {
    const dir = join(tmp, name);
    await copyTo(join(GATE, name), dir);
    await run('scripts/gate.mjs', ['--dir', dir, '--now', '2026-10-17T00:00:00.000Z']);
    return fn(dir);
  });
}

test('adv-report: SIGNALS.jsonl with 10,000 rows renders under 2 s and caps lead lists', () => {
  const signals = Array.from({ length: 10000 }, (_, i) => signal({ instance_id: `inst-${i}`, observed: `line one\nline two \`${'x'.repeat(500)}` }));
  const started = Date.now();
  const text = renderReport({ auditId: 'demo', assessment: ASSESSMENT, signals });
  const elapsed = Date.now() - started;
  assert.ok(elapsed < 2000, `rendering 10,000 signals took ${elapsed}ms (limit: 2000ms)`);
  const tail = section10(text);
  assert.ok(tail.includes('Worth a look (10000):'), 'the count is shown');
  assert.ok(tail.includes('9990 more instances in SIGNALS.jsonl.'), 'excess instances are capped');
  assert.ok(tail.split('\n').length < 100, 'the report stays readable');
  assert.doesNotMatch(tail.replace(/`[^`]*`/g, ''), /line one\nline two/, 'untrusted text is flattened to one line');
});

test('adv-report: garbage lines in SIGNALS.jsonl do not change ASSESSMENT.json', async () => {
  await gated('mandatory-fail', async dir => {
    const before = await readFile(join(dir, 'ASSESSMENT.json'), 'utf8');
    await writeFile(join(dir, 'SIGNALS.jsonl'), `not json at all\n${JSON.stringify(signal())}\nalso not json\n`);
    const result = await run('scripts/report.mjs', ['--dir', dir]);
    assert.equal(result.code, 0, result.stderr);
    const after = await readFile(join(dir, 'ASSESSMENT.json'), 'utf8');
    assert.equal(after, before, 'ASSESSMENT.json is byte-identical regardless of SIGNALS.jsonl content');
    const report = await readFile(join(dir, 'REPORT.md'), 'utf8');
    const tail = section10(report);
    assert.match(tail, /Signals recorded: 1\./, 'the one valid row is rendered');
    assert.match(tail, /2 rows in SIGNALS\.jsonl failed schema validation and were not rendered\./);
    assert.doesNotMatch(tail, /could not be rendered/);
  });
});

test('adv-report: a RULES.lock pack entry with a path traversal is ignored', async () => {
  await gated('mandatory-fail', async dir => {
    const lock = JSON.parse(await readFile(join(dir, 'RULES.lock.json'), 'utf8'));
    const original = JSON.stringify(lock);
    lock.packs[0].path = '../../../etc/passwd';
    await writeFile(join(dir, 'RULES.lock.json'), JSON.stringify(lock, null, 2));
    const result = await run('scripts/report.mjs', ['--dir', dir]);
    assert.equal(result.code, 0, result.stderr);
    const report = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.match(report, /## 4\. Accessibility coverage/);
    const after = await readFile(join(dir, 'RULES.lock.json'), 'utf8');
    assert.equal(after, JSON.stringify(lock, null, 2), 'the lock file is not rewritten');
    assert.notEqual(after, original, 'the tampered lock file is still there but the path is ignored');
  });
});
