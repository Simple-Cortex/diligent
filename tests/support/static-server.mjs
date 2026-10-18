/** A tiny static server for end-to-end and orchestrator tests. Serves an in-memory route map on 127.0.0.1. */
import { createServer } from 'node:http';

/**
 * routes: { '/path': { status?, headers?, body } }. A string body is sent as UTF-8.
 * `${ORIGIN}` inside a body is replaced with the server origin when the server starts.
 * Resolves to { origin, port, close() }.
 */
export function startStaticServer(routes, { host = '127.0.0.1' } = {}) {
  let origin = '';
  const server = createServer((request, response) => {
    const path = new URL(request.url, 'http://local').pathname;
    const route = routes[path];
    if (!route) { response.writeHead(404, { 'content-type': 'text/plain' }); response.end('not found'); return; }
    response.writeHead(route.status ?? 200, { 'content-type': 'text/html; charset=utf-8', ...(route.headers || {}) });
    response.end(String(route.body ?? '').replaceAll('${ORIGIN}', origin));
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, host, () => {
      const { port } = server.address();
      origin = `http://${host}:${port}`;
      resolve({ origin, port, close: () => new Promise(done => { server.closeAllConnections?.(); server.close(() => done()); }) });
    });
  });
}
