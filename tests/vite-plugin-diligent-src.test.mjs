/**
 * Tests for plugins/vite-plugin-diligent-src: attribute placement, skipped
 * tags, line numbers, and the options that gate it.
 */
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { ROOT } from './helpers.mjs';
import { diligentSrc, stampSource, ATTRIBUTE } from '../plugins/vite-plugin-diligent-src/index.mjs';

const ROOT_DIR = '/proj';

function run(code, file, options = {}) {
  const plugin = diligentSrc({ enabled: true, root: ROOT_DIR, ...options });
  const out = plugin.transform(code, `${ROOT_DIR}/${file}`);
  return out === null ? null : out.code;
}

function stamps(code) {
  return [...code.matchAll(/ data-diligent-src="([^"]*)"/g)].map(m => m[1]);
}

describe('vite-plugin-diligent-src', () => {
  it('stamps native elements in JSX with the file and line', () => {
    const code = [
      'export function A() {',
      '  return (',
      '    <main>',
      '      <h1>Title</h1>',
      '      <img src="/a.png" alt="" />',
      '    </main>',
      '  );',
      '}'
    ].join('\n');
    const out = run(code, 'src/A.tsx');
    assert.deepEqual(stamps(out), ['src/A.tsx:3', 'src/A.tsx:4', 'src/A.tsx:5']);
    assert.match(out, /<main data-diligent-src="src\/A\.tsx:3">/);
    assert.match(out, /<img data-diligent-src="src\/A\.tsx:5" src="\/a\.png" alt="" \/>/);
  });

  it('skips components, member components and fragments', () => {
    const code = 'const x = (<><Card><Ui.Box /><span>t</span></Card></>);';
    const out = run(code, 'x.jsx');
    assert.equal(stamps(out).length, 1);
    assert.match(out, /<span data-diligent-src="x\.jsx:1">/);
    assert.ok(!out.includes('<Card data-'));
    assert.ok(!out.includes('<Ui.Box data-'));
  });

  it('stamps elements inside expressions', () => {
    const code = 'const a = (<ul>{items.map(i => <li key={i}>{i}</li>)}{ok && <b>x</b>}</ul>);';
    const out = run(code, 'l.jsx');
    assert.equal(stamps(out).length, 3);
  });

  it('does not mistake TypeScript generics or comparisons for elements', () => {
    const code = [
      'const [s, setS] = useState<string>("a");',
      'for (let i = 0; i<n; i++) {}',
      'if (a < b && c > d) {}',
      'const el = <div>{count < 3 ? <i>few</i> : null}</div>;'
    ].join('\n');
    const out = run(code, 'g.tsx');
    assert.deepEqual(stamps(out), ['g.tsx:4', 'g.tsx:4']);
    assert.ok(out.includes('useState<string>("a")'));
  });

  it('ignores tags in strings, comments and template literals', () => {
    const code = 'const a = "<div>"; // <p>\n/* <b> */ const t = `<i>${x}</i>`; const e = <u>ok</u>;';
    const out = run(code, 's.jsx');
    assert.equal(stamps(out).length, 1);
    assert.match(out, /<u data-diligent-src="s\.jsx:2">/);
  });

  it('skips a tag that already has the attribute, so a second pass changes nothing', () => {
    const once = run('<p>hi</p>', 'p.jsx');
    assert.equal(stamps(once).length, 1);
    assert.equal(run(once, 'p.jsx'), null);
    const manual = run('<p data-diligent-src="manual:1">hi</p>', 'p.jsx');
    assert.equal(manual, null);
  });

  it('stamps only the template of a Vue file', () => {
    const code = [
      '<script setup>',
      'const html = "<div>not a tag</div>"',
      '</script>',
      '',
      '<template>',
      '  <main>',
      '    <img :alt="t" src="/x.png">',
      '    <Fancy v-if="ok" />',
      '    <p>{{ a < b }}</p>',
      '  </main>',
      '</template>',
      '',
      '<style>a > b { color: red }</style>'
    ].join('\n');
    const out = run(code, 'V.vue');
    assert.deepEqual(stamps(out), ['V.vue:6', 'V.vue:7', 'V.vue:9']);
    assert.ok(out.includes('"<div>not a tag</div>"'));
    assert.ok(out.includes('<template>'), 'template tag itself is not stamped');
    assert.ok(!out.includes('<Fancy data-'));
  });

  it('stamps Svelte markup and leaves blocks, script and style alone', () => {
    const code = [
      '<script>',
      '  let items = [];',
      '</script>',
      '',
      '{#each items as it}',
      '  <li><img src={it.u} alt={it.a}></li>',
      '{/each}',
      '<svelte:head><title>x</title></svelte:head>',
      '<Widget />',
      '<style>li { color: red }</style>'
    ].join('\n');
    const out = run(code, 'S.svelte');
    assert.deepEqual(stamps(out), ['S.svelte:6', 'S.svelte:6', 'S.svelte:8']);
    assert.ok(!out.includes('<svelte:head data-'));
    assert.ok(!out.includes('<Widget data-'));
  });

  it('skips Astro frontmatter and stamps the markup', () => {
    const code = [
      '---',
      'const html = "<div>x</div>";',
      '---',
      '<Layout>',
      '  <h1>Hi</h1>',
      '  {list.map((p) => <img src={p.u} alt={p.a} />)}',
      '</Layout>'
    ].join('\n');
    const out = run(code, 'pages/index.astro');
    assert.deepEqual(stamps(out), ['pages/index.astro:5', 'pages/index.astro:6']);
    assert.ok(out.includes('"<div>x</div>"'));
  });

  it('counts lines with CRLF and lone CR line endings', () => {
    const crlf = run('<a>\r\n<b>x</b>\r\n</a>', 'c.jsx');
    assert.deepEqual(stamps(crlf), ['c.jsx:1', 'c.jsx:2']);
    const cr = run('<a>\r<b>x</b>\r</a>', 'c.jsx');
    assert.deepEqual(stamps(cr), ['c.jsx:1', 'c.jsx:2']);
  });

  it('writes paths relative to root with forward slashes', () => {
    const out = run('<p>x</p>', 'deep/er/P.jsx');
    assert.deepEqual(stamps(out), ['deep/er/P.jsx:1']);
    const custom = run('<p>x</p>', 'deep/er/P.jsx', { root: '/proj/deep' });
    assert.deepEqual(stamps(custom), ['er/P.jsx:1']);
  });

  it('is off unless enabled, and defaults to development mode', () => {
    const off = diligentSrc({ enabled: false, root: ROOT_DIR });
    assert.equal(off.transform('<p>x</p>', `${ROOT_DIR}/a.jsx`), null);

    const dev = diligentSrc();
    dev.configResolved({ mode: 'development', root: ROOT_DIR });
    assert.ok(dev.transform('<p>x</p>', `${ROOT_DIR}/a.jsx`));

    const prod = diligentSrc();
    prod.configResolved({ mode: 'production', root: ROOT_DIR });
    assert.equal(prod.transform('<p>x</p>', `${ROOT_DIR}/a.jsx`), null);

    const before = diligentSrc();
    assert.equal(before.transform('<p>x</p>', `${ROOT_DIR}/a.jsx`), null, 'off until configured');
  });

  it('ignores other file types, compiled Vue requests and node_modules', () => {
    const plugin = diligentSrc({ enabled: true, root: ROOT_DIR });
    assert.equal(plugin.transform('<p>x</p>', `${ROOT_DIR}/a.js`), null);
    assert.equal(plugin.transform('<p>x</p>', `${ROOT_DIR}/a.html`), null);
    assert.equal(plugin.transform('<p>x</p>', `${ROOT_DIR}/A.vue?vue&type=template&lang.js`), null);
    assert.equal(plugin.transform('<p>x</p>', `${ROOT_DIR}/node_modules/p/a.jsx`), null);
  });

  it('runs before other plugins', () => {
    assert.equal(diligentSrc().enforce, 'pre');
  });

  it('survives unterminated and hostile input without throwing', () => {
    const inputs = ['<', '<a', '<a b="', '<a b={', '{', '`${', '<!--', '---\n<a', '<script>', '{{', '{#', '<>', '</>'];
    for (const flavor of ['jsx', 'vue', 'svelte', 'astro']) {
      for (const input of inputs) {
        stampSource(input, flavor, 'f');
      }
      stampSource('<'.repeat(5000) + '{'.repeat(5000), flavor, 'f');
    }
  });

  it('is private and not part of the packed package', async () => {
    const pkg = JSON.parse(await readFile(join(ROOT, 'plugins', 'vite-plugin-diligent-src', 'package.json'), 'utf8'));
    assert.equal(pkg.private, true);
    const root = JSON.parse(await readFile(join(ROOT, 'package.json'), 'utf8'));
    assert.ok(!root.files.some(f => f.includes('plugins')), 'package.json files must not include the plugin');
    const manifest = JSON.parse(await readFile(join(ROOT, 'runtime-manifest.json'), 'utf8'));
    assert.ok(!JSON.stringify(manifest).includes('vite-plugin'), 'runtime-manifest must not include the plugin');
  });

  it('exports the attribute name', () => {
    assert.equal(ATTRIBUTE, 'data-diligent-src');
  });
});
