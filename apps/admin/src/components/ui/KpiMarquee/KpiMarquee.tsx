import { Children, cloneElement, isValidElement, type ReactElement } from 'react'
import { cn } from '@/lib/utils'
import type { KpiMarqueeProps } from './KpiMarquee.types'
import './KpiMarquee.css'

/**
 * Seamless horizontal marquee for stat-tile strips (design spec: every
 * page's analytics strip animates rather than sitting static — see mockups).
 *
 * Renders `children` twice back-to-back inside a `width: max-content` flex
 * track animated with `translateX(0) -> translateX(-50%)`, so the loop reads
 * as continuous. The second copy is `aria-hidden` so screen readers never
 * hear a stat announced twice. Hovering the strip pauses the animation
 * (readability), and `prefers-reduced-motion: reduce` pauses it outright —
 * see KpiMarquee.css for both.
 *
 * Pass `role` / `aria-label` / etc. straight through — they land on the
 * track element that holds BOTH copies, but since the duplicate copy is
 * `aria-hidden`, it's excluded from the accessibility tree, so e.g.
 * `role="list"` still describes only the real (first) set of tiles.
 */
export function KpiMarquee({ children, className, wrapperClassName, ...trackProps }: KpiMarqueeProps) {
  const items = Children.toArray(children)

  return (
    <div className={cn('kpi-marquee-wrapper', wrapperClassName)}>
      <div className={cn('kpi-marquee-track', className)} {...trackProps}>
        {items}
        {items.map((child, i) => {
          if (isValidElement(child)) {
            const el = child as ReactElement<Record<string, unknown>>
            const dupKey = `kpi-marquee-dup-${el.key ?? i}`
            return cloneElement(el, { key: dupKey, 'aria-hidden': true })
          }
          return (
            <span key={`kpi-marquee-dup-${i}`} aria-hidden="true">
              {child}
            </span>
          )
        })}
      </div>
    </div>
  )
}
