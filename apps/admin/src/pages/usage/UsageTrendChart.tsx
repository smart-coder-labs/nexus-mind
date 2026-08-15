import { useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { UsageBucket, UsageBucketSize } from '../../types'
import { CHART_BRIGHT, CHART_DIM, CHART_GRID, CHART_SURFACE } from './chartColors'
import { bucketLabel, compactNumber, formatDuration } from './format'

export type TrendMetric = 'tokens' | 'duration' | 'events'

export interface UsageTrendChartProps {
  /** Gap-filled buckets, oldest first. Zero-value buckets are meaningful. */
  buckets: UsageBucket[]
  size: UsageBucketSize
  metric: TrendMetric
}

const PAD = { top: 14, right: 10, bottom: 26, left: 54 }
const HEIGHT = 268
/** Mark spec: bars are capped rather than filling their band — the leftover is air. */
const MAX_BAR_W = 24
/** The 2px surface gap that separates the two stacked segments. */
const STACK_GAP = 2

/**
 * Rounds a raw maximum up to a clean axis bound (1 / 2 / 5 × 10ⁿ), so the
 * ticks read as 0 / 500 / 1,000 rather than 0 / 437 / 874.
 */
function niceMax(raw: number): number {
  if (raw <= 0) return 1
  const mag = 10 ** Math.floor(Math.log10(raw))
  const norm = raw / mag
  const step = norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10
  return step * mag
}

/** Path for a bar with rounded top corners and a square base at the axis. */
function topRoundedPath(x: number, y: number, w: number, h: number, r: number): string {
  const rr = Math.max(0, Math.min(r, w / 2, h))
  const b = y + h
  return `M${x},${b} L${x},${y + rr} Q${x},${y} ${x + rr},${y} L${x + w - rr},${y} Q${x + w},${y} ${x + w},${y + rr} L${x + w},${b} Z`
}

/**
 * Width the chart draws at before/without a real measurement. A layout effect
 * measures ahead of the browser's first paint, so this is only ever seen where
 * there is no layout at all (jsdom) — but it means the chart degrades to a
 * reasonable size instead of rendering an empty box.
 */
const FALLBACK_WIDTH = 640

/** Measures the container so the SVG renders at real pixel size (no scaling). */
function useMeasuredWidth<T extends HTMLElement>() {
  const ref = useRef<T | null>(null)
  const [width, setWidth] = useState(0)

  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    setWidth(Math.floor(el.getBoundingClientRect().width))
    // Guarded for environments without ResizeObserver (jsdom in tests), matching
    // the pattern in components/OrgMemoryGraph.tsx.
    if (typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(entries => {
      const r = entries[0]?.contentRect
      if (r) setWidth(Math.floor(r.width))
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  return [ref, width] as const
}

const METRIC_LABEL: Record<TrendMetric, string> = {
  tokens: 'Tokens',
  duration: 'Execution time',
  events: 'Events',
}

function formatValue(metric: TrendMetric, v: number): string {
  return metric === 'duration' ? formatDuration(v) : v.toLocaleString()
}

function tickValue(metric: TrendMetric, v: number): string {
  return metric === 'duration' ? formatDuration(v) : compactNumber(v)
}

/**
 * The panel's lead chart: usage over time as columns.
 *
 * Columns rather than an area — daily token sums are discrete totals, and an
 * area fill would draw a continuous slope between two days that never happened.
 *
 * `tokens` renders as a two-segment stack (in below, out above) in two steps of
 * one hue; `duration` and `events` are single-series and carry no legend, since
 * the chart's own heading already names what is plotted.
 */
export function UsageTrendChart({ buckets, size, metric }: UsageTrendChartProps) {
  const [ref, measured] = useMeasuredWidth<HTMLDivElement>()
  const [active, setActive] = useState<number | null>(null)

  const width = measured || FALLBACK_WIDTH
  const plotW = Math.max(0, width - PAD.left - PAD.right)
  const plotH = HEIGHT - PAD.top - PAD.bottom
  const stacked = metric === 'tokens'

  const totals = useMemo(
    () =>
      buckets.map(b =>
        metric === 'tokens' ? b.tokens_total : metric === 'duration' ? b.duration_ms : b.event_count,
      ),
    [buckets, metric],
  )

  const max = useMemo(() => niceMax(Math.max(...totals, 0)), [totals])
  const ticks = useMemo(() => [0, 0.25, 0.5, 0.75, 1].map(f => max * f), [max])

  const band = buckets.length > 0 ? plotW / buckets.length : 0
  const barW = Math.max(2, Math.min(MAX_BAR_W, band * 0.62))
  const y = (v: number) => PAD.top + plotH - (v / max) * plotH

  // Label roughly every 90px so ticks never collide, always keeping the last.
  const labelStep = band > 0 ? Math.max(1, Math.ceil(90 / band)) : 1

  const activeBucket = active !== null ? buckets[active] : undefined

  const summary = useMemo(() => {
    const sum = totals.reduce((a, b) => a + b, 0)
    const n = buckets.length
    return `${METRIC_LABEL[metric]} by ${size}, ${n} ${n === 1 ? 'bucket' : 'buckets'}, ${formatValue(metric, sum)} total.`
  }, [totals, metric, size, buckets.length])

  return (
    <div className="relative" ref={ref}>
      {/* Legend — present whenever there are two series, never color alone. */}
      {stacked && (
        <div className="flex items-center gap-4 mb-2.5 pl-[54px]">
          {[
            { label: 'Tokens in', color: CHART_DIM },
            { label: 'Tokens out', color: CHART_BRIGHT },
          ].map(s => (
            <span key={s.label} className="flex items-center gap-1.5">
              <span
                className="w-2.5 h-2.5 rounded-[3px] shrink-0"
                style={{ backgroundColor: s.color }}
              />
              <span className="text-[11px] text-text-tertiary">{s.label}</span>
            </span>
          ))}
        </div>
      )}

      <svg
        width={width}
        height={HEIGHT}
        role="img"
        aria-label={summary}
        onMouseLeave={() => setActive(null)}
        style={{ display: 'block' }}
      >
        {/* Gridlines + y ticks. Hairline, solid, recessive. */}
        {ticks.map(t => (
          <g key={t}>
            <line
              x1={PAD.left}
              x2={width - PAD.right}
              y1={y(t)}
              y2={y(t)}
              stroke={CHART_GRID}
              strokeWidth={1}
            />
            <text
              x={PAD.left - 10}
              y={y(t) + 3.5}
              textAnchor="end"
              className="fill-text-quaternary"
              style={{ fontSize: 10, fontVariantNumeric: 'tabular-nums' }}
            >
              {tickValue(metric, t)}
            </text>
          </g>
        ))}

        {buckets.map((b, i) => {
          const cx = PAD.left + band * i + band / 2
          const x = cx - barW / 2
          const total = totals[i]
          const isActive = active === i

          return (
            <g key={b.bucket_ts}>
              {/* Hover band — a hit target far bigger than the mark itself. */}
              <rect
                x={PAD.left + band * i}
                y={PAD.top}
                width={Math.max(band, 1)}
                height={plotH}
                fill={isActive ? 'rgba(255,255,255,0.04)' : 'transparent'}
                onMouseEnter={() => setActive(i)}
              />

              {total > 0 &&
                (stacked ? (
                  (() => {
                    const hOut = (b.tokens_out / max) * plotH
                    const hIn = (b.tokens_in / max) * plotH
                    // The gap only exists when both segments are actually drawn.
                    const gap = hOut > 0 && hIn > 0 ? STACK_GAP : 0
                    const inTop = PAD.top + plotH - hIn
                    const outTop = inTop - gap - hOut
                    return (
                      <>
                        {hIn > 0 && (
                          <path
                            d={
                              hOut > 0
                                ? `M${x},${PAD.top + plotH} L${x},${inTop} L${x + barW},${inTop} L${x + barW},${PAD.top + plotH} Z`
                                : topRoundedPath(x, inTop, barW, hIn, 4)
                            }
                            fill={CHART_DIM}
                            opacity={active === null || isActive ? 1 : 0.45}
                          />
                        )}
                        {hOut > 0 && (
                          <path
                            d={topRoundedPath(x, outTop, barW, hOut, 4)}
                            fill={CHART_BRIGHT}
                            opacity={active === null || isActive ? 1 : 0.45}
                          />
                        )}
                      </>
                    )
                  })()
                ) : (
                  <path
                    d={topRoundedPath(x, y(total), barW, PAD.top + plotH - y(total), 4)}
                    fill={CHART_BRIGHT}
                    opacity={active === null || isActive ? 1 : 0.45}
                  />
                ))}

              {(i % labelStep === 0 || i === buckets.length - 1) && (
                <text
                  x={cx}
                  y={HEIGHT - 8}
                  textAnchor="middle"
                  className="fill-text-quaternary"
                  style={{ fontSize: 10 }}
                >
                  {bucketLabel(b.bucket_ts, size)}
                </text>
              )}
            </g>
          )
        })}

        {/* Baseline — the one axis line that stays. */}
        <line
          x1={PAD.left}
          x2={width - PAD.right}
          y1={PAD.top + plotH}
          y2={PAD.top + plotH}
          stroke="rgba(255,255,255,0.12)"
          strokeWidth={1}
        />
      </svg>

      {/* Tooltip. Follows the hovered band, clamped inside the plot. */}
      {activeBucket && (
        <div
          className="pointer-events-none absolute z-10 rounded-[10px] border border-white/[0.10] px-3 py-2 shadow-lg"
          style={{
            backgroundColor: CHART_SURFACE,
            left: Math.min(
              Math.max(PAD.left + band * (active as number) + band / 2 - 70, 0),
              Math.max(width - 140, 0),
            ),
            top: PAD.top + (stacked ? 26 : 4),
            width: 140,
          }}
        >
          <div className="text-[11px] font-semibold text-text-primary mb-1">
            {bucketLabel(activeBucket.bucket_ts, size)}
          </div>
          {stacked ? (
            <>
              <TooltipRow color={CHART_BRIGHT} label="Out" value={activeBucket.tokens_out.toLocaleString()} />
              <TooltipRow color={CHART_DIM} label="In" value={activeBucket.tokens_in.toLocaleString()} />
              <div className="mt-1 pt-1 border-t border-white/[0.08] flex items-center justify-between">
                <span className="text-[11px] text-text-tertiary">Total</span>
                <span className="text-[11px] font-semibold text-text-primary tabular-nums">
                  {activeBucket.tokens_total.toLocaleString()}
                </span>
              </div>
            </>
          ) : (
            <div className="flex items-center justify-between">
              <span className="text-[11px] text-text-tertiary">{METRIC_LABEL[metric]}</span>
              <span className="text-[11px] font-semibold text-text-primary tabular-nums">
                {formatValue(metric, metric === 'duration' ? activeBucket.duration_ms : activeBucket.event_count)}
              </span>
            </div>
          )}
          <div className="mt-0.5 text-[10px] text-text-quaternary">
            {activeBucket.event_count.toLocaleString()}{' '}
            {activeBucket.event_count === 1 ? 'event' : 'events'}
          </div>
        </div>
      )}

      {/* Table view — the non-visual path to the same numbers. */}
      <table className="sr-only">
        <caption>{summary}</caption>
        <thead>
          <tr>
            <th scope="col">Period</th>
            <th scope="col">{METRIC_LABEL[metric]}</th>
          </tr>
        </thead>
        <tbody>
          {buckets.map((b, i) => (
            <tr key={b.bucket_ts}>
              <th scope="row">{bucketLabel(b.bucket_ts, size)}</th>
              <td>{formatValue(metric, totals[i])}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function TooltipRow({ color, label, value }: { color: string; label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="flex items-center gap-1.5 min-w-0">
        <span className="w-2 h-2 rounded-[2px] shrink-0" style={{ backgroundColor: color }} />
        <span className="text-[11px] text-text-tertiary">{label}</span>
      </span>
      <span className="text-[11px] text-text-secondary tabular-nums">{value}</span>
    </div>
  )
}
