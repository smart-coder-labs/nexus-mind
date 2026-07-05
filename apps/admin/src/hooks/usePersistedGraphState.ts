import { useCallback, useEffect, useRef, useState } from 'react'

/**
 * Options for `usePersistedGraphState`.
 *
 * - `version` (optional): bump to invalidate stored values when the shape of
 *   the persisted data changes. The stored JSON becomes `{ __v, value }`.
 *   When omitted, the value is stored as raw JSON.
 * - `debounceMs` (default 200): delay before flushing writes to localStorage,
 *   so rapid filter changes don't hammer the disk.
 * - `validate` (optional): predicate that returns true if a parsed value is
 *   acceptable. Returning false causes the hook to fall back to `defaultValue`.
 */
export interface UsePersistedGraphStateOptions {
  version?: number | string
  debounceMs?: number
  validate?: (raw: unknown) => boolean
}

/**
 * `useState`-shaped hook that mirrors the value to `localStorage` under
 * `key`. Returns `[value, setValue, reset]` — `reset` clears the storage
 * entry and falls back to `defaultValue`.
 *
 * Safe to use when localStorage is unavailable (private mode, SSR): reads
 * fall back to `defaultValue` and writes swallow exceptions.
 */
export function usePersistedGraphState<T>(
  key: string,
  defaultValue: T,
  options: UsePersistedGraphStateOptions = {},
): [T, (next: T | ((prev: T) => T)) => void, () => void] {
  const { version, debounceMs = 200, validate } = options

  const [value, setValue] = useState<T>(() => readPersisted(key, version, defaultValue, validate))

  // Keep latest options in refs so the write effect only depends on `value`.
  const keyRef = useRef(key)
  const versionRef = useRef(version)
  keyRef.current = key
  versionRef.current = version

  // Debounce writes — rapid filter toggles shouldn't spam localStorage.
  useEffect(() => {
    if (typeof window === 'undefined') return
    const write = () => {
      try {
        if (versionRef.current != null) {
          window.localStorage.setItem(
            keyRef.current,
            JSON.stringify({ __v: versionRef.current, value }),
          )
        } else {
          window.localStorage.setItem(keyRef.current, JSON.stringify(value))
        }
      } catch {
        // localStorage can throw in private mode or when the quota is full.
        // Persistence is a progressive enhancement — never break the UI.
      }
    }
    const handle = setTimeout(write, debounceMs)
    return () => clearTimeout(handle)
  }, [value, debounceMs])

  const reset = useCallback(() => {
    setValue(defaultValue)
    if (typeof window === 'undefined') return
    try {
      window.localStorage.removeItem(key)
    } catch {
      /* ignore */
    }
  }, [key, defaultValue])

  return [value, setValue, reset]
}

function readPersisted<T>(
  key: string,
  version: number | string | undefined,
  defaultValue: T,
  validate?: (raw: unknown) => boolean,
): T {
  if (typeof window === 'undefined') return defaultValue
  try {
    const stored = window.localStorage.getItem(key)
    if (stored == null) return defaultValue
    const parsed: unknown = JSON.parse(stored)
    if (version != null) {
      // Versioned shape: { __v: <version>, value: <payload> }
      if (!isRecord(parsed)) return defaultValue
      if (parsed.__v !== version) return defaultValue
      return parsed.value as T
    }
    if (validate && !validate(parsed)) return defaultValue
    return parsed as T
  } catch {
    return defaultValue
  }
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v)
}
