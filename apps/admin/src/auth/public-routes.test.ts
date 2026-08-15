import { describe, expect, it } from 'vitest'
import { isPublicRoute } from './public-routes'

describe('isPublicRoute', () => {
  it('matches the public routes exactly', () => {
    expect(isPublicRoute('/login')).toBe(true)
    expect(isPublicRoute('/set-password')).toBe(true)
  })

  it('matches nested paths under a public route (e.g. a token segment)', () => {
    expect(isPublicRoute('/set-password/abc123')).toBe(true)
  })

  it('does not match protected routes', () => {
    expect(isPublicRoute('/')).toBe(false)
    expect(isPublicRoute('/memories')).toBe(false)
    expect(isPublicRoute('/settings')).toBe(false)
  })

  it('does not match a route that merely shares a prefix', () => {
    expect(isPublicRoute('/login-history')).toBe(false)
  })

  it('treats a missing pathname as non-public so 401s still eject', () => {
    expect(isPublicRoute(undefined)).toBe(false)
    expect(isPublicRoute('')).toBe(false)
  })
})
