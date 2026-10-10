import type { HTMLAttributes, ReactNode } from 'react'

export interface KpiMarqueeProps extends HTMLAttributes<HTMLDivElement> {
  compact?: boolean
  /** Convert compact metric cards into floating colored badges on page scroll. Defaults to `compact`. */
  dockOnScroll?: boolean
  /** Statistics rendered once in a responsive grid. */
  children: ReactNode
  /** Extra classes for the outer statistics group (rarely needed; `className` targets the track instead). */
  wrapperClassName?: string
}
