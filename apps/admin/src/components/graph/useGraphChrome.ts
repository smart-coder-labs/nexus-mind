import { useCallback, useEffect, useRef, useState } from 'react'
import { usePersistedGraphState } from '../../hooks/usePersistedGraphState'

/**
 * Shell behavior shared by the immersive graph pages: focus mode (button /
 * `F` / double-click / `Esc`), idle auto-hide, auto-rotate, container
 * measurement and camera flights.
 *
 * Lifted verbatim out of `OrgMemoryGraph` so the memory graph and the code
 * graph behave identically — only the data adapters differ.
 */

// Idle time before auto-hide kicks in (design: 3.5s).
const AUTO_HIDE_MS = 3500

export interface FgInstance {
  controls?: () => { autoRotate?: boolean; autoRotateSpeed?: number }
  cameraPosition?: (
    pos: { x?: number; y?: number; z?: number },
    lookAt?: { x: number; y: number; z: number },
    ms?: number,
  ) => void
  zoomToFit?: (ms?: number, padding?: number) => void
}

interface UseGraphChromeOptions {
  /** localStorage key suffix for the persisted behavior toggles. */
  storageKey: string
  /** True while a node detail panel is open — `Esc` closes it before exiting
   *  focus, and auto-hide never fires while something is selected. */
  hasSelection: boolean
  /** Closes the detail panel (first `Esc` press). */
  clearSelection: () => void
  /** True while a node is hovered — pauses auto-rotate so tooltips stay put. */
  hoveredNode: boolean
  /** Only start auto-rotate once there is something to rotate. */
  graphReady: boolean
}

export function useGraphChrome({
  storageKey,
  hasSelection,
  clearSelection,
  hoveredNode,
  graphReady,
}: UseGraphChromeOptions) {
  // User-configurable behavior (design props `autoRotate` / `autoHide`,
  // both default true). Persisted like the rest of the graph state.
  const [autoRotate, setAutoRotate] = usePersistedGraphState<boolean>(
    `nexusmind-graph-auto-rotate-${storageKey}`, true,
  )
  const [autoHide, setAutoHide] = usePersistedGraphState<boolean>(
    `nexusmind-graph-auto-hide-${storageKey}`, true,
  )
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [focused, setFocused] = useState(false)

  // Whether the current focus state was entered automatically (idle) — those
  // exit on any pointer movement; manual focus does not.
  const autoFocusedRef = useRef(false)
  const idleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const containerRef = useRef<HTMLDivElement>(null)
  const fgRef = useRef<FgInstance | null>(null)
  const [size, setSize] = useState({ w: 0, h: 0 })

  const toggleFocus = useCallback(() => {
    autoFocusedRef.current = false
    setFocused(f => !f)
  }, [])

  // Measure the container so the 3D graph fills it exactly in both modes.
  // Guarded for environments without ResizeObserver (jsdom in tests).
  useEffect(() => {
    const el = containerRef.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(entries => {
      const r = entries[0]?.contentRect
      if (r) setSize({ w: Math.floor(r.width), h: Math.floor(r.height) })
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // Keyboard: F toggles focus (ignored while typing in a field), Esc closes
  // the detail panel first, then exits focus — same order as the design.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null
      const typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)
      if (e.key === 'Escape') {
        if (hasSelection) { clearSelection(); return }
        if (focused) { autoFocusedRef.current = false; setFocused(false) }
        return
      }
      if ((e.key === 'f' || e.key === 'F') && !typing) {
        e.preventDefault()
        autoFocusedRef.current = false
        setFocused(f => !f)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [focused, hasSelection, clearSelection])

  // Live mirrors of state for the idle timer (avoids stale closures without
  // re-registering the listener on every state change).
  const focusedRef = useRef(focused)
  const selectedRef = useRef(hasSelection)
  useEffect(() => { focusedRef.current = focused }, [focused])
  useEffect(() => { selectedRef.current = hasSelection }, [hasSelection])

  // Auto-hide: after 3.5s of pointer inactivity (and nothing selected) enter
  // focus automatically; ANY pointer movement exits an auto-entered focus.
  useEffect(() => {
    if (!autoHide) return
    const onMove = () => {
      if (autoFocusedRef.current) {
        autoFocusedRef.current = false
        setFocused(false)
      }
      if (idleTimerRef.current) clearTimeout(idleTimerRef.current)
      idleTimerRef.current = setTimeout(() => {
        if (!focusedRef.current && !selectedRef.current) {
          autoFocusedRef.current = true
          setFocused(true)
        }
      }, AUTO_HIDE_MS)
    }
    window.addEventListener('pointermove', onMove)
    onMove()
    return () => {
      window.removeEventListener('pointermove', onMove)
      if (idleTimerRef.current) clearTimeout(idleTimerRef.current)
    }
  }, [autoHide])

  // Auto-rotate (OrbitControls via controlType="orbit"). Pauses while a node
  // is hovered — the design pauses rotation on hover so tooltips stay put.
  useEffect(() => {
    if (!graphReady) return
    let raf = 0
    let tries = 0
    const apply = () => {
      const controls = fgRef.current?.controls?.()
      if (controls) {
        controls.autoRotate = autoRotate && !hoveredNode
        controls.autoRotateSpeed = 0.6
        return
      }
      if (tries++ < 60) raf = requestAnimationFrame(apply)
    }
    apply()
    return () => cancelAnimationFrame(raf)
  }, [graphReady, autoRotate, hoveredNode])

  // ── Camera flights ─────────────────────────────────────────────────────────

  const flyTo = useCallback((x: number, y: number, z: number, dist: number) => {
    const fg = fgRef.current
    if (!fg?.cameraPosition) return
    const len = Math.hypot(x, y, z) || 1
    const k = 1 + dist / len
    fg.cameraPosition({ x: x * k, y: y * k, z: z * k }, { x, y, z }, 900)
  }, [])

  const flyHome = useCallback(() => {
    fgRef.current?.zoomToFit?.(900, 60)
  }, [])

  return {
    containerRef,
    fgRef,
    size,
    focused,
    // `setFocused` is deliberately NOT returned: setting it without clearing
    // `autoFocusedRef` would make a manual focus exit on the next pointer move.
    toggleFocus,
    autoRotate,
    setAutoRotate,
    autoHide,
    setAutoHide,
    settingsOpen,
    setSettingsOpen,
    flyTo,
    flyHome,
  }
}
