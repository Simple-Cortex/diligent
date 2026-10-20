#!/usr/bin/env node
/** Dev-only: serve the end-to-end smoke site, print `origin=<url>` once, and stay up until SIGTERM or SIGINT. Not packaged. */
import { smokeRoutes } from '../../scripts/e2e-smoke.mjs';
import { startStaticServer } from './static-server.mjs';

const server = await startStaticServer(smokeRoutes());
console.log(`origin=${server.origin}`);
const stop = async () => { await server.close(); process.exit(0); };
process.on('SIGTERM', stop);
process.on('SIGINT', stop);
