import test from 'node:test';
import assert from 'node:assert/strict';
import { parseCsv } from '../src/csv.js';

test('plain rows and empty cells', () => {
  assert.deepEqual(parseCsv('a,b,\n,c,d'), [['a', 'b', ''], ['', 'c', 'd']]);
});

test('quoted commas, escaped quotes and embedded newlines', () => {
  assert.deepEqual(parseCsv('name,note\r\n"Ada, A.","said ""hello""\nand left"'), [
    ['name', 'note'], ['Ada, A.', 'said "hello"\nand left'],
  ]);
});

test('BOM, CRLF and final record terminator', () => {
  assert.deepEqual(parseCsv('\uFEFFa,b\r\nc,d\r\n'), [['a', 'b'], ['c', 'd']]);
});

test('empty input and empty quoted field', () => {
  assert.deepEqual(parseCsv(''), []);
  assert.deepEqual(parseCsv('"",x'), [['', 'x']]);
});

test('reject malformed quoting', () => {
  for (const input of ['"unclosed', 'a"b,c', '"a"b,c']) {
    assert.throws(() => parseCsv(input), SyntaxError, input);
  }
});
