import { describe, expect, it } from 'vitest'
import { allowsPreviewRequest, liveApiProxy, localSessionCookie } from '../../dev/live-api-proxy'

describe('live API read-only preview boundary', () => {
  it('permits authenticated reads and explicit authentication/search endpoints', () => {
    expect(allowsPreviewRequest('GET', '/v1/admin/stats')).toBe(true)
    expect(allowsPreviewRequest('POST', '/v1/admin/auth/login')).toBe(true)
    expect(allowsPreviewRequest('POST', '/v1/memory/search?limit=5')).toBe(true)
    expect(allowsPreviewRequest('POST', '/v1/code/locate')).toBe(true)
  })
  it('denies mutations and routes that merely resemble an allowed path', () => {
    for (const [method, path] of [['POST','/v1/projects'],['PATCH','/v1/users/123'],['DELETE','/v1/memory/123'],['POST','/v1/code/search/delete'],['POST','/v1/admin/auth/reset-key']]) {
      expect(allowsPreviewRequest(method, path)).toBe(false)
    }
  })
  it('requires TLS on the upstream connection', () => {
    expect(() => liveApiProxy('http://api.nexusmind.smartcoderlabs.com')).toThrow('HTTPS')
    expect(liveApiProxy('https://api.nexusmind.smartcoderlabs.com').proxy.secure).toBe(true)
  })
  it('adapts only the loopback session cookie, preserving HttpOnly, SameSite and expiry', () => {
    expect(localSessionCookie('nexusmind_session=example; Path=/; HttpOnly; Secure; SameSite=Lax; Domain=api.example.test; Max-Age=3600')).toBe('nexusmind_session=example; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600')
    expect(localSessionCookie('other=example; Secure; HttpOnly')).toBe('other=example; Secure; HttpOnly')
  })
})
