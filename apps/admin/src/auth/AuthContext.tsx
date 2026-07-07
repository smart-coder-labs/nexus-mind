import { createContext, useContext, useState, useEffect, type ReactNode } from 'react'
import type { AuthSession } from '../types'
import { createClient } from '../api/client'

interface AuthContextValue {
  session: AuthSession | null
  loading: boolean
  setSession: (s: AuthSession | null) => void
  logout: () => void
}

// Module-level token store — kept in sync by AuthProvider so that
// getAuthToken() can be called outside the React tree (e.g. in download.ts).
let _currentToken: string | null = null

/**
 * Returns the current session token (API key / Bearer token) without
 * requiring a React hook. Returns null when the session uses cookie-only auth.
 */
export function getAuthToken(): string | null {
  return _currentToken
}

export const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSessionState] = useState<AuthSession | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const client = createClient()
    client.getMe()
      .then(data => setSessionState(data))
      .catch(() => setSessionState(null))
      .finally(() => setLoading(false))
  }, [])

  const setSession = (s: AuthSession | null) => {
    setSessionState(s)
  }

  const logout = () => {
    const client = createClient()
    client.logout().catch(() => {/* ignore errors on logout */})
    setSessionState(null)
  }

  return (
    <AuthContext.Provider value={{ session, loading, setSession, logout }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be used within AuthProvider')
  return ctx
}

export function isPrivileged(role: string | undefined): boolean {
  return role === 'admin' || role === 'super_user'
}
