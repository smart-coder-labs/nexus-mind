import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/utils'
import type { KpiMarqueeProps } from './KpiMarquee.types'
import './KpiMarquee.css'

type MetricSummaryItem = { label: string; value: string; accent?: string }
const FALLBACK_ACCENTS = ['var(--data-lime)', 'var(--data-blue)', 'var(--data-teal)', 'var(--data-amber)', 'var(--data-rose)']

function getMetricItems(children: ReactNode): MetricSummaryItem[] {
  const items: MetricSummaryItem[] = []
  const visit = (node: ReactNode) => {
    if (Array.isArray(node)) { node.forEach(visit); return }
    if (!node || typeof node !== 'object' || !('props' in node)) return
    const props = node.props as { label?: unknown; value?: unknown; accent?: unknown; children?: ReactNode }
    if (typeof props.label === 'string' && (typeof props.value === 'string' || typeof props.value === 'number')) {
      items.push({ label: props.label, value: String(props.value), accent: typeof props.accent === 'string' ? props.accent : undefined })
      return
    }
    visit(props.children)
  }
  visit(children)
  return items
}

/** Responsive statistic cards that dock into compact badges as the page scrolls. */
export function KpiMarquee({ children, className, wrapperClassName, compact, dockOnScroll, ...trackProps }: KpiMarqueeProps) {
  const enabled = dockOnScroll ?? Boolean(compact)
  const [docked, setDocked] = useState(false)
  const wrapperRef = useRef<HTMLDivElement>(null)
  const badgeRefs = useRef<Array<HTMLSpanElement | null>>([])
  const animationsRef = useRef<Animation[]>([])
  const previousDockedRef = useRef<boolean | undefined>(undefined)
  const items = getMetricItems(children)

  useEffect(() => {
    if (!enabled) return
    const workspace = document.getElementById('main-content')
    if (!workspace) return
    const updateDock = () => {
      const next = workspace.scrollTop > 72
      setDocked(current => current === next ? current : next)
    }
    updateDock()
    workspace.addEventListener('scroll', updateDock, { passive: true })
    return () => workspace.removeEventListener('scroll', updateDock)
  }, [enabled])

  useLayoutEffect(() => {
    animationsRef.current.forEach(animation => animation.cancel())
    animationsRef.current = []
    const wasDocked = previousDockedRef.current
    previousDockedRef.current = docked
    if (!enabled || wasDocked === undefined || wasDocked === docked) return
    if (typeof window === 'undefined' || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return

    const sources = wrapperRef.current?.querySelectorAll<HTMLElement>('[data-metric-label]')
    if (!sources?.length) return
    const measurements = badgeRefs.current.map((badge, index) => {
      const source = sources[index]
      if (!source || !badge) return
      const from = source.getBoundingClientRect()
      const to = badge.getBoundingClientRect()
      return {
        badge,
        dx: from.left + from.width / 2 - to.left - to.width / 2,
        dy: from.top + from.height / 2 - to.top - to.height / 2,
        index,
      }
    }).filter((measurement): measurement is { badge: HTMLSpanElement; dx: number; dy: number; index: number } => Boolean(measurement))

    measurements.forEach(({ badge, dx, dy, index }) => {
      const delay = (docked ? index : items.length - 1 - index) * 42
      const keyframes: Keyframe[] = docked ? [
        { transform: `translate(${dx}px, ${dy}px) scale(.72)`, opacity: 0.25, offset: 0 },
        { transform: `translate(${dx * 0.56}px, ${dy * 0.56 - 24}px) scale(.88)`, opacity: 0.86, offset: 0.58 },
        { transform: 'translate(0, 0) scale(1)', opacity: 1, offset: 1 },
      ] : [
        { transform: 'translate(0, 0) scale(1)', opacity: 1, offset: 0 },
        { transform: `translate(${dx * 0.56}px, ${dy * 0.56 - 24}px) scale(.88)`, opacity: 0.86, offset: 0.42 },
        { transform: `translate(${dx}px, ${dy}px) scale(.72)`, opacity: 0, offset: 1 },
      ]
      animationsRef.current.push(badge.animate(keyframes, {
        duration: 720,
        delay,
        easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
        fill: 'none',
      }))
    })

    return () => {
      animationsRef.current.forEach(animation => animation.cancel())
      animationsRef.current = []
    }
  }, [docked, enabled, items.length])

  return (
    <>
      <div
        ref={wrapperRef}
        className={cn('kpi-marquee-wrapper transition-opacity duration-300 ease-out motion-reduce:transition-none', wrapperClassName, enabled && docked && 'pointer-events-none opacity-0')}
        aria-hidden={enabled && docked}
      >
        <div className={cn('kpi-marquee-track', compact && 'kpi-summary-track', className)} {...trackProps}>
          {children}
        </div>
      </div>
      {enabled && typeof document !== 'undefined' && createPortal(<div
        aria-hidden={!docked}
        data-kpi-dock
        role="list"
        aria-label={`${trackProps['aria-label'] || 'Statistics'} summary`}
        className={cn('fixed bottom-5 right-5 z-30 flex max-w-[calc(100vw-2rem)] flex-wrap justify-end gap-2', !docked && 'pointer-events-none')}
      >
        {items.map((item, index) => <span
          key={`${item.label}-${index}`}
          ref={element => { badgeRefs.current[index] = element }}
          role="listitem"
          title={`${item.label}: ${item.value}`}
          className={cn('inline-flex min-h-8 max-w-[min(14rem,calc(100vw-2rem))] items-center gap-1.5 rounded-full px-3 py-1 text-[11px] font-semibold leading-none text-neutral-950 shadow-md ring-1 ring-black/10 dark:text-neutral-950 dark:ring-white/10', docked ? 'opacity-100' : 'opacity-0')}
          style={{ backgroundColor: item.accent || FALLBACK_ACCENTS[index % FALLBACK_ACCENTS.length] }}
        >
          <span className="truncate">{item.label}</span>
          <span className="shrink-0 font-bold tabular-nums">{item.value}</span>
        </span>)}
      </div>, document.body)}
    </>
  )
}
