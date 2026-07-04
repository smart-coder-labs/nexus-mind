// TEMPORARY feature flag — remove entries from these sets to re-enable sections.
//
// To re-enable everything:
//   DISABLED_NAV_HREFS = new Set()
//   NOTIFICATIONS_DISABLED = false

export const DISABLED_NAV_HREFS = new Set([
  '/sessions',
  '/api-keys',
  '/agents',
  '/policies',
  '/webhooks',
])

export const NOTIFICATIONS_DISABLED = true
