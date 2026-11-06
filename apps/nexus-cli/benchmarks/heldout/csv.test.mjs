import test from 'node:test';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const { parseCsv } = await import(pathToFileURL(process.env.BENCH_IMPL).href);

test('single empty record is distinct from empty input', () => {
  assert.deepEqual(parseCsv('\n'), [['']]);
});

test('quoted CRLF remains in field data', () => {
  assert.deepEqual(parseCsv('"a\r\nb",c'), [['a\r\nb', 'c']]);
});

test('trailing comma preserves an empty cell', () => {
  assert.deepEqual(parseCsv('a,b,'), [['a', 'b', '']]);
});

test('multiple escaped quotes and subsequent rows', () => {
  assert.deepEqual(parseCsv('"a""b""c",d\r\nx,y'), [['a"b"c', 'd'], ['x', 'y']]);
});

test('invalid quote after a closed field is rejected', () => {
  assert.throws(() => parseCsv('"a" x'), SyntaxError);
});
