import { describe, it, expect } from 'vitest'
import { parseAdminProfile, disabledNavHrefsFor, profileHiddenHrefsFor, isSectionEnabledIn, sectionOf } from './disabled-sections'

// The only-context cut, as agreed with the client: memories, the knowledge and
// code graphs, tags, conventions, clients, projects, users & roles, migration.
const ONLY_CONTEXT_KEPT = [
  '/', '/search', '/memories', '/graph', '/tags', '/conventions', '/migrations',
  '/clients', '/projects', '/code', '/users', '/roles', '/settings',
]
const ONLY_CONTEXT_HIDDEN = [
  '/usage', '/collections', '/sessions', '/tasks', '/sdd', '/harnesses', '/autonomous-agents',
  '/api-keys', '/agents', '/policies', '/webhooks', '/audit', '/backups',
]

describe('parseAdminProfile', () => {
  it('defaults_to_full_when_unset', () => {
    expect(parseAdminProfile(undefined)).toBe('full')
    expect(parseAdminProfile('')).toBe('full')
  })

  it('accepts_the_known_profiles', () => {
    expect(parseAdminProfile('full')).toBe('full')
    expect(parseAdminProfile('only-context')).toBe('only-context')
  })

  it('rejects_an_unknown_profile_loudly', () => {
    // A typo must never fall back to the full panel (vite.config.ts fails the build first).
    expect(() => parseAdminProfile('only_context')).toThrow(/Unknown VITE_ADMIN_PROFILE/)
  })
})

describe('disabledNavHrefsFor', () => {
  it('full_profile_keeps_only_the_temporary_flags', () => {
    const disabled = disabledNavHrefsFor('full')
    expect([...disabled].sort()).toEqual(['/agents', '/api-keys', '/policies', '/sessions', '/webhooks'])
  })

  it('only_context_hides_everything_that_is_not_context', () => {
    const disabled = disabledNavHrefsFor('only-context')
    for (const href of ONLY_CONTEXT_HIDDEN) expect(disabled.has(href), href).toBe(true)
    for (const href of ONLY_CONTEXT_KEPT) expect(disabled.has(href), href).toBe(false)
  })

  it('only_context_is_a_superset_of_full', () => {
    const full = disabledNavHrefsFor('full')
    const onlyContext = disabledNavHrefsFor('only-context')
    for (const href of full) expect(onlyContext.has(href), href).toBe(true)
  })
})

describe('isSectionEnabledIn', () => {
  const disabled = disabledNavHrefsFor('only-context')

  it('matches_on_the_first_path_segment', () => {
    expect(isSectionEnabledIn('/sdd?change=x', disabled)).toBe(false)
    expect(isSectionEnabledIn('/sdd/', disabled)).toBe(false)
    expect(isSectionEnabledIn('/tasks#top', disabled)).toBe(false)
  })

  it('keeps_enabled_sections_and_the_dashboard', () => {
    expect(isSectionEnabledIn('/memories?tab=collections', disabled)).toBe(true)
    expect(isSectionEnabledIn('/', disabled)).toBe(true)
    expect(isSectionEnabledIn('/migrations', disabled)).toBe(true)
  })
})

describe('profileHiddenHrefsFor — the layer non-link surfaces must use', () => {
  it('full_profile_hides_nothing_so_facets_permissions_and_tabs_stay', () => {
    // Regression guard: keying a search facet or a role permission on the
    // TEMPORARY flags removed Policies from search and five permissions from
    // the role editor in the default panel. The profile layer must be empty.
    expect(profileHiddenHrefsFor('full').size).toBe(0)
    const kept = profileHiddenHrefsFor('full')
    for (const href of ['/policies', '/sessions', '/api-keys', '/webhooks']) {
      expect(isSectionEnabledIn(href, kept), href).toBe(true)
    }
  })

  it('only_context_hides_the_same_sections_as_the_nav', () => {
    const hidden = profileHiddenHrefsFor('only-context')
    for (const href of ONLY_CONTEXT_HIDDEN) expect(hidden.has(href), href).toBe(true)
    for (const href of ONLY_CONTEXT_KEPT) expect(hidden.has(href), href).toBe(false)
  })
})

describe('sectionOf', () => {
  it('normalises_query_hash_trailing_slash_and_root', () => {
    expect(sectionOf('/sdd?change=x')).toBe('/sdd')
    expect(sectionOf('/tasks#top')).toBe('/tasks')
    expect(sectionOf('/memories/')).toBe('/memories')
    expect(sectionOf('/')).toBe('/')
  })
})
