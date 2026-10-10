import type { ReactNode } from 'react'
import { AlertCircle, Lock } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import { EmptyState } from '../../../components/ui/EmptyState'
import { Skeleton } from '../../../components/ui/Skeleton'
import { cn } from '../../../lib/utils'

/**
 * Small presentation pieces shared by the factory "govern" pages (Needs a human,
 * Intake, Autonomy policies, Decision model). Page-local on purpose: they encode
 * DESIGN_DIRECTION §3–§5 for these pages only.
 */

/** Native select aligned with the Audit1 control geometry. */
export const SELECT_CLASS =
  'block h-9 w-full rounded-md border border-border-primary bg-foreground/[0.03] px-3 text-base md:text-sm text-text-primary ' +
  'transition-apple focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring ' +
  'disabled:cursor-not-allowed disabled:opacity-40'

/** Neutral Audit1 section card with a thin border. */
export const PANEL_CLASS = 'min-w-0 rounded-xl border border-border-primary bg-card p-4 sm:p-5'

export const LINK_CLASS =
  'text-accent-on-dark underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring rounded-[4px]'

export function PageHeader({
  title,
  subtitle,
  action,
  aside,
}: {
  title: string
  subtitle: ReactNode
  action?: ReactNode
  /** Quiet element next to the title, e.g. a state pill. */
  aside?: ReactNode
}) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-4">
      <div className="min-w-0 flex-1 basis-[18rem]">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <h1 className="text-[22px] font-semibold leading-[1.2] tracking-[-0.3px] text-text-primary">{title}</h1>
          {aside}
        </div>
        <p className="mt-1 max-w-3xl text-sm leading-normal text-text-secondary">{subtitle}</p>
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </header>
  )
}

export function SectionHeading({ id, title, description, action }: { id: string; title: string; description?: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-3">
      <div className="min-w-0">
        <h2 id={id} className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">{title}</h2>
        {description && <p className="mt-1 max-w-3xl text-sm text-text-secondary">{description}</p>}
      </div>
      {action}
    </div>
  )
}

/** Inline error alert (§5): 11px radius, error tint, icon, message, retry. */
export function InlineAlert({ children, onRetry, className }: { children: ReactNode; onRetry?: () => void; className?: string }) {
  return (
    <div
      role="alert"
      className={cn(
        'flex flex-wrap items-center gap-3 rounded-md border border-status-error/20 bg-status-error/[0.08] px-3.5 py-2.5 text-sm text-text-primary',
        className,
      )}
    >
      <AlertCircle className="h-4 w-4 shrink-0 text-status-error" aria-hidden="true" />
      <span className="min-w-0 flex-1">{children}</span>
      {onRetry && (
        <Button size="sm" variant="secondary" onClick={onRetry}>
          Try again
        </Button>
      )}
    </div>
  )
}

/** Permission-denied state that names the permission and who grants it. */
export function PermissionDenied({ title, permission, what }: { title: string; permission: string; what: string }) {
  return (
    <div className="p-6 md:p-8 max-w-7xl mx-auto">
      <EmptyState
        icon={<Lock />}
        title={title}
        description={`You need the ${permission} permission to ${what}. Ask an organization owner to grant it.`}
      />
    </div>
  )
}

/** Label above the control, 12px/500 (§5). */
export function Field({
  id,
  label,
  hint,
  children,
  className,
}: {
  id: string
  label: string
  hint?: ReactNode
  children: ReactNode
  className?: string
}) {
  return (
    <div className={cn('min-w-0', className)}>
      <label htmlFor={id} className="mb-1.5 block text-[12px] font-medium text-text-secondary">
        {label}
      </label>
      {children}
      {hint && <p className="mt-1.5 text-[12px] leading-normal text-text-tertiary">{hint}</p>}
    </div>
  )
}

/** A group of fields with a 15px heading, used to chunk long editors. */
export function FieldGroup({ title, description, children }: { title: string; description?: ReactNode; children: ReactNode }) {
  return (
    <fieldset className="min-w-0 space-y-4 border-t border-border-primary pt-5 first:border-t-0 first:pt-0">
      <legend className="float-left mb-1 w-full text-[15px] font-semibold tracking-[-0.2px] text-text-primary">{title}</legend>
      {description && <p className="clear-both text-[12px] leading-normal text-text-tertiary">{description}</p>}
      <div className="clear-both space-y-4">{children}</div>
    </fieldset>
  )
}

/** A radio rendered as a selectable card with a plain description. */
export function RadioCard({
  name,
  value,
  checked,
  onChange,
  title,
  description,
  icon,
  disabled,
}: {
  name: string
  value: string
  checked: boolean
  onChange: (value: string) => void
  title: string
  description?: string
  icon?: ReactNode
  disabled?: boolean
}) {
  return (
    <label
      className={cn(
        'flex gap-3 rounded-md border px-3.5 py-3 transition-apple',
        checked ? 'border-accent-blue/60 bg-action-primary-tint' : 'border-border-primary hover:bg-foreground/[0.03]',
        disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer',
      )}
    >
      <span className="relative mt-0.5 inline-flex h-[18px] w-[18px] shrink-0 items-center justify-center">
        <input
          type="radio"
          name={name}
          value={value}
          checked={checked}
          disabled={disabled}
          onChange={() => onChange(value)}
          className={cn(
            'peer m-0 h-[18px] w-[18px] appearance-none rounded-full border-[1.5px] transition-colors',
            'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring',
            checked ? 'border-accent-blue' : 'border-border-primary',
          )}
        />
        <span className="pointer-events-none absolute h-[9px] w-[9px] rounded-full bg-action-primary opacity-0 transition-opacity peer-checked:opacity-100" />
      </span>
      <span className="min-w-0">
        <span className="flex items-center gap-2 text-sm font-medium text-text-primary">
          {icon}
          {title}
        </span>
        {description && <span className="mt-0.5 block text-[12px] leading-normal text-text-secondary">{description}</span>}
      </span>
    </label>
  )
}

/** Static compact summary: labels and values stay visible. */
export function StatTile({ label, value, tone }: { label: string; value: ReactNode; tone?: 'error' }) {
  return (
    <div className="flex min-w-0 items-center justify-between gap-2 rounded-lg border border-border-primary bg-card px-3 py-2">
      <dt className="min-w-0 text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className={cn('shrink-0 rounded-md bg-muted px-2 py-0.5 text-xs font-semibold tabular-nums', tone === 'error' ? 'text-status-error' : 'text-text-primary')}>
        {value}
      </dd>
    </div>
  )
}

/** Skeleton cards that mirror a list of cards. */
export function CardListSkeleton({ count = 3, height = 'h-28' }: { count?: number; height?: string }) {
  return (
    <div className="space-y-3" aria-hidden="true">
      {Array.from({ length: count }, (_, index) => (
        <Skeleton key={index} className={cn('w-full rounded-xl', height)} />
      ))}
    </div>
  )
}

/** `2026-10-04T01:10:00Z` → `2026-10-04 01:10`. */
export function when(value: string | null | undefined): string {
  return value ? value.replace('T', ' ').slice(0, 16) : 'never'
}
