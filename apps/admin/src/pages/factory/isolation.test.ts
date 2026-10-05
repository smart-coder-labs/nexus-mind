import { describe, expect, it } from 'vitest'
import { isolationConfig, isolationFromConfig, localSupported, parseAllowedHosts, sandboxSupported } from './isolation'

describe('sandboxSupported', () => {
  it('matches the templates the server can sandbox, with the Claude executor only', () => {
    expect(sandboxSupported('qa', 'claude')).toBe(true)
    expect(sandboxSupported('github_issue_resolver', 'claude')).toBe(true)
    expect(sandboxSupported('security_dast', 'claude')).toBe(true)
    expect(sandboxSupported('lead_generation', 'claude')).toBe(false)
    expect(sandboxSupported('qa', 'nexus')).toBe(false)
  })

  it('runs Codex only in the sandbox and only for code templates', () => {
    expect(sandboxSupported('github_issue_resolver', 'codex')).toBe(true)
    expect(sandboxSupported('github_pr_reviewer', 'codex')).toBe(true)
    expect(sandboxSupported('qa', 'codex')).toBe(false)
    expect(localSupported('codex')).toBe(false)
    expect(localSupported('claude')).toBe(true)
  })
})

describe('parseAllowedHosts', () => {
  it('accepts lowercase DNS names separated by commas or lines', () => {
    expect(parseAllowedHosts('cdn.acme.test, Auth.Example.com\n\nstripe.com')).toEqual({
      hosts: ['cdn.acme.test', 'auth.example.com', 'stripe.com'],
    })
    expect(parseAllowedHosts('   ')).toEqual({ hosts: [] })
  })

  it('names the first entry the server would refuse', () => {
    expect(parseAllowedHosts('cdn.acme.test, *.acme.test')).toEqual({ error: 'Not a host name: *.acme.test' })
    expect(parseAllowedHosts('10.0.0.1')).toEqual({ error: 'Not a host name: 10.0.0.1' })
    expect(parseAllowedHosts('localhost')).toEqual({ error: 'Not a host name: localhost' })
    expect(parseAllowedHosts('https://cdn.acme.test')).toEqual({ error: 'Not a host name: https://cdn.acme.test' })
    const many = Array.from({ length: 17 }, (_, i) => `h${i}.acme.test`).join(',')
    expect(parseAllowedHosts(many)).toEqual({ error: 'At most 16 hosts' })
  })
})

describe('isolationConfig', () => {
  it('leaves the choice to the server default unless one is picked', () => {
    expect(isolationConfig({ isolation: 'default', allowedHosts: '' }, 'qa')).toEqual({})
    expect(isolationConfig({ isolation: 'local', allowedHosts: '' }, 'qa')).toEqual({ isolation: 'local' })
  })

  it('sends allowed hosts only for browser-driven templates', () => {
    expect(isolationConfig({ isolation: 'sandbox', allowedHosts: 'cdn.acme.test' }, 'judge')).toEqual({
      isolation: 'sandbox',
      sandbox_allowed_hosts: ['cdn.acme.test'],
    })
    expect(isolationConfig({ isolation: 'sandbox', allowedHosts: 'cdn.acme.test' }, 'github_pr_reviewer')).toEqual({
      isolation: 'sandbox',
    })
  })
})

describe('isolationFromConfig', () => {
  it('round-trips a saved agent', () => {
    expect(isolationFromConfig({ isolation: 'sandbox', sandbox_allowed_hosts: ['a.test', 'b.test'] })).toEqual({
      isolation: 'sandbox',
      allowedHosts: 'a.test, b.test',
    })
    expect(isolationFromConfig({})).toEqual({ isolation: 'default', allowedHosts: '' })
    expect(isolationFromConfig({ isolation: 'weird' })).toEqual({ isolation: 'default', allowedHosts: '' })
  })
})
