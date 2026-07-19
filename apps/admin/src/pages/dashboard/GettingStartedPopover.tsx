import { Sparkles, Check, ChevronRight, Minus, CheckCircle } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { OnboardingItem } from '../../types'

const FOCUS_TILE = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

// Best-effort navigation for onboarding rows. GET /v1/admin/onboarding does not
// return an href per item (OnboardingItem is { key, label, description, done }),
// so this maps the well-known item keys the backend has shipped historically to
// their in-app destination. Any key not listed here renders as a non-interactive
// row (no fabricated destination) — the chevron is still shown for visual
// consistency with the mockup but the row has no onClick/href.
const ONBOARDING_LINKS: Record<string, string> = {
  connect: '/projects',
  memory: '/memories',
  invite: '/users',
  convention: '/conventions',
  conventions: '/conventions',
  project: '/projects',
  projects: '/projects',
  webhook: '/settings',
  webhooks: '/settings',
  events: '/settings',
  code: '/code',
}

export interface GettingStartedPopoverProps {
  items: OnboardingItem[]
  doneCount: number
  totalCount: number
  allDone: boolean
  minimized: boolean
  onMinimize: () => void
  onExpand: () => void
  onNavigate: (href: string) => void
}

/**
 * Design delta 2: floating collapsible popover, pinned to the viewport's
 * bottom-right (user-requested placement — mirrors the Focus-button
 * convention on Memories, and expands upward). Backed by the real
 * GET /v1/admin/onboarding checklist — consolidates what used to be two
 * separate "getting started" widgets (an API-backed inline card and a
 * locally-derived one) into this one panel.
 */
export function GettingStartedPopover({
  items,
  doneCount,
  totalCount,
  allDone,
  minimized,
  onMinimize,
  onExpand,
  onNavigate,
}: GettingStartedPopoverProps) {
  if (minimized) {
    return (
      <button
        onClick={onExpand}
        className={cn(
          'fixed right-6 bottom-6 z-40 flex items-center gap-2.5 h-11 px-4 rounded-full border border-accent-blue/35 bg-[#0d0f14]/60 backdrop-blur-[12px] shadow-lg hover:border-accent-blue/70 transition-colors',
          FOCUS_TILE
        )}
      >
        <Sparkles className="w-[15px] h-[15px] text-accent-blue" />
        <span className="text-[13px] font-semibold text-text-primary">Getting started</span>
        <span className="text-[11px] font-semibold px-2 py-0.5 rounded-full bg-accent-blue/15 text-accent-blue">
          {doneCount}/{totalCount}
        </span>
      </button>
    )
  }

  return (
    <div className="fixed right-6 bottom-6 z-40 w-[360px] max-w-[calc(100vw-3rem)] max-h-[70vh] rounded-2xl border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] shadow-2xl flex flex-col overflow-hidden">
      <div className="flex items-center gap-[11px] px-[18px] pt-4 pb-3">
        <div className="w-8 h-8 rounded-[10px] bg-accent-blue/15 flex items-center justify-center shrink-0">
          <Sparkles className="w-4 h-4 text-accent-blue" />
        </div>
        <div className="flex flex-col gap-px flex-1 min-w-0">
          <span className="text-[14px] font-bold text-text-primary">Getting started</span>
          <span className="text-[11.5px] text-text-tertiary">{doneCount} of {totalCount} completed</span>
        </div>
        <button
          onClick={onMinimize}
          aria-label="Minimize"
          title="Minimize"
          className={cn(
            'shrink-0 w-[26px] h-[26px] rounded-[8px] flex items-center justify-center text-text-tertiary hover:bg-white/[0.06] hover:text-text-primary transition-colors',
            FOCUS_TILE
          )}
        >
          <Minus className="w-3.5 h-3.5" />
        </button>
      </div>

      <div className="px-[18px] pb-1.5">
        <div className="h-1 rounded-full bg-white/[0.06] overflow-hidden">
          <div
            className="h-full rounded-full bg-accent-blue transition-all duration-500"
            style={{ width: `${totalCount > 0 ? (doneCount / totalCount) * 100 : 0}%` }}
          />
        </div>
      </div>

      {allDone ? (
        <div className="flex flex-col items-center gap-1.5 py-6">
          <CheckCircle className="w-6 h-6 text-status-success" />
          <p className="text-[13px] font-semibold text-text-primary">You're all set!</p>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto px-3 py-2.5 flex flex-col gap-1">
          {items.map(item => {
            const href = ONBOARDING_LINKS[item.key]
            const clickable = !item.done && !!href
            return (
              <div
                key={item.key}
                role={clickable ? 'button' : undefined}
                tabIndex={clickable ? 0 : undefined}
                onClick={clickable ? () => onNavigate(href) : undefined}
                onKeyDown={clickable ? (e) => { if (e.key === 'Enter') onNavigate(href) } : undefined}
                className={cn(
                  'flex items-start gap-[11px] px-2.5 py-2.5 rounded-[11px]',
                  clickable && cn('cursor-pointer hover:bg-white/[0.04]', FOCUS_TILE)
                )}
              >
                <div
                  className={cn(
                    'shrink-0 w-5 h-5 rounded-full mt-px flex items-center justify-center border-[1.5px] transition-colors',
                    item.done ? 'bg-accent-blue border-accent-blue' : 'border-white/20'
                  )}
                >
                  {item.done && <Check className="w-[11px] h-[11px] text-white" strokeWidth={3} />}
                </div>
                <div className="flex flex-col gap-0.5 flex-1 min-w-0">
                  <span
                    className={cn(
                      'text-[13px] font-semibold',
                      item.done ? 'text-text-tertiary line-through' : 'text-text-primary'
                    )}
                  >
                    {item.label}
                  </span>
                  <span className="text-[11.5px] text-text-tertiary leading-snug">{item.description}</span>
                </div>
                <ChevronRight className="w-[13px] h-[13px] text-text-quaternary shrink-0 mt-0.5" />
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
