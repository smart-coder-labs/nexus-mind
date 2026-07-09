import { describe, expect, it } from 'vitest'
import { sha256Hex } from './sha256'

const enc = (s: string) => new TextEncoder().encode(s)

describe('sha256Hex (pure-JS fallback)', () => {
  it('matches known FIPS 180-4 vectors', () => {
    expect(sha256Hex(enc(''))).toBe('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')
    expect(sha256Hex(enc('abc'))).toBe('ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad')
    expect(sha256Hex(enc('abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq'))).toBe(
      '248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1',
    )
  })

  it('agrees with crypto.subtle across content lengths and UTF-8', async () => {
    const samples = ['a', 'x'.repeat(55), 'y'.repeat(56), 'z'.repeat(64), 'w'.repeat(120), '{"name":"Café ☕"}\n#!/bin/sh']
    for (const sample of samples) {
      const digest = await crypto.subtle.digest('SHA-256', enc(sample))
      const expected = Array.from(new Uint8Array(digest)).map(b => b.toString(16).padStart(2, '0')).join('')
      expect(sha256Hex(enc(sample))).toBe(expected)
    }
  })
})
