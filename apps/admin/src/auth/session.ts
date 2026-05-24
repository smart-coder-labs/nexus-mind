// Session is now managed server-side via HttpOnly cookie.
// These stubs exist to avoid breaking any remaining import sites
// during the migration. They are no-ops.

import type { AuthSession } from '../types'

export function saveSession(_session: AuthSession): void {
  // no-op: session identity lives in the HttpOnly cookie set by the server
}

export function loadSession(): AuthSession | null {
  // no-op: boot via GET /v1/admin/auth/me instead
  return null
}

export function clearSession(): void {
  // no-op: logout is handled server-side via POST /v1/admin/auth/logout
}
