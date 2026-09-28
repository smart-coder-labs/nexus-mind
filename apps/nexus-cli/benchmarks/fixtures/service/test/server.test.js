import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from '../src/server.js';

async function withServer(fn) {
  const server = createServer();
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try { return await fn(`http://127.0.0.1:${server.address().port}`); }
  finally { await new Promise((resolve) => server.close(resolve)); }
}

test('health endpoint returns JSON without exposing orders', () => withServer(async (base) => {
  const response = await fetch(`${base}/health`);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { status: 'ok' });
}));
test('create and read an order', () => withServer(async (base) => {
  const created = await fetch(`${base}/orders`, { method: 'POST', headers: { 'content-type': 'application/json', 'idempotency-key': 'a' }, body: JSON.stringify({ sku: 'book', quantity: 2 }) });
  assert.equal(created.status, 201);
  const body = await created.json();
  assert.equal(body.sku, 'book'); assert.equal(body.quantity, 2); assert.ok(body.id);
  const read = await fetch(`${base}/orders/${body.id}`);
  assert.equal(read.status, 200); assert.deepEqual(await read.json(), body);
}));
test('same idempotency key returns the same order', () => withServer(async (base) => {
  const options = { method: 'POST', headers: { 'content-type': 'application/json', 'idempotency-key': 'same' }, body: JSON.stringify({ sku: 'pen', quantity: 1 }) };
  const first = await fetch(`${base}/orders`, options); const second = await fetch(`${base}/orders`, options);
  assert.equal(first.status, 201); assert.equal(second.status, 200);
  assert.deepEqual(await second.json(), await first.json());
}));
test('invalid payload and malformed JSON are client errors', () => withServer(async (base) => {
  for (const body of ['{', JSON.stringify({ sku: '', quantity: 0 })]) {
    const response = await fetch(`${base}/orders`, { method: 'POST', headers: { 'content-type': 'application/json' }, body });
    assert.equal(response.status, 400); assert.match(response.headers.get('content-type'), /application\/json/);
  }
}));
test('unknown route and order return JSON 404', () => withServer(async (base) => {
  for (const path of ['/missing', '/orders/nope']) {
    const response = await fetch(base + path); assert.equal(response.status, 404);
    assert.ok((await response.json()).error);
  }
}));
