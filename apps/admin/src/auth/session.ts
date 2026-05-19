import type { AuthSession } from '../types'

const KEY = 'nexusmind_session'

export function saveSession(session: AuthSession): void {
  localStorage.setItem(KEY, JSON.stringify(session))
}

export function loadSession(): AuthSession | null {
  try {
    const raw = localStorage.getItem(KEY)
    return raw ? (JSON.parse(raw) as AuthSession) : null
  } catch {
    return null
  }
}

export function clearSession(): void {
  localStorage.removeItem(KEY)
}
