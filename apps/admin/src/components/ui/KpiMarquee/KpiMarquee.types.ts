import type { HTMLAttributes, ReactNode } from 'react'

export interface KpiMarqueeProps extends HTMLAttributes<HTMLDivElement> {
  /** The tile elements to render — duplicated internally for the seamless loop. */
  children: ReactNode
  /** Extra classes for the outer `overflow:hidden` wrapper (rarely needed; `className` targets the track instead). */
  wrapperClassName?: string
}
