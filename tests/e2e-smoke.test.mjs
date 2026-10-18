import { test } from 'node:test';
import assert from 'node:assert/strict';
import { smokeRoutes } from '../scripts/e2e-smoke.mjs';
import { run } from './helpers.mjs';
import { startStaticServer } from './support/static-server.mjs';

test('with --allow-missing-chrome and no binary the smoke reports SKIPPED and exits 0', async () => {
  // The unit-test job never drives a real browser: a missing prerequisite is pinned here, so a machine that
  // happens to have Chrome or a built target/ cannot turn this into a full run. The real run is
  // `npm run release:gate` and the CI smoke job.
  const result = await run('scripts/e2e-smoke.mjs', ['--allow-missing-chrome'], { env: { DILIGENT_RS_BIN: '/nonexistent/diligent-rs' } });
  assert.equal(result.code, 0, `${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout, /e2e smoke: SKIPPED \(diligent-rs binary \(\/nonexistent\/diligent-rs\)/);
});

test('without the escape hatch a missing prerequisite fails loudly', async () => {
  const result = await run('scripts/e2e-smoke.mjs', [], { env: { DILIGENT_RS_BIN: '/nonexistent/diligent-rs', DILIGENT_CHROME: '/nonexistent/chrome' } });
  assert.equal(result.code, 1);
  assert.match(result.stdout, /FAILED\s+prerequisites: diligent-rs binary .* and Chrome .* not found/);
});

test('the smoke site has the failing-contrast page, a link to it, and no leftover example.gov URLs', () => {
  const routes = smokeRoutes();
  assert.ok(routes['/contrast-fail'].body.includes('<html'));
  assert.ok(routes['/'].body.includes('href="/contrast-fail"'));
  assert.ok(routes['/'].body.includes('color:#aaaaaa'));
  for (const [path, route] of Object.entries(routes)) assert.doesNotMatch(route.body, /https:\/\/example\.gov/, path);
  assert.ok(routes['/sitemap.xml'].body.includes('${ORIGIN}/about'));
});

test('the static server fills in its own origin and answers 404 for unknown paths', async () => {
  const server = await startStaticServer({ '/': { body: '<a href="${ORIGIN}/x">x</a>' }, '/robots.txt': { headers: { 'content-type': 'text/plain' }, body: 'ok' } });
  try {
    const home = await fetch(`${server.origin}/`);
    assert.equal(await home.text(), `<a href="${server.origin}/x">x</a>`);
    assert.equal((await fetch(`${server.origin}/robots.txt`)).headers.get('content-type'), 'text/plain');
    assert.equal((await fetch(`${server.origin}/missing`)).status, 404);
    assert.match(server.origin, /^http:\/\/127\.0\.0\.1:\d+$/);
  } finally {
    await server.close();
  }
});
