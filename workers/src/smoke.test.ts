import test from 'node:test'
import assert from 'node:assert/strict'

interface Thing {
  readonly name: string
}

test('type stripping works', () => {
  const thing: Thing = { name: 'ok' }
  assert.equal(thing.name, 'ok')
})
