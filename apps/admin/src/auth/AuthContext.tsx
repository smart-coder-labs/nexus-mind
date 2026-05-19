import { createContext, useContext, useState, type ReactNode } from 'react'
import type { AuthSession } from '../types'
import { loadSession, clearSession } from './session'

interface AuthContextValue {
  session: AuthSession | null
  setSession: (s: AuthSession | null) => void
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSessionState] = useState<AuthSession | null>(loadSession)

  const setSession = (s: AuthSession | null) => {
    setSessionState(s)
  }

  const logout = () => {
    clearSession()
    setSessionState(null)
  }

  return (
    <AuthContext.Provider value={{ session, setSession, logout }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be used within AuthProvider')
  return ctx
}
