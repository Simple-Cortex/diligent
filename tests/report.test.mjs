import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { SECTION_TITLES, SIGNALS_TITLE, buildWcagIndex, renderReport } from '../scripts/report.mjs';
import { sha256 } from '../scripts/lib/artifacts.mjs';
import { FIXED_NOW, ROOT, copyTo, run, withTemp } from './helpers.mjs';

const GATE = join(ROOT, 'fixtures', 'gate');
async function gated(name, fn) {
  await withTemp(async tmp => {
    const dir = join(tmp, name);
    await copyTo(join(GATE, name), dir);
    await run('scripts/gate.mjs', ['--dir', dir, '--now', FIXED_NOW]);
    return fn(dir);
  });
}

test('REPORT.md has the required header and the nine sections in blueprint order', async () => {
  await gated('mandatory-fail', async dir => {
    const result = await run('scripts/report.mjs', ['--dir', dir]);
    assert.equal(result.code, 0, result.stderr);
    const text = await readFile(join(dir, 'REPORT.md'), 'utf8');
    const header = text.slice(0, text.indexOf('## 1.'));
    for (const label of ['Target:', 'Snapshot and date:', 'Audit modes:', 'Selected profiles:', 'Assessment status:']) assert.ok(header.includes(label), label);
    const positions = SECTION_TITLES.map((title, i) => text.indexOf(`## ${i + 1}. ${title}`));
    assert.ok(positions.every(p => p >= 0), `all sections present: ${positions}`);
    assert.deepEqual([...positions].sort((a, b) => a - b), positions, 'sections are in order');
    assert.equal(SECTION_TITLES.length, 9);
  });
});

test('a non-passing assessment still exits 0 when the report is written, and says so', async () => {
  for (const [name, status] of [['mandatory-fail', 'FAIL'], ['incomplete-untested', 'INCOMPLETE']]) {
    await gated(name, async dir => {
      const result = await run('scripts/report.mjs', ['--dir', dir]);
      assert.equal(result.code, 0);
      assert.match(result.stdout, /not that the assessment passed/);
      assert.match(await readFile(join(dir, 'REPORT.md'), 'utf8'), new RegExp(`Assessment status:\\*\\* ${status}`));
    });
  }
  const help = await run('scripts/report.mjs', ['--help']);
  assert.match(help.stdout, /Exit codes: 0 when REPORT\.md was written, even if the assessment is fail or/);
});

test('the report lists failures, exceptions and the absence of repairs without softening them', async () => {
  await gated('exception-does-not-pass', async dir => {
    await run('scripts/report.mjs', ['--dir', dir]);
    const text = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.match(text, /OBL-A11Y-1\.1\.1/);
    assert.match(text, /`exc-1` on `OBL-A11Y-1\.1\.1`: active\. It does not change any result\./);
    assert.match(text, /No repair plan and no verification exist/);
    assert.match(text, /No release decision is recorded/);
    assert.match(text, /not a government certification/);
  });
});

test('coverage appears as counts with denominators and no blended score', async () => {
  await gated('incomplete-unknown-applicability', async dir => {
    await run('scripts/report.mjs', ['--dir', dir]);
    const text = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.match(text, /\| Unresolved applicability \| 1 \| 2 \|/);
    assert.match(text, /never blended into a single score/);
    assert.doesNotMatch(text, /\d+\s*%/);
  });
});

test('the report renders the recorded assessment and never recomputes it', () => {
  const assessment = {
    schema_version: '1.0.0', run_id: 'run-2026-10-17-abc123', computed_at: '2026-10-17T00:00:00.000Z', overall: 'pass_for_declared_scope', has_incomplete_work: false,
    counts: { selected: 1, applicable: 1, not_applicable: 0, unresolved_applicability: 0, pending: 0, required_instances: 1, executed: 1, reviewed: 0, unsupported: 0 },
    denominators: { applicable: 1, not_applicable: 1, unresolved_applicability: 1, pending: 1, executed: 1, reviewed: 0, unsupported: 1 },
    unresolved: [], failures: [], exceptions: [], ux_findings_ref: null, release_disposition: null, limitations: [], claim_boundary: 'Boundary text.'
  };
  const withFailure = { ...assessment, failures: [{ result_id: 'r1', obligation_id: 'o', instance_id: 'i', check_id: 'c' }] };
  const text = renderReport({ auditId: 'demo', assessment: withFailure });
  assert.match(text, /PASS FOR DECLARED SCOPE/, 'the recorded overall is shown as recorded; fixing it is the gate\'s job');
  assert.match(text, /\| `o` \| `c` \| `i` \| `r1` \|/);
});

test('the report refuses a missing or invalid assessment', async () => {
  await withTemp(async tmp => {
    const missing = await run('scripts/report.mjs', ['--dir', tmp]);
    assert.equal(missing.code, 3);
    assert.match(missing.stderr, /ASSESSMENT\.json is missing/);
    await writeFile(join(tmp, 'ASSESSMENT.json'), '{"overall":"pass"}');
    const invalid = await run('scripts/report.mjs', ['--dir', tmp]);
    assert.equal(invalid.code, 3);
    assert.match(invalid.stderr, /refusing to render/);
    await rm(join(tmp, 'REPORT.md'), { force: true });
  });
});

test('init, gate and report together: demo audit reports incomplete and records the stage', async () => {
  await withTemp(async workspace => {
    const data = join(ROOT, 'fixtures', 'package');
    await run('scripts/init.mjs', ['demo', '--root', workspace, '--data-root', data, '--profile', 'mini-profile']);
    assert.equal((await run('scripts/gate.mjs', ['demo', '--root', workspace])).code, 2);
    const report = await run('scripts/report.mjs', ['demo', '--root', workspace]);
    assert.equal(report.code, 0, report.stderr);
    const text = await readFile(join(workspace, '.diligent', 'demo', 'REPORT.md'), 'utf8');
    assert.match(text, /INCOMPLETE/);
    assert.match(text, /rule pack `mini-core` `0\.1\.0`/);
    assert.match(text, /not yet human reviewed: 2 of 2/);
    const status = JSON.parse((await run('scripts/status.mjs', ['demo', '--root', workspace, '--json'])).stdout);
    assert.equal(status.pipeline_state, 'reported');
  });
});

const RUN = 'run-2026-10-17-abc123';
const SENTENCE = 'These are not legal obligations. They are practice signals, labelled lab, static or field, for a person to weigh.';
function signal(over = {}) {
  return {
    schema_version: '1.0.0', signal_id: 'sig-0123456789ab', signal_key: 'content.readability.fk_grade', family: 'content', instance_id: 'page-1', state: 'lead',
    value: 11.2, unit: 'grade', observed: 'Flesch-Kincaid grade 11.2 over 420 words.',
    threshold: { rule: 'fk_grade <= 8', source: 'plainlanguage.gov Federal Plain Language Guidelines', confidence: 'medium' },
    label: 'static', normative: false, evidence_refs: [], limitations: ['Readability formulas are approximate.'],
    executed_at: '2026-10-17T00:00:00.000Z', engine_version: '0.2.0', run_id: RUN, extensions: {}, ...over
  };
}
const jsonl = rows => `${rows.map(r => JSON.stringify(r)).join('\n')}\n`;
const section10 = text => text.slice(text.indexOf(`## 10. ${SIGNALS_TITLE}`));

test('section 10 is present without SIGNALS.jsonl, says so, and sections 1 to 9 keep their ids', async () => {
  await gated('mandatory-fail', async dir => {
    const result = await run('scripts/report.mjs', ['--dir', dir]);
    assert.equal(result.code, 0, result.stderr);
    const text = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.ok(text.includes('## 10. Craft and clarity (not legal obligations)'));
    const tail = section10(text);
    assert.ok(tail.includes(SENTENCE));
    assert.match(tail, /Not available: SIGNALS\.jsonl does not exist for this audit\. Nothing is implied by its absence\./);
    SECTION_TITLES.forEach((title, i) => assert.ok(text.includes(`## ${i + 1}. ${title}`)));
    assert.ok(text.indexOf('## 9.') < text.indexOf('## 10.'));
  });
});

test('section 10 groups signals by family, leads first, with threshold, source and confidence', async () => {
  await gated('mandatory-fail', async dir => {
    const rows = [
      signal(),
      signal({ signal_key: 'content.readability.passive_share', instance_id: 'page-1', state: 'within_practice', value: 0.1, unit: null, threshold: { rule: 'passive_share <= 0.20', source: 'plainlanguage.gov, use active voice', confidence: 'medium' } }),
      signal({ signal_key: 'craft.font_family_count', family: 'craft', state: 'lead', value: 5, unit: null, observed: 'Five families.', threshold: { rule: 'families <= 3', source: 'practice', confidence: 'low' } }),
      signal({ signal_key: 'perf.lcp_ms', family: 'lab_performance', state: 'informational', value: 3100, unit: 'ms', label: 'lab', threshold: null }),
      signal({ signal_key: 'perf.inp_ms', family: 'lab_performance', state: 'not_tested', value: null, unit: null, label: 'lab', threshold: null, limitations: ['INP needs real user interaction.'] })
    ];
    await writeFile(join(dir, 'SIGNALS.jsonl'), jsonl(rows));
    const result = await run('scripts/report.mjs', ['--dir', dir]);
    assert.equal(result.code, 0, result.stderr);
    const tail = section10(await readFile(join(dir, 'REPORT.md'), 'utf8'));
    assert.ok(tail.includes(SENTENCE));
    assert.match(tail, /Signals recorded: 5\. Leads: 2\. Within practice: 1\. Informational: 1\. Not tested: 1\./);
    const order = ['**Content and plain language**', '**Craft**', '**Lab performance'].map(h => tail.indexOf(h));
    assert.deepEqual([...order].sort((a, b) => a - b), order);
    assert.match(tail, /`content\.readability\.fk_grade` on page-1 \[static\]: 11\.2 grade\..*practice: fk\\_grade <= 8, aligned with plainlanguage\.gov Federal Plain Language Guidelines, confidence medium/);
    assert.ok(tail.indexOf('Worth a look (1):') < tail.indexOf('Within practice: 1'));
    assert.match(tail, /Informational numbers, with no threshold applied:\n- `perf\.lcp_ms` on page-1 \[lab\]: 3100 ms\./);
    assert.match(tail, /Not tested: 1 \(`perf\.inp_ms`\)/);
    assert.match(tail, /INP needs real user interaction\./);
    assert.doesNotMatch(tail.replace(/`[^`]*`/g, ''), /\brequired\b|\bmandatory\b|\bviolation\b/i, 'prose in section 10 never uses obligation language');
  });
});

test('signals never change the assessment status shown in the header or sections 1 to 9', async () => {
  await gated('mandatory-fail', async dir => {
    await run('scripts/report.mjs', ['--dir', dir]);
    const before = await readFile(join(dir, 'REPORT.md'), 'utf8');
    await writeFile(join(dir, 'SIGNALS.jsonl'), jsonl([signal(), signal({ state: 'within_practice' })]));
    await run('scripts/report.mjs', ['--dir', dir]);
    const after = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.equal(after.slice(0, after.indexOf('## 10.')), before.slice(0, before.indexOf('## 10.')));
  });
});

test('an invalid SIGNALS.jsonl is named in section 10 and does not block the report', async () => {
  await gated('mandatory-fail', async dir => {
    await writeFile(join(dir, 'SIGNALS.jsonl'), `${JSON.stringify(signal())}\nnot json\n${JSON.stringify({ ...signal(), normative: true })}\n`);
    const result = await run('scripts/report.mjs', ['--dir', dir]);
    assert.equal(result.code, 0, result.stderr);
    const text = await readFile(join(dir, 'REPORT.md'), 'utf8');
    assert.match(section10(text), /Signals recorded: 1\./);
    assert.match(section10(text), /2 rows in SIGNALS\.jsonl failed schema validation and were not rendered\./);
    assert.match(text, /Assessment status:\*\* FAIL/);
  });
});

test('a signals file from another run is flagged, and a very large signal set renders with capped lead lists', () => {
  const assessment = { schema_version: '1.0.0', run_id: RUN, computed_at: '2026-10-17T00:00:00.000Z', overall: 'incomplete', has_incomplete_work: true,
    counts: { selected: 0, applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, required_instances: 0, executed: 0, reviewed: 0, unsupported: 0 },
    denominators: { applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, executed: 0, reviewed: 0, unsupported: 0 },
    unresolved: [], failures: [], exceptions: [], ux_findings_ref: null, release_disposition: null, limitations: [], claim_boundary: 'Boundary.' };
  const other = renderReport({ auditId: 'demo', assessment, signals: [signal({ run_id: 'run-2026-10-16-ffffff' })] });
  assert.match(other, /Run mismatch: the signals were recorded for run-2026-10-16-ffffff but the assessment is for run-2026-10-17-abc123/);
  const many = Array.from({ length: 10000 }, (_, i) => signal({ instance_id: `inst-${i}`, observed: `line one\nline two \`${'x'.repeat(500)}` }));
  const started = Date.now();
  const text = renderReport({ auditId: 'demo', assessment, signals: many });
  assert.ok(Date.now() - started < 5000, 'renders 10,000 signals quickly');
  assert.ok(text.includes('Worth a look (10000):'));
  assert.ok(text.includes('9990 more instances in SIGNALS.jsonl.'));
  assert.ok(section10(text).split('\n').length < 100, 'the report stays readable');
  assert.doesNotMatch(section10(text), /line one\nline two/, 'untrusted text is flattened to one line');
});

test('empty, null-valued and unknown-family signals render without throwing', () => {
  const assessment = { schema_version: '1.0.0', run_id: RUN, computed_at: '2026-10-17T00:00:00.000Z', overall: 'incomplete', has_incomplete_work: true,
    counts: { selected: 0, applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, required_instances: 0, executed: 0, reviewed: 0, unsupported: 0 },
    denominators: { applicable: 0, not_applicable: 0, unresolved_applicability: 0, pending: 0, executed: 0, reviewed: 0, unsupported: 0 },
    unresolved: [], failures: [], exceptions: [], ux_findings_ref: null, release_disposition: null, limitations: [], claim_boundary: 'Boundary.' };
  assert.match(renderReport({ auditId: 'demo', assessment, signals: [] }), /SIGNALS\.jsonl exists but holds no signals\./);
  const text = renderReport({ auditId: 'demo', assessment, signals: [signal({ value: null, unit: null, threshold: null }), signal({ family: 'future_family', signal_key: 'future.key', value: { a: 1 } })] });
  assert.match(text, /no value/);
  assert.match(text, /\*\*future_family\*\*/);
});

async function realLock(packIds) {
  const packs = [];
  for (const id of packIds) {
    const path = `rules/packs/${id}.json`;
    packs.push({ pack_id: id, pack_version: '0.1.1', path, sha256: sha256(await readFile(join(ROOT, path))), rule_count: 0, normative: true });
  }
  return { packs };
}
const ASSESSMENT_BASE = { schema_version: '1.0.0', run_id: RUN, computed_at: '2026-10-17T00:00:00.000Z', overall: 'fail', has_incomplete_work: false,
  counts: { selected: 2, applicable: 2, not_applicable: 0, unresolved_applicability: 0, pending: 0, required_instances: 2, executed: 2, reviewed: 0, unsupported: 0 },
  denominators: { applicable: 2, not_applicable: 2, unresolved_applicability: 2, pending: 2, executed: 2, reviewed: 0, unsupported: 2 },
  unresolved: [], failures: [], exceptions: [], ux_findings_ref: null, release_disposition: null, limitations: [], claim_boundary: 'Boundary.' };

test('accessibility results are grouped by the three WCAG mappings from the rule packs and never blended', async () => {
  const wcag = await buildWcagIndex(await realLock(['a11y-wcag20-aa', 'a11y-wcag21-aa', 'a11y-wcag22-aa']), ROOT);
  assert.deepEqual(wcag.notes, []);
  assert.deepEqual([...new Set(wcag.index.get('OBL-A11Y-1.1.1').map(m => m.role))].sort(), ['legal_floor', 'recommended', 'title_ii_target']);
  const assessment = {
    ...ASSESSMENT_BASE,
    failures: [{ result_id: 'r1', obligation_id: 'OBL-A11Y-1.1.1', instance_id: 'i', check_id: 'c' }, { result_id: 'r2', obligation_id: 'OBL-A11Y-2.4.11', instance_id: 'i', check_id: 'c' }],
    unresolved: [{ kind: 'no_result', ref: 'OBL-A11Y-2.4.11', reason: 'x' }]
  };
  const applicability = { obligations: [{ obligation_id: 'OBL-A11Y-1.1.1' }, { obligation_id: 'OBL-A11Y-2.4.11' }] };
  const text = renderReport({ auditId: 'demo', assessment, applicability, wcag });
  const part = text.slice(text.indexOf('**Accessibility results by WCAG mapping.**'), text.indexOf('## 5.'));
  assert.match(part, /\| Legal floor \(WCAG 2\.0 A\/AA\) \| 1 \| 1 \| 0 \|/);
  assert.match(part, /\| ADA Title II target \(WCAG 2\.1 A\/AA\) \| 1 \| 1 \| 0 \|/);
  assert.match(part, /\| Recommended target \(WCAG 2\.2 A\/AA\) \| 2 \| 2 \| 1 \|/);
  assert.match(part, /never added together/);
  assert.match(part, /Recommended target: OBL-A11Y-1\.1\.1, OBL-A11Y-2\.4\.11/);
  assert.match(part, /Legal floor: OBL-A11Y-1\.1\.1\n/);
  assert.doesNotMatch(part, /\d+\s*%/);
});

test('an unreadable or changed rule pack is stated in the mapping part, never skipped silently', async () => {
  const lock = await realLock(['a11y-wcag22-aa']);
  const missing = await buildWcagIndex({ packs: [...lock.packs, { ...lock.packs[0], pack_id: 'gone', path: 'rules/packs/gone.json' }] }, ROOT);
  assert.match(missing.notes.join('\n'), /Rule pack gone could not be read/);
  const changed = await buildWcagIndex({ packs: [{ ...lock.packs[0], sha256: 'a'.repeat(64) }] }, ROOT);
  assert.match(changed.notes.join('\n'), /differs from the locked copy, so its WCAG mappings are not shown/);
  assert.equal(changed.index.size, 0, 'a pack with a different hash contributes nothing');
  const none = renderReport({ auditId: 'demo', assessment: ASSESSMENT_BASE, wcag: await buildWcagIndex(null, ROOT) });
  assert.match(none, /No accessibility rule pack with WCAG mappings is locked for this audit/);
});

test('a lock entry cannot steer the pack read: path is ignored, ids are validated, errors are not echoed', async () => {
  const real = (await realLock(['a11y-wcag22-aa'])).packs[0];
  const traversal = await buildWcagIndex({ packs: [{ ...real, path: '../../etc/passwd' }] }, ROOT);
  assert.ok(traversal.index.size > 0, 'the pack loads by id from rules/packs, not from the lock path');
  const badId = await buildWcagIndex({ packs: [{ ...real, pack_id: '../../etc/passwd', path: '../../etc/passwd' }, { ...real, pack_id: 'A/b' }] }, ROOT);
  assert.equal(badId.index.size, 0);
  assert.equal(badId.notes.length, 2);
  assert.ok(badId.notes.every(n => /invalid id/.test(n)));
  const absent = await buildWcagIndex({ packs: [{ ...real, pack_id: 'no-such-pack' }] }, ROOT);
  assert.deepEqual(absent.notes, ['Rule pack no-such-pack could not be read, so its WCAG mappings are not shown.']);
  assert.doesNotMatch(absent.notes.join(' '), /ENOENT|\//, 'no raw error text or paths');
});

test('untrusted signal text renders inert: links, images, HTML and table breaks are escaped', () => {
  const assessment = { ...ASSESSMENT_BASE, failures: [], unresolved: [] };
  const evil = '[x](http://evil) <img src=x onerror=1> | **b** _i_ ![p](u) # h &lt;';
  const text = renderReport({ auditId: 'demo', assessment, signals: [signal({ observed: evil, limitations: [evil], value: evil, signal_key: 'k`ey', instance_id: evil, threshold: { rule: evil, source: evil, confidence: 'low' } })] });
  const part = section10(text);
  assert.doesNotMatch(part, /(^|[^\\])\[x\]\(/, 'no live link');
  assert.doesNotMatch(part, /(^|[^\\])</, 'no live HTML');
  assert.doesNotMatch(part, /(^|[^\\])!\[/, 'no live image');
  assert.ok(part.includes('\\[x\\]\\(http://evil\\)'));
  assert.ok(part.includes('\\<img src=x onerror=1\\>'));
  assert.ok(part.includes('\\&lt;'));
  assert.doesNotMatch(part, /k`ey/, 'a backtick cannot break out of a code span');
});

test('hostile text in FINDINGS.json, SERVICE-BRIEF.json and the assessment renders inert in the report', () => {
  const evil = '[click](http://evil) <img src=x onerror=1> | **b** ![p](u) # h';
  const assessment = { ...ASSESSMENT_BASE, failures: [{ result_id: 'r`1', obligation_id: 'o|bl', instance_id: 'i', check_id: 'c' }],
    unresolved: [{ kind: 'no_result', ref: 'o`bl', reason: evil }], limitations: [evil], claim_boundary: evil };
  const brief = { brief_status: 'confirmed', service_name: evil, owner: evil, subject: evil, jobs: [{ description: evil, provenance: 'inferred' }, { description: evil, provenance: evil }] };
  const findings = { findings: [
    { finding_id: 'f`1', rule_id: 'r', category: 'mandatory_failure', severity: 'high', observation: evil, impact: evil },
    { finding_id: 'f2', rule_id: 'r', category: 'ux_barrier', severity: 'low', observation: evil, impact: 'x' }] };
  const text = renderReport({ auditId: 'demo', assessment, brief, findings });
  assert.doesNotMatch(text, /(^|[^\\])\[click\]\(/m, 'no live link');
  assert.doesNotMatch(text, /(^|[^\\])</m, 'no live HTML');
  assert.doesNotMatch(text, /(^|[^\\])!\[/m, 'no live image');
  assert.ok(text.includes('\\[click\\]\\(http://evil\\)'));
  assert.ok(text.includes('\\<img src=x onerror=1\\>'));
  const row = text.split('\n').filter(l => l.startsWith('| `'));
  assert.equal(row.length, 1);
  assert.equal(row[0].split(' | ').length, 4, 'a pipe in an id cannot add a table cell');
  assert.ok(!text.includes('f`1') && !text.includes('o`bl'), 'a backtick in an id cannot break out of its code span');
});

test('a mixed SIGNALS.jsonl renders only schema-valid rows, counts the rest, and keeps threshold comparisons readable', async () => {
  await gated('mandatory-fail', async dir => {
    const good = signal({ threshold: { rule: 'fk_grade <= 8 <img src=x> [x](http://evil) | z', source: 'plainlanguage.gov', confidence: 'medium' } });
    const bad = signal({ signal_id: 'sig-aaaaaaaaaaaa', family: '*evil*<img src=x onerror=1>', threshold: { rule: '*evil* <img src=x>', source: '_s_', confidence: 'high' } });
    const alsoBad = { ...signal({ signal_id: 'sig-bbbbbbbbbbbb' }), extra_field: '*evil*' };
    await writeFile(join(dir, 'SIGNALS.jsonl'), jsonl([bad, good, alsoBad]));
    const result = await run('scripts/report.mjs', ['--dir', dir]);
    assert.equal(result.code, 0, result.stderr);
    const tail = section10(await readFile(join(dir, 'REPORT.md'), 'utf8'));
    assert.match(tail, /Signals recorded: 1\./);
    assert.match(tail, /2 rows in SIGNALS\.jsonl failed schema validation and were not rendered\./);
    assert.ok(!tail.includes('evil*') && !tail.includes('onerror'), 'no field of an invalid row reaches the report');
    assert.ok(tail.includes('fk\\_grade <= 8 \\<img src=x\\>'), 'the comparison token stays, the tag is escaped');
    assert.ok(tail.includes('\\[x\\]\\(http://evil\\)'), 'the link is fully escaped');
    assert.ok(tail.includes('\\|'));
    assert.doesNotMatch(tail, /(^|[^\\])<img/m);
  });
});

test('a SIGNALS.jsonl with only invalid rows reports that no valid signal exists', async () => {
  await gated('mandatory-fail', async dir => {
    await writeFile(join(dir, 'SIGNALS.jsonl'), jsonl([signal({ family: '<img>' }), signal({ state: 'required' })]));
    await run('scripts/report.mjs', ['--dir', dir]);
    const tail = section10(await readFile(join(dir, 'REPORT.md'), 'utf8'));
    assert.match(tail, /SIGNALS\.jsonl holds no valid signals\. 2 rows in SIGNALS\.jsonl failed schema validation/);
    assert.ok(!tail.includes('<img>'));
  });
});

test('comparison tokens are restored only as exact tokens: they cannot open a link or an emphasis span', async () => {
  await gated('mandatory-fail', async dir => {
    const rows = [
      signal({ signal_id: 'sig-111111111111', signal_key: 'k.one', threshold: { rule: '[x <= y](http://evil)', source: 'practice', confidence: 'low' } }),
      signal({ signal_id: 'sig-222222222222', signal_key: 'k.two', threshold: { rule: '*a < b*', source: '_c > d_', confidence: 'low' } }),
      signal({ signal_id: 'sig-333333333333', signal_key: 'k.three', threshold: { rule: 'a<b a<=b a>b x\\ < y', source: 'practice', confidence: 'low' } })
    ];
    await writeFile(join(dir, 'SIGNALS.jsonl'), jsonl(rows));
    await run('scripts/report.mjs', ['--dir', dir]);
    const tail = section10(await readFile(join(dir, 'REPORT.md'), 'utf8'));
    assert.ok(tail.includes('\\[x <= y\\]\\(http://evil\\)'), 'the operator inside link text does not unlock the link');
    assert.ok(tail.includes('\\*a < b\\*'), 'emphasis markers stay escaped around an operator');
    assert.ok(tail.includes('\\_c > d\\_'));
    assert.ok(tail.includes('a\\<b a\\<=b a\\>b'), 'operators without surrounding spaces stay escaped');
    assert.ok(tail.includes('x\\\\ < y'), 'a backslash is escaped, then the spaced token is restored');
    assert.doesNotMatch(tail, /(^|[^\\])[[(*_]x /m);
  });
});
