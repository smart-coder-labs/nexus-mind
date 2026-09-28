import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { products, selectProducts, renderProducts } from '../src/app.js';

test('search is case-insensitive and accent-insensitive', () => {
  assert.deepEqual(selectProducts(products, { query: 'MECANICO' }).map((p) => p.id), ['p2']);
});
test('category filter combines with search', () => {
  assert.deepEqual(selectProducts(products, { query: 'mini', category: 'Audio' }).map((p) => p.id), ['p3']);
});
test('price ordering does not mutate input', () => {
  const before = products.map((p) => p.id);
  assert.deepEqual(selectProducts(products, { sort: 'price-asc' }).map((p) => p.id), ['p4', 'p3', 'p1', 'p2']);
  assert.deepEqual(products.map((p) => p.id), before);
});
test('empty matches are represented in the rendered output', () => {
  assert.match(renderProducts([]), /sin resultados|no hay resultados|no results/i);
});
test('product strings are escaped before HTML insertion', () => {
  const html = renderProducts([{ id: 'x', name: '<script>alert(1)</script>', category: 'X', price: 1 }]);
  assert.doesNotMatch(html, /<script>/);
  assert.match(html, /&lt;script&gt;/);
});
test('page has semantic language and a main landmark', () => {
  const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
  assert.match(html, /<html[^>]+lang="es"/i);
  assert.match(html, /<main\b/i);
});
