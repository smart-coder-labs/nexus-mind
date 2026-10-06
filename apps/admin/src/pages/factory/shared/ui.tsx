import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Ban, CheckCircle2, CircleDashed, Clock, Loader2, MoreHorizontal, RefreshCw, XCircle } from 'lucide-react'
import { createClient } from '../../../api/client'
import { useAuth } from '../../../auth/AuthContext'
import { Badge } from '../../../components/ui/Badge'
import { Button } from '../../../components/ui/Button'
import { runStatusMeta } from './format'

/** Session, permission check, API client and cache helpers every factory page needs. */
export function useFactory() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const can = (permission: string) => permissions.includes(permission)
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const invalidate = (...keys: string[]) => Promise.all(keys.map(key => queryClient.invalidateQueries({ queryKey: [key] })))
  return { session, can, client, queryClient, invalidate }
}

export const errorMessage = (value: unknown, fallback: string) =>
  (value instanceof Error && value.message) || (value as { message?: string } | null)?.message || fallback

/** Page header per DESIGN_DIRECTION §4: title + one-line subtitle + actions right-aligned. */
export function PageHeader({ title, subtitle, actions }: { title: string; subtitle: string; actions?: ReactNode }) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="text-[22px] font-semibold leading-[1.2] tracking-[-0.3px] text-text-primary">{title}</h1>
        <p className="mt-1 text-[13px] text-text-secondary">{subtitle}</p>
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  )
}

/** Section title (15px/600). */
export function SectionTitle({ children, id, className = '' }: { children: ReactNode; id?: string; className?: string }) {
  return <h2 id={id} className={`text-[15px] font-semibold tracking-[-0.2px] text-text-primary ${className}`}>{children}</h2>
}

/** Small group label (12px/500, sentence case). */
export function FieldLabel({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <p className={`text-[12px] font-medium text-text-tertiary ${className}`}>{children}</p>
}

/** Inline error (§5): icon + what failed + retry. */
export function InlineError({ message, onRetry, onDismiss }: { message: string; onRetry?: () => void; onDismiss?: () => void }) {
  return (
    <div role="alert" className="flex flex-wrap items-center gap-3 rounded-[11px] border border-status-error/25 bg-status-error/[0.08] px-4 py-3 text-[13px] text-text-primary">
      <AlertTriangle className="h-4 w-4 shrink-0 text-status-error" aria-hidden />
      <span className="min-w-0 flex-1">{message}</span>
      {onRetry && <Button size="sm" variant="secondary" leftIcon={<RefreshCw className="h-3.5 w-3.5" />} onClick={onRetry}>Try again</Button>}
      {onDismiss && <Button size="sm" variant="ghost" onClick={onDismiss}>Dismiss</Button>}
    </div>
  )
}

/** Permission-denied state that names the permission. */
export function PermissionNote({ permission, action }: { permission: string; action: string }) {
  return (
    <p className="text-[12px] text-text-tertiary">
      To {action} you need the <span className="font-mono text-text-secondary">{permission}</span> permission. An org admin can grant it in Roles.
    </p>
  )
}

const RUN_ICON: Record<string, ReactNode> = {
  queued: <Clock className="h-3 w-3" aria-hidden />,
  leased: <Loader2 className="h-3 w-3 animate-spin motion-reduce:animate-none" aria-hidden />,
  running: <Loader2 className="h-3 w-3 animate-spin motion-reduce:animate-none" aria-hidden />,
  succeeded: <CheckCircle2 className="h-3 w-3" aria-hidden />,
  partial: <AlertTriangle className="h-3 w-3" aria-hidden />,
  budget_exhausted: <AlertTriangle className="h-3 w-3" aria-hidden />,
  blocked_runtime: <XCircle className="h-3 w-3" aria-hidden />,
  blocked_policy: <XCircle className="h-3 w-3" aria-hidden />,
  failed: <XCircle className="h-3 w-3" aria-hidden />,
  cancelled: <Ban className="h-3 w-3" aria-hidden />,
  dead_letter: <XCircle className="h-3 w-3" aria-hidden />,
}

/** Run status as icon + word, never color alone. */
export function RunStatusPill({ status, size = 'sm' }: { status: string; size?: 'sm' | 'md' }) {
  const meta = runStatusMeta(status)
  return (
    <Badge size={size} variant={meta.variant} className="shrink-0">
      {RUN_ICON[status] ?? <CircleDashed className="h-3 w-3" aria-hidden />}
      {meta.label}
    </Badge>
  )
}

/** Raw payload escape hatch — always last, always collapsed. */
export function RawJson({ label, value }: { label: string; value: unknown }) {
  return (
    <details className="group border-t border-border-secondary pt-3">
      <summary className="cursor-pointer rounded-[8px] text-[12px] text-text-tertiary hover:text-text-secondary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">{label}</summary>
      <pre className="mt-2.5 max-h-[420px] overflow-auto rounded-[11px] bg-black/30 p-3 font-mono text-[11px] leading-relaxed text-text-secondary whitespace-pre-wrap break-all">{JSON.stringify(value, null, 2)}</pre>
    </details>
  )
}

export type MenuItem = { label: string; onSelect: () => void; icon?: ReactNode; danger?: boolean; disabled?: boolean }

/**
 * Accessible overflow menu: a labelled button with aria-haspopup/expanded and a
 * role="menu" list. Arrow keys move, Escape closes and returns focus, clicking
 * outside closes.
 */
export function OverflowMenu({ label, items }: { label: string; items: MenuItem[] }) {
  const [open, setOpen] = useState(false)
  const menuId = useId()
  const buttonRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const first = menuRef.current?.querySelector<HTMLButtonElement>('[role="menuitem"]:not([disabled])')
    first?.focus()
    const onDown = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node) && !buttonRef.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [open])

  if (items.length === 0) return null

  const onKeyDown = (event: React.KeyboardEvent) => {
    const nodes = [...(menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not([disabled])') ?? [])]
    const index = nodes.indexOf(document.activeElement as HTMLButtonElement)
    if (event.key === 'Escape') { event.preventDefault(); setOpen(false); buttonRef.current?.focus() }
    else if (event.key === 'ArrowDown') { event.preventDefault(); nodes[(index + 1) % nodes.length]?.focus() }
    else if (event.key === 'ArrowUp') { event.preventDefault(); nodes[(index - 1 + nodes.length) % nodes.length]?.focus() }
    else if (event.key === 'Home') { event.preventDefault(); nodes[0]?.focus() }
    else if (event.key === 'End') { event.preventDefault(); nodes[nodes.length - 1]?.focus() }
    else if (event.key === 'Tab') setOpen(false)
  }

  return (
    <div className="relative">
      <button
        ref={buttonRef}
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={() => setOpen(value => !value)}
        className="grid h-8 w-8 place-items-center rounded-full border border-border-primary text-text-secondary transition-colors hover:bg-white/5 hover:text-text-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring"
      >
        <MoreHorizontal className="h-4 w-4" aria-hidden />
      </button>
      {open && (
        <div
          ref={menuRef}
          id={menuId}
          role="menu"
          aria-label={label}
          onKeyDown={onKeyDown}
          className="absolute right-0 z-30 mt-1.5 min-w-[180px] rounded-[18px] border border-border-primary bg-background-tertiary p-1.5"
        >
          {items.map(item => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              disabled={item.disabled}
              onClick={() => { setOpen(false); item.onSelect() }}
              className={`flex w-full items-center gap-2 rounded-[8px] px-3 py-2 text-left text-[13px] transition-colors hover:bg-white/[0.06] focus-visible:bg-white/[0.06] focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-focus-ring disabled:opacity-40 ${item.danger ? 'text-status-error' : 'text-text-primary'}`}
            >
              {item.icon && <span className="inline-flex text-current" aria-hidden>{item.icon}</span>}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

/** Filter chip with a count; pressed state is announced. */
export function FilterChip({ label, count, active, onClick }: { label: string; count?: number; active: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={`inline-flex h-7 items-center gap-1.5 rounded-full border px-3 text-[12px] font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring ${active ? 'border-accent-blue/40 bg-accent-blue/15 text-text-primary' : 'border-border-primary text-text-secondary hover:text-text-primary'}`}
    >
      {label}
      {count != null && <span className={`tabular-nums ${active ? 'text-text-secondary' : 'text-text-tertiary'}`}>{count}</span>}
    </button>
  )
}

/** Text link on dark surfaces (Sky Link Blue passes AA on near-black; Action Blue does not). */
export const TEXT_LINK = 'rounded-[8px] text-[13px] font-medium text-[var(--color-accent-on-dark)] hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'
