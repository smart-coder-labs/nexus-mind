import test from 'node:test';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const { createServer } = await import(pathToFileURL(process.env.BENCH_IMPL).href);
async function withServer(fn) {
  const server = createServer();
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try { await fn(`http://127.0.0.1:${server.address().port}`); }
  finally { await new Promise((resolve) => server.close(resolve)); }
}
const post = (base, key, body) => fetch(`${base}/orders`, { method: 'POST', headers: { 'content-type': 'application/json', 'idempotency-key': key }, body: JSON.stringify(body) });

test('reused idempotency key with different payload is conflict', () => withServer(async (base) => {
  assert.equal((await post(base, 'same', { sku: 'a', quantity: 1 })).status, 201);
  const response = await post(base, 'same', { sku: 'b', quantity: 1 });
  assert.equal(response.status, 409); assert.ok((await response.json()).error);
}));
test('quantity must be a positive integer', () => withServer(async (base) => {
  for (const quantity of [-1, 0, 1.5, '2']) {
    assert.equal((await post(base, `k-${quantity}`, { sku: 'a', quantity })).status, 400);
  }
}));
test('invalid content type is rejected with JSON', () => withServer(async (base) => {
  const response = await fetch(`${base}/orders`, { method: 'POST', headers: { 'content-type': 'text/plain' }, body: 'hello' });
  assert.equal(response.status, 415); assert.ok((await response.json()).error);
}));
test('request bodies over 64 KiB are rejected', () => withServer(async (base) => {
  const response = await post(base, 'large', { sku: 'x'.repeat(70_000), quantity: 1 });
  assert.equal(response.status, 413);
}));
