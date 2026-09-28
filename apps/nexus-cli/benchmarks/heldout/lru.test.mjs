import test from 'node:test';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const { LruCache } = await import(pathToFileURL(process.env.BENCH_IMPL).href);

test('reads refresh recency repeatedly', () => {
  const cache = new LruCache(3);
  cache.set('a', 1); cache.set('b', 2); cache.set('c', 3);
  cache.get('a'); cache.get('b'); cache.set('d', 4);
  assert.equal(cache.get('c'), undefined);
  assert.equal(cache.get('a'), 1);
  assert.equal(cache.get('b'), 2);
});

test('missing key does not change size', () => {
  const cache = new LruCache(1);
  assert.equal(cache.get('none'), undefined);
  assert.equal(cache.size, 0);
});

test('expired most-recent entry is purged', () => {
  let now = 0;
  const cache = new LruCache(2, () => now);
  cache.set('a', 1); cache.set('b', 2, 1);
  now = 2; cache.set('c', 3);
  assert.equal(cache.get('a'), 1);
  assert.equal(cache.get('b'), undefined);
  assert.equal(cache.get('c'), 3);
});

test('updating TTL changes the deadline', () => {
  let now = 0;
  const cache = new LruCache(1, () => now);
  cache.set('a', 1, 5);
  now = 4; cache.set('a', 2, 10);
  now = 5; assert.equal(cache.get('a'), 2);
  now = 14; assert.equal(cache.get('a'), undefined);
});
