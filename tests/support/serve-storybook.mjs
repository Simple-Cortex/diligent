#!/usr/bin/env node
/** Dev-only: serve the Storybook-shaped fixture, print `origin=<url>` once, and stay up until SIGTERM or SIGINT. Not packaged. */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { startStaticServer } from './static-server.mjs';

const FIXTURE = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'fixtures', 'storybook');

/** Routes for startStaticServer: index.json and iframe.html. The static server ignores the query string, as Storybook's does. */
export function storybookRoutes() {
  return {
    '/index.json': { headers: { 'content-type': 'application/json' }, body: readFileSync(join(FIXTURE, 'index.json'), 'utf8') },
    '/iframe.html': { body: readFileSync(join(FIXTURE, 'iframe.html'), 'utf8') }
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const server = await startStaticServer(storybookRoutes());
  console.log(`origin=${server.origin}`);
  const stop = async () => { await server.close(); process.exit(0); };
  process.on('SIGTERM', stop);
  process.on('SIGINT', stop);
}
