/**
 * Routes that render without an authenticated session.
 *
 * Keep in sync with the routes declared outside <ProtectedRoute> in App.tsx.
 * The API client uses this to decide whether a 401 should eject the user to
 * /login — on these routes a 401 is the expected state, not a session loss.
 */
export const PUBLIC_ROUTES = ['/login', '/set-password'] as const

export function isPublicRoute(pathname: string | undefined): boolean {
  if (!pathname) return false
  return PUBLIC_ROUTES.some(route => pathname === route || pathname.startsWith(`${route}/`))
}
