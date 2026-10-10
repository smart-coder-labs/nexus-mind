import { StyledSelect } from '@/components/ui/Select/StyledSelect'
import { ChevronDown, RotateCcw, Search, Settings2 } from 'lucide-react'

/**
 * Shared presentational chrome for the immersive, full-bleed graph pages
 * (memory knowledge graph and code graph).
 *
 * Everything here is pure presentation lifted verbatim out of
 * `OrgMemoryGraph` so both graphs render the SAME glass surfaces, spacing and
 * focus-mode transitions. Data fetching, node colors and detail panels stay in
 * the per-graph adapters — only the shell is shared.
 */

// Shared glass-surface styling for every floating control (design spec:
// rgba(13,15,20,0.72) + blur 14).
export const GLASS = 'border border-border-primary bg-surface-primary '
export const GLASS_SOFT = 'border border-border-primary bg-surface-primary '

// Keyboard focus indicator (matches the rest of the admin app).
export const FOCUS_RING = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

// Chrome fade/slide transition curve when entering/leaving focus mode.
const EASE = '[transition-timing-function:cubic-bezier(0.32,0.72,0,1)]'

/** Fade/slide classes for a chrome layer, driven by focus mode. */
export function chromeCls(focused: boolean, slide: string): string {
  return `transition-[opacity,transform] duration-[450ms] ${EASE} ${
    focused ? `opacity-0 pointer-events-none ${slide}` : 'opacity-100 translate-x-0 translate-y-0'
  }`
}

export const fmt = (n: number) => n.toLocaleString('en-US')

// ── Top bar ──────────────────────────────────────────────────────────────────

/**
 * Floating top bar: title + subtitle on the left, a control cluster on the
 * right. Controls are passed as children so each graph composes its own
 * (tabs / search / selector / settings).
 */
export function GraphTopBar({
  title,
  subtitle,
  focused,
  children,
}: {
  title?: string
  subtitle?: string
  focused: boolean
  children: React.ReactNode
}) {
  return (
    <div inert={focused} aria-hidden={focused} className={`relative z-20 flex flex-col gap-3 border-b border-border-primary bg-background/95 p-4 pointer-events-none ${chromeCls(focused, '-translate-y-3.5')}`}>
      <div className="pointer-events-auto min-w-0 pr-28">
        {title && (
          <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">{title}</h1>
        )}
        {subtitle && (
          <p className="text-sm text-text-secondary mt-1 max-w-[560px]">{subtitle}</p>
        )}
      </div>
      <div className="pointer-events-auto flex min-w-0 items-center gap-2 flex-wrap">
        {children}
      </div>
    </div>
  )
}

/**
 * Segmented switch between the graph sources (Knowledge / Code). Rendered
 * inside the top bar's control cluster so it fades with the rest of the
 * chrome in focus mode.
 */
export function GraphTabs<T extends string>({
  value,
  onChange,
  tabs,
  label,
}: {
  value: T
  onChange: (next: T) => void
  tabs: { id: T; label: string }[]
  label: string
}) {
  return (
    // `role="group"` + `aria-pressed`, not the ARIA tablist pattern: there is
    // no `tabpanel` to point at (the switch swaps the whole full-bleed graph,
    // not a panel), and the pattern would also owe arrow-key navigation. This
    // mirrors how `TypeChip` below exposes its state.
    <div
      role="group"
      aria-label={label}
      className={`flex items-center h-9 px-1 gap-1 rounded-md ${GLASS}`}
    >
      {tabs.map(tab => {
        const active = tab.id === value
        return (
          <button
            key={tab.id}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(tab.id)}
            className={`h-7 px-3.5 rounded-md text-sm font-semibold transition-colors cursor-pointer ${FOCUS_RING} ${
              active
                ? 'bg-foreground/[0.10] text-text-primary'
                : 'text-text-tertiary hover:text-text-primary'
            }`}
          >
            {tab.label}
          </button>
        )
      })}
    </div>
  )
}

/** Node search box — shows a blue match count once the query is active. */
export function GraphSearchBox({
  value,
  onChange,
  active,
  count,
  maxMatches,
  placeholder = 'Search nodes…',
}: {
  value: string
  onChange: (next: string) => void
  active: boolean
  count: number
  maxMatches: number
  placeholder?: string
}) {
  return (
    <div className={`flex items-center gap-2 h-9 px-3.5 rounded-md ${GLASS} min-w-0 flex-1 basis-40 max-w-full sm:max-w-[280px]`}>
      <Search className="w-[15px] h-[15px] text-text-tertiary shrink-0" aria-hidden="true" />
      <input
        type="text"
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label="Search graph nodes"
        className="flex-1 min-w-0 bg-transparent border-none outline-none text-sm text-text-primary placeholder:text-text-tertiary"
      />
      {active && (
        <span className="shrink-0 text-[11px] font-bold text-accent-blue" aria-label={`${count} matching nodes`}>
          {count >= maxMatches ? `${maxMatches}+` : count}
        </span>
      )}
    </div>
  )
}

/** Glass `<StyledSelect>` used for the project / repository pickers. */
export function GraphSelect({
  value,
  onChange,
  disabled,
  ariaLabel,
  placeholder,
  options,
}: {
  value: string
  onChange: (next: string) => void
  disabled?: boolean
  ariaLabel: string
  placeholder: string
  options: { value: string; label: string }[]
}) {
  return (
    <div className="relative min-w-0 max-w-full">
      <StyledSelect
        value={value}
        onChange={e => onChange(e.target.value)}
        disabled={disabled}
        aria-label={ariaLabel}
        className={`appearance-none max-w-full h-9 ${GLASS} rounded-md pl-3.5 pr-9 text-sm text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer disabled:opacity-50 ${FOCUS_RING}`}
      >
        <option value="">{placeholder}</option>
        {options.map(o => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </StyledSelect>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-tertiary" />
    </div>
  )
}

/** Gear button + popover holding the behavior toggles. */
export function GraphSettings({
  open,
  onOpenChange,
  children,
}: {
  open: boolean
  onOpenChange: (next: boolean) => void
  children: React.ReactNode
}) {
  return (
    <div className="relative ml-auto min-w-0 max-w-full" onPointerDown={event => event.stopPropagation()}>
      <button
        type="button"
        onClick={() => onOpenChange(!open)}
        className={`flex items-center justify-center w-9 h-9 rounded-md ${GLASS} text-text-secondary hover:text-text-primary hover:border-border-primary transition-colors ${FOCUS_RING}`}
        aria-label="Graph settings"
        aria-expanded={open}
      >
        <Settings2 className="w-4 h-4" />
      </button>
      {open && (
        <div className={`absolute right-0 top-11 w-[220px] rounded-xl ${GLASS} shadow-[0_12px_40px_rgba(0,0,0,0.45)] p-3 space-y-1`}>
          {children}
        </div>
      )}
    </div>
  )
}

export function SettingToggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string
  description: string
  checked: boolean
  onChange: (v: boolean) => void
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={`w-full flex items-center justify-between gap-3 px-2 py-2 rounded-md hover:bg-foreground/[0.05] transition-colors text-left ${FOCUS_RING}`}
    >
      <span className="min-w-0">
        <span className="block text-[12.5px] font-semibold text-text-primary">{label}</span>
        <span className="block text-[11px] text-text-tertiary">{description}</span>
      </span>
      <span
        className={`shrink-0 w-[34px] h-[20px] rounded-full p-[2px] transition-colors ${checked ? 'bg-action-primary' : 'bg-foreground/[0.12]'}`}
        aria-hidden="true"
      >
        <span
          className={`block w-4 h-4 rounded-full bg-white transition-transform ${checked ? 'translate-x-[14px]' : 'translate-x-0'}`}
        />
      </span>
    </button>
  )
}

// ── Chip rows ────────────────────────────────────────────────────────────────

/** Container for the floating chip rows below the top bar. */
export function GraphChipRows({
  focused,
  offsetForChrome,
  children,
}: {
  focused: boolean
  offsetForChrome: boolean
  children: React.ReactNode
}) {
  return (
    <div inert={focused} aria-hidden={focused} data-with-header={offsetForChrome} className={`relative z-10 flex max-h-[190px] flex-col gap-2 overflow-y-auto p-4 pointer-events-none ${chromeCls(focused, '-translate-y-3.5')}`}>
      {children}
    </div>
  )
}

export function GraphChipRow({
  children,
  ...rest
}: { children: React.ReactNode } & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className="pointer-events-auto flex items-center gap-2 flex-wrap max-w-full" {...rest}>
      {children}
    </div>
  )
}

/** Node-type filter chip — always type-colored, opacity signals on/off. */
export function TypeChip({
  type,
  color,
  active,
  darkInk,
  onClick,
}: {
  type: string
  color: string
  active: boolean
  darkInk?: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center h-[28px] px-[13px] rounded-xl text-[12px] font-semibold cursor-pointer transition-opacity hover:brightness-[1.15]"
      style={{
        backgroundColor: color,
        color: darkInk ? '#1a1405' : '#ffffff',
        opacity: active ? 1 : 0.28,
      }}
      aria-pressed={active}
      aria-label={`Toggle ${type} nodes`}
    >
      {type}
    </button>
  )
}

/** Neutral glass chip used for secondary chip-row actions. */
export function GlassChip({
  onClick,
  ariaLabel,
  children,
}: {
  onClick: () => void
  ariaLabel: string
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-[7px] h-[28px] px-[13px] rounded-xl ${GLASS_SOFT} text-[12px] font-semibold text-text-secondary hover:text-text-primary hover:border-border-primary transition-colors`}
      aria-label={ariaLabel}
    >
      {children}
    </button>
  )
}

export function ResetFiltersChip({ onClick }: { onClick: () => void }) {
  return (
    <GlassChip onClick={onClick} ariaLabel="Reset graph filters">
      <RotateCcw className="w-3 h-3" />
      Reset filters
    </GlassChip>
  )
}

// ── Bottom chrome ────────────────────────────────────────────────────────────

export function GraphStatsPill({
  focused,
  offsetForChrome,
  children,
}: {
  focused: boolean
  offsetForChrome: boolean
  children: React.ReactNode
}) {
  return (
    <div inert={focused} aria-hidden={focused} data-with-header={offsetForChrome} className={`absolute bottom-4 left-4 max-w-[calc(100%-2rem)] z-20 flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2 rounded-lg ${GLASS_SOFT} text-xs text-text-tertiary ${chromeCls(focused, 'translate-y-3.5')}`}>
      {children}
    </div>
  )
}

export function StatValue({ children }: { children: React.ReactNode }) {
  return <strong className="text-text-secondary font-semibold">{children}</strong>
}

export function StatSeparator() {
  return <span className="opacity-40">·</span>
}

export function GraphHint({ focused, text }: { focused: boolean; text: string }) {
  return (
    <div className={`absolute bottom-6 right-6 z-10 text-[12px] text-text-tertiary [text-shadow:0_1px_8px_rgba(0,0,0,0.8)] pointer-events-none whitespace-nowrap hidden xl:block ${chromeCls(focused, 'translate-y-3.5')}`}>
      {text}
    </div>
  )
}

/** Keep the focus exit below the app topbar and reachable on touch screens. */
export function FocusToggle({ focused, onToggle }: { focused: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      data-graph-focus-toggle
      onClick={onToggle}
      className={`absolute right-4 top-4 z-40 flex items-center gap-2 h-9 px-3 rounded-md ${GLASS} border-border-primary cursor-pointer select-none transition-opacity duration-300 hover:!opacity-100 hover:border-border-primary ${FOCUS_RING}`}
      title="Shortcut: F or double-click the graph"
      aria-pressed={focused}
    >
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d={focused
          ? 'M9 4H4v5M15 4h5v5M9 20H4v-5M15 20h5v-5'
          : 'M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5'} />
      </svg>
      <span className="text-sm font-medium text-text-primary">{focused ? 'Show UI' : 'Focus'}</span>
      <span aria-hidden="true" className="hidden sm:inline text-xs text-text-tertiary border border-border-primary rounded-sm px-1.5 py-px">F</span>
    </button>
  )
}

export function FocusExitHint() {
  return (
    <div className="absolute bottom-6 left-1/2 -translate-x-1/2 z-20 h-7 flex items-center px-[18px] rounded-[17px] border border-border-primary bg-surface-primary text-[12.5px] text-text-tertiary pointer-events-none whitespace-nowrap">
      Focus mode — press <span className="text-text-primary font-semibold mx-[5px]">F</span> or double-click to exit
    </div>
  )
}

// ── Detail panel primitives ──────────────────────────────────────────────────

/** Floating rounded glass sheet used by both detail panels. */
export function GraphDetailPanel({ children }: { children: React.ReactNode }) {
  return (
    <div className="absolute right-3 top-[64px] bottom-3 w-[420px] max-w-[calc(100%-1.5rem)] z-[35] rounded-xl border border-border-primary bg-surface-elevated shadow-[-16px_0_50px_rgba(0,0,0,0.55)] flex flex-col overflow-hidden">
      {children}
    </div>
  )
}

/** Uppercase small label + value, per the design. */
export function DetailField({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-[5px]">
      <span className="text-[10.5px] font-bold tracking-[0.1em] text-text-tertiary">{label}</span>
      <span className="text-sm text-text-secondary leading-[1.6]">{value}</span>
    </div>
  )
}

/** Root container class for a full-bleed graph, honoring focus mode. */
export function graphRootClass(focused: boolean): string {
  return focused
    ? 'absolute inset-0 z-40 bg-background-primary overflow-hidden'
    : 'absolute inset-0 bg-background-primary overflow-hidden'
}

/** Canvas background, shared by both graphs. */
export const GRAPH_BG = '#07080c'
