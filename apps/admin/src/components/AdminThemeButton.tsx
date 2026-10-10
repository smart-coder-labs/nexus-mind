import { useEffect, useState } from 'react'
import { Moon, Sun } from 'lucide-react'

const STORAGE_KEY = 'nexusmind-admin-theme'
const THEME_EVENT = 'nexusmind-theme-change'
const isDark = () => document.documentElement.classList.contains('dark')

export function AdminThemeButton({ compact = false }: { compact?: boolean }) {
  const [dark, setDark] = useState(isDark)
  useEffect(() => {
    const sync = () => setDark(isDark())
    window.addEventListener(THEME_EVENT, sync)
    return () => window.removeEventListener(THEME_EVENT, sync)
  }, [])
  const toggle = () => {
    const next = !isDark()
    document.documentElement.classList.toggle('dark', next)
    document.documentElement.dataset.theme = next ? 'dark' : 'light'
    try { localStorage.setItem(STORAGE_KEY, next ? 'dark' : 'light') } catch { /* Theme remains usable without storage. */ }
    window.dispatchEvent(new Event(THEME_EVENT))
  }
  return <button
    className={compact ? 'lima-theme-button lima-theme-button--compact' : 'lima-theme-button'}
    type="button"
    onClick={toggle}
    aria-label={dark ? 'Switch to light mode' : 'Switch to dark mode'}
    aria-pressed={dark}
    title={dark ? 'Switch to light mode' : 'Switch to dark mode'}
  >
    {dark ? <Sun size={17} aria-hidden="true" /> : <Moon size={17} aria-hidden="true" />}
    {!compact && <span>{dark ? 'Light mode' : 'Dark mode'}</span>}
  </button>
}
