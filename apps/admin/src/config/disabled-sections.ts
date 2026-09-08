// Which sections of the admin exist in THIS build.
//
// Two layers, both resolved at build time:
//
// 1. The admin PROFILE (`VITE_ADMIN_PROFILE`), a per-deployment cut of the
//    product. `full` is the default. `only-context` is the "company brain" cut
//    shipped to the u2s box: memories, the knowledge and code graphs, tags,
//    conventions, clients, projects, users & roles, and the migration page.
//    Everything else is hidden from navigation AND blocked by route — a URL
//    typed by hand lands on the dashboard, not on the hidden page.
//
// 2. A TEMPORARY flag set that hides sections in every profile while they are
//    unfinished. Remove entries from `TEMPORARILY_DISABLED` to re-enable them.
//
// The two layers answer different questions, and call sites must pick the
// right one:
//   - `isSectionEnabled(href)`        — "may I LINK to this route?" Both layers:
//                                        a temporarily disabled route redirects
//                                        too, so a link to it is dead.
//   - `isSectionKeptByProfile(href)`  — "does this FEATURE exist in this cut?"
//                                        Profile layer only. Use it for content
//                                        that is not a link (a search facet, a
//                                        permission in the role editor, a tab):
//                                        a temporary flag hides a page, not the
//                                        feature behind it.
//
// Hiding is a UI concern only. The backend keeps serving every endpoint and
// enforcing its own permissions; this file decides what the panel shows.

export type AdminProfile = 'full' | 'only-context'

const PROFILES: readonly AdminProfile[] = ['full', 'only-context']

/** Parses the build-time profile. An unset value means `full`. An unknown
 *  value throws rather than falling back to `full`, so a typo can never ship
 *  the full panel to a customer who bought the cut. vite.config.ts runs the
 *  same check at build time, where it fails the deploy instead of the page. */
export function parseAdminProfile(raw: string | undefined): AdminProfile {
  if (raw === undefined || raw === '') return 'full'
  if ((PROFILES as readonly string[]).includes(raw)) return raw as AdminProfile
  throw new Error(`Unknown VITE_ADMIN_PROFILE "${raw}". Expected one of: ${PROFILES.join(', ')}`)
}

export const ADMIN_PROFILE: AdminProfile = parseAdminProfile(import.meta.env.VITE_ADMIN_PROFILE)

// TEMPORARY feature flag — remove entries to re-enable sections in every profile.
const TEMPORARILY_DISABLED: readonly string[] = [
  '/sessions',
  '/api-keys',
  '/agents',
  '/policies',
  '/webhooks',
]

// Everything that is NOT context in the only-context cut. Kept as an explicit
// deny-list (rather than an allow-list) so a section added to the product later
// shows up in this profile by default and someone decides on it consciously.
const ONLY_CONTEXT_HIDDEN: readonly string[] = [
  '/usage',
  '/collections',
  '/sessions',
  '/tasks',
  '/sdd',
  '/harnesses',
  '/autonomous-agents',
  '/api-keys',
  '/agents',
  '/policies',
  '/webhooks',
  '/audit',
  '/backups',
]

/** Sections the PROFILE removes. Empty for `full`. */
export function profileHiddenHrefsFor(profile: AdminProfile): Set<string> {
  return new Set(profile === 'only-context' ? ONLY_CONTEXT_HIDDEN : [])
}

/** Sections whose route redirects in this build: the profile's cut plus the
 *  temporary flags. */
export function disabledNavHrefsFor(profile: AdminProfile): Set<string> {
  return new Set([...TEMPORARILY_DISABLED, ...profileHiddenHrefsFor(profile)])
}

export const PROFILE_HIDDEN_HREFS: ReadonlySet<string> = profileHiddenHrefsFor(ADMIN_PROFILE)
export const DISABLED_NAV_HREFS: ReadonlySet<string> = disabledNavHrefsFor(ADMIN_PROFILE)

/** Resolves `href` to its section: the first path segment, so `/sdd?change=x`
 *  and `/sdd/` both resolve to `/sdd`, and `/` stays `/`. */
export function sectionOf(href: string): string {
  const path = href.split(/[?#]/)[0]
  const segment = path.split('/').filter(Boolean)[0]
  return segment ? `/${segment}` : '/'
}

export function isSectionEnabledIn(href: string, disabled: ReadonlySet<string>): boolean {
  return !disabled.has(sectionOf(href))
}

/** May the panel link to `href`? False when its route would redirect. */
export function isSectionEnabled(href: string): boolean {
  return isSectionEnabledIn(href, DISABLED_NAV_HREFS)
}

/** Does the feature behind `href` exist in this cut? Ignores temporary flags. */
export function isSectionKeptByProfile(href: string): boolean {
  return isSectionEnabledIn(href, PROFILE_HIDDEN_HREFS)
}

// Features that live INSIDE a kept page rather than on a route of their own.
// The webhooks card sits in Settings, so hiding the `/webhooks` route alone
// (which the TEMPORARY set does) does not remove it — the profile has to.
export const WEBHOOKS_ENABLED = isSectionKeptByProfile('/webhooks')

export const NOTIFICATIONS_DISABLED = true
