import { createServer as createHttpServer } from 'node:http';

export function createServer() {
  return createHttpServer((_req, res) => {
    res.writeHead(501, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ error: 'not implemented' }));
  });
}
