import { createContext, useContext, useState, useEffect, type ReactNode } from 'react'
import { validateKey } from '../api/client'

interface AuthContextValue {
  authenticated: boolean
  loading: boolean
  login: (key: string) => Promise<boolean>
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [authenticated, setAuthenticated] = useState(false)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const stored = sessionStorage.getItem('bo_key')
    if (!stored) {
      setLoading(false)
      return
    }
    validateKey(stored)
      .then(valid => setAuthenticated(valid))
      .finally(() => setLoading(false))
  }, [])

  const login = async (key: string): Promise<boolean> => {
    const valid = await validateKey(key)
    if (valid) {
      sessionStorage.setItem('bo_key', key)
      setAuthenticated(true)
    }
    return valid
  }

  const logout = () => {
    sessionStorage.removeItem('bo_key')
    setAuthenticated(false)
  }

  return (
    <AuthContext.Provider value={{ authenticated, loading, login, logout }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be used within AuthProvider')
  return ctx
}
