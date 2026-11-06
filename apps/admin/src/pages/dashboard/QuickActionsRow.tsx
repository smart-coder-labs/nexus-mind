import { Link } from 'react-router-dom'
import type { LucideIcon } from 'lucide-react'
import { cn } from '@/lib/utils'

export type QuickAction =
  | { label: string; icon: LucideIcon; href: string }
  | { label: string; icon: LucideIcon; onAction: () => void }

const FOCUS_TILE = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

/**
 * Design delta 3: quick actions moved from a bottom card into a row of
 * compact outline buttons under the header. Handlers/hrefs are passed in
 * unchanged from the page — this component is purely presentational.
 */
export function QuickActionsRow({ actions }: { actions: QuickAction[] }) {
  return (
    <div className="flex items-center gap-1.5 flex-wrap justify-end" aria-label="Quick actions">
      {actions.map(action =>
        'href' in action ? (
          <Link
            key={action.label}
            to={action.href}
            className={cn(
              'flex items-center gap-1.5 h-8 px-3 rounded-md border border-border-secondary bg-foreground/[0.03] text-[12px] text-text-secondary hover:text-text-primary hover:border-border-primary transition-colors',
              FOCUS_TILE
            )}
          >
            <action.icon className="w-3 h-3 opacity-80" />
            {action.label}
          </Link>
        ) : (
          <button
            key={action.label}
            onClick={action.onAction}
            className={cn(
              'flex items-center gap-1.5 h-8 px-3 rounded-md border border-border-secondary bg-foreground/[0.03] text-[12px] text-text-secondary hover:text-text-primary hover:border-border-primary transition-colors',
              FOCUS_TILE
            )}
          >
            <action.icon className="w-3 h-3 opacity-80" />
            {action.label}
          </button>
        )
      )}
    </div>
  )
}
