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
export const GLASS = 'border border-white/[0.09] bg-[#0d0f14]/[0.72] backdrop-blur-[14px]'
export const GLASS_SOFT = 'border border-white/[0.08] bg-[#0d0f14]/[0.66] backdrop-blur-[12px]'

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
    <div className={`absolute inset-x-0 top-0 z-20 flex items-start justify-between gap-4 px-6 lg:pl-[292px] pt-5 pr-[150px] pointer-events-none ${chromeCls(focused, '-translate-y-3.5')}`}>
      <div className="pointer-events-auto min-w-0 [text-shadow:0_2px_16px_rgba(0,0,0,0.8)]">
        {title && (
          <h1 className="text-[28px] font-extrabold tracking-[-0.02em] leading-[1.15] text-[#f4f6fa]">{title}</h1>
        )}
        {subtitle && (
          <p className="text-[13px] text-[#98a0b1] mt-1 max-w-[560px]">{subtitle}</p>
        )}
      </div>
      <div className="pointer-events-auto flex items-center gap-2 shrink-0 flex-wrap justify-end">
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
      className={`flex items-center h-[42px] px-1 gap-1 rounded-[11px] ${GLASS}`}
    >
      {tabs.map(tab => {
        const active = tab.id === value
        return (
          <button
            key={tab.id}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(tab.id)}
            className={`h-[34px] px-3.5 rounded-[8px] text-[13px] font-semibold transition-colors cursor-pointer ${FOCUS_RING} ${
              active
                ? 'bg-white/[0.10] text-[#f4f6fa]'
                : 'text-[#8b93a5] hover:text-[#dde1e9]'
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
    <div className={`flex items-center gap-2 h-[42px] px-3.5 rounded-[11px] ${GLASS} min-w-[150px] max-w-[280px]`}>
      <Search className="w-[15px] h-[15px] text-[#5b6373] shrink-0" aria-hidden="true" />
      <input
        type="text"
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label="Search graph nodes"
        className="flex-1 min-w-0 bg-transparent border-none outline-none text-[13px] text-[#e7eaf0] placeholder:text-[#5b6373]"
      />
      {active && (
        <span className="shrink-0 text-[11px] font-bold text-[#7aa2ff]" aria-label={`${count} matching nodes`}>
          {count >= maxMatches ? `${maxMatches}+` : count}
        </span>
      )}
    </div>
  )
}

/** Glass `<select>` used for the project / repository pickers. */
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
    <div className="relative">
      <select
        value={value}
        onChange={e => onChange(e.target.value)}
        disabled={disabled}
        aria-label={ariaLabel}
        className={`appearance-none h-[42px] ${GLASS} rounded-[11px] pl-3.5 pr-9 text-[13.5px] text-[#dde1e9] focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer disabled:opacity-50 ${FOCUS_RING}`}
      >
        <option value="">{placeholder}</option>
        {options.map(o => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-[#7c8496]" />
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
    <div className="relative">
      <button
        type="button"
        onClick={() => onOpenChange(!open)}
        className={`flex items-center justify-center w-[42px] h-[42px] rounded-[11px] ${GLASS} text-[#9aa2b2] hover:text-[#e7eaf0] hover:border-white/[0.18] transition-colors ${FOCUS_RING}`}
        aria-label="Graph settings"
        aria-expanded={open}
      >
        <Settings2 className="w-4 h-4" />
      </button>
      {open && (
        <div className={`absolute right-0 top-[48px] w-[220px] rounded-[12px] ${GLASS} shadow-[0_12px_40px_rgba(0,0,0,0.45)] p-3 space-y-1`}>
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
      className="w-full flex items-center justify-between gap-3 px-2 py-2 rounded-[8px] hover:bg-white/[0.05] transition-colors text-left"
    >
      <span className="min-w-0">
        <span className="block text-[12.5px] font-semibold text-[#e7eaf0]">{label}</span>
        <span className="block text-[11px] text-[#7c8496]">{description}</span>
      </span>
      <span
        className={`shrink-0 w-[34px] h-[20px] rounded-full p-[2px] transition-colors ${checked ? 'bg-accent-blue' : 'bg-white/[0.12]'}`}
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
    <div className={`absolute left-6 z-20 flex flex-col gap-[9px] pointer-events-none ${offsetForChrome ? 'top-[108px] lg:left-[292px]' : 'top-5'} ${chromeCls(focused, '-translate-y-3.5')}`}>
      {children}
    </div>
  )
}

export function GraphChipRow({
  children,
  ...rest
}: { children: React.ReactNode } & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className="pointer-events-auto flex items-center gap-2 flex-wrap max-w-[72vw]" {...rest}>
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
      className="flex items-center h-[28px] px-[13px] rounded-[14px] text-[12px] font-semibold cursor-pointer transition-opacity hover:brightness-[1.15]"
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
      className={`flex items-center gap-[7px] h-[28px] px-[13px] rounded-[14px] ${GLASS_SOFT} text-[12px] font-semibold text-[#9aa2b2] hover:text-[#e7eaf0] hover:border-white/30 transition-colors`}
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
    <div className={`absolute bottom-5 left-6 ${offsetForChrome ? 'lg:left-[292px]' : ''} z-20 flex items-center gap-3.5 h-[36px] px-4 rounded-[18px] ${GLASS_SOFT} text-[12.5px] text-[#8b93a5] whitespace-nowrap ${chromeCls(focused, 'translate-y-3.5')}`}>
      {children}
    </div>
  )
}

export function StatValue({ children }: { children: React.ReactNode }) {
  return <strong className="text-[#c9cfda] font-semibold">{children}</strong>
}

export function StatSeparator() {
  return <span className="opacity-40">·</span>
}

export function GraphHint({ focused, text }: { focused: boolean; text: string }) {
  return (
    <div className={`absolute bottom-6 right-6 z-10 text-[12px] text-[#646c7d] [text-shadow:0_1px_8px_rgba(0,0,0,0.8)] pointer-events-none whitespace-nowrap hidden md:block ${chromeCls(focused, 'translate-y-3.5')}`}>
      {text}
    </div>
  )
}

/** Focus toggle — always visible; dims to 55% while focused (design). */
export function FocusToggle({ focused, onToggle }: { focused: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className={`absolute right-6 top-5 z-30 flex items-center gap-2 h-[42px] px-4 rounded-[11px] ${GLASS} border-white/[0.12] cursor-pointer select-none transition-opacity duration-300 hover:!opacity-100 hover:border-white/[0.28] ${FOCUS_RING}`}
      style={{ opacity: focused ? 0.55 : 1 }}
      title="Shortcut: F or double-click the graph"
      aria-pressed={focused}
    >
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#dde1e9" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d={focused
          ? 'M9 4H4v5M15 4h5v5M9 20H4v-5M15 20h5v-5'
          : 'M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5'} />
      </svg>
      <span className="text-[13.5px] font-semibold text-[#dde1e9]">{focused ? 'Show UI' : 'Focus'}</span>
      <span className="text-[11px] text-[#6b7384] border border-white/[0.14] rounded-[5px] px-1.5 py-px">F</span>
    </button>
  )
}

export function FocusExitHint() {
  return (
    <div className="absolute bottom-6 left-1/2 -translate-x-1/2 z-20 h-[34px] flex items-center px-[18px] rounded-[17px] border border-white/[0.08] bg-[#0d0f14]/60 backdrop-blur-[12px] text-[12.5px] text-[#8b93a5] pointer-events-none whitespace-nowrap">
      Focus mode — press <span className="text-[#dde1e9] font-semibold mx-[5px]">F</span> or double-click to exit
    </div>
  )
}

// ── Detail panel primitives ──────────────────────────────────────────────────

/** Floating rounded glass sheet used by both detail panels. */
export function GraphDetailPanel({ children }: { children: React.ReactNode }) {
  return (
    <div className="absolute right-3 top-[76px] bottom-3 w-[420px] max-w-[calc(100vw-320px)] z-[35] rounded-[16px] border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] shadow-[-16px_0_50px_rgba(0,0,0,0.55)] flex flex-col overflow-hidden">
      {children}
    </div>
  )
}

/** Uppercase small label + value, per the design. */
export function DetailField({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-[5px]">
      <span className="text-[10.5px] font-bold tracking-[0.1em] text-[#5b6373]">{label}</span>
      <span className="text-[13px] text-[#cfd4de] leading-[1.6]">{value}</span>
    </div>
  )
}

/** Root container class for a full-bleed graph, honoring focus mode. */
export function graphRootClass(focused: boolean): string {
  return focused
    ? 'fixed inset-0 z-[100] bg-[#07080c] overflow-hidden'
    : 'absolute inset-0 bg-[#07080c] overflow-hidden'
}

/** Canvas background, shared by both graphs. */
export const GRAPH_BG = '#07080c'
