import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import path from 'node:path';

const file = process.env.BENCH_IMPL;
const { selectProducts, renderProducts } = await import(pathToFileURL(file).href);
const root = path.dirname(path.dirname(file));

test('empty query and default category preserve all products', () => {
  const items = [{ id: 'b', name: 'Beta', category: 'B', price: 2 }, { id: 'a', name: 'Alpha', category: 'A', price: 1 }];
  assert.equal(selectProducts(items, { query: ' ', category: 'all' }).length, 2);
});
test('descending price ordering is independent of source order', () => {
  const items = [{ id: 'a', name: 'A', category: 'A', price: 2 }, { id: 'b', name: 'B', category: 'B', price: 8 }];
  assert.deepEqual(selectProducts(items, { sort: 'price-desc' }).map((p) => p.id), ['b', 'a']);
  assert.deepEqual(items.map((p) => p.id), ['a', 'b']);
});
test('descending name ordering follows the requested sort', () => {
  const items = [{ id: 'a', name: 'Alpha', category: 'A', price: 1 }, { id: 'b', name: 'Beta', category: 'B', price: 2 }];
  assert.deepEqual(selectProducts(items, { sort: 'name-desc' }).map((p) => p.id), ['b', 'a']);
});
test('HTML-sensitive product names and categories are escaped', () => {
  const html = renderProducts([{ id: 'x', name: '<img src=x>', category: '<b>bad</b>', price: 1 }]);
  assert.doesNotMatch(html, /<img\b|<b>/);
});
test('page exposes labelled controls and responsive CSS', () => {
  const html = readFileSync(path.join(root, 'index.html'), 'utf8');
  const js = readFileSync(file, 'utf8');
  const css = readFileSync(path.join(root, 'styles.css'), 'utf8');
  assert.match(html, /<main\b/i);
  assert.match(html + js, /<label\b|aria-label=/i);
  assert.match(css, /@media\s*\(/i);
});
