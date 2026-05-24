import { createContext, useContext, useState, useEffect, type ReactNode } from 'react'
import type { AuthSession } from '../types'
import { createClient } from '../api/client'

interface AuthContextValue {
  session: AuthSession | null
  loading: boolean
  setSession: (s: AuthSession | null) => void
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

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
