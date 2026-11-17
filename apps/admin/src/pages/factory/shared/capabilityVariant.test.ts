import { describe, expect, it } from 'vitest'
import { capabilityVariant } from './format'

describe('capabilityVariant', () => {
  it('colours capabilities by what they can do', () => {
    expect(capabilityVariant('repository:read')).toBe('info')
    expect(capabilityVariant('web:search')).toBe('info')
    expect(capabilityVariant('finding:write')).toBe('purple')
    expect(capabilityVariant('github:review')).toBe('warning')
    expect(capabilityVariant('scan:active')).toBe('warning')
  })
})
