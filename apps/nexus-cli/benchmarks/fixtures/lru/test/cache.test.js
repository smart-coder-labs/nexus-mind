import test from 'node:test';
import assert from 'node:assert/strict';
import { LruCache } from '../src/cache.js';

test('least recently used entry is evicted', () => {
  const cache = new LruCache(2);
  cache.set('a', 1); cache.set('b', 2); cache.get('a'); cache.set('c', 3);
  assert.equal(cache.get('a'), 1);
  assert.equal(cache.get('b'), undefined);
  assert.equal(cache.get('c'), 3);
});

test('updating an entry refreshes recency', () => {
  const cache = new LruCache(2);
  cache.set('a', 1); cache.set('b', 2); cache.set('a', 4); cache.set('c', 3);
  assert.equal(cache.get('a'), 4);
  assert.equal(cache.get('b'), undefined);
});

test('TTL boundary and zero TTL use an injected clock', () => {
  let now = 100;
  const cache = new LruCache(2, () => now);
  cache.set('a', 1, 10); cache.set('b', 2, 0);
  assert.equal(cache.get('b'), undefined);
  now = 109; assert.equal(cache.get('a'), 1);
  now = 110; assert.equal(cache.get('a'), undefined);
  assert.equal(cache.size, 0);
});

test('expired entries do not evict live entries', () => {
  let now = 0;
  const cache = new LruCache(2, () => now);
  cache.set('a', 1, 1); cache.set('b', 2);
  now = 2; cache.set('c', 3);
  assert.equal(cache.get('b'), 2);
  assert.equal(cache.get('c'), 3);
  assert.equal(cache.size, 2);
});

test('capacity must be a positive integer', () => {
  for (const capacity of [0, -1, 1.5, NaN]) {
    assert.throws(() => new LruCache(capacity), RangeError);
  }
});
