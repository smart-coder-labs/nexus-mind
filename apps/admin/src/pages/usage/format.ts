import type { UsageBucket, UsageBucketSize } from '../../types'

/**
 * Formats a millisecond duration into a compact human string:
 *   820        → "820ms"
 *   4200       → "4.2s"
 *   63000      → "1m 3s"
 *   3_780_000  → "1h 3m"
 * Zero / negative collapses to "0ms".
 */
export function formatDuration(ms: number): string {
  if (!ms || ms <= 0) return '0ms'
  if (ms < 1000) return `${Math.round(ms)}ms`

  const totalSec = ms / 1000
  if (totalSec < 60) {
    // One decimal place, trimming a trailing ".0" (e.g. "4.2s", "12s").
    const s = Math.round(totalSec * 10) / 10
    return `${Number.isInteger(s) ? s : s.toFixed(1)}s`
  }

  const totalMin = Math.floor(totalSec / 60)
  const hours = Math.floor(totalMin / 60)
  const mins = totalMin % 60
  if (hours > 0) return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`

  const secs = Math.floor(totalSec % 60)
  return secs > 0 ? `${totalMin}m ${secs}s` : `${totalMin}m`
}

/**
 * Axis-tick / stat-tile number shortening: 1284 → "1.3K", 4_200_000 → "4.2M".
 * Values under 1000 keep their exact digits — rounding them away loses the
 * only precision small numbers have.
 */
export function compactNumber(n: number): string {
  const abs = Math.abs(n)
  if (abs < 1000) return String(Math.round(n))
  if (abs < 1_000_000) {
    const v = n / 1000
    return `${Math.abs(v) < 10 ? v.toFixed(1).replace(/\.0$/, '') : Math.round(v)}K`
  }
  if (abs < 1_000_000_000) {
    const v = n / 1_000_000
    return `${Math.abs(v) < 10 ? v.toFixed(1).replace(/\.0$/, '') : Math.round(v)}M`
  }
  const v = n / 1_000_000_000
  return `${Math.abs(v) < 10 ? v.toFixed(1).replace(/\.0$/, '') : Math.round(v)}B`
}

// ── Date arithmetic on the wire format ───────────────────────────────────────
//
// `bucket_ts` and the date-range filters are naive `YYYY-MM-DD` strings that the
// backend derives from SQLite's `datetime('now')`, i.e. UTC. Everything below
// therefore does its arithmetic in UTC and never hands a bare date string to
// `new Date(...)`, whose date-only parsing is UTC but whose *getters* are local
// — the combination silently shifts a day for anyone west of Greenwich.

/** `YYYY-MM-DD` → epoch ms at UTC midnight. Returns NaN for a malformed input. */
function isoToUtcMs(iso: string): number {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso)
  if (!m) return NaN
  return Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]))
}

const DAY_MS = 86_400_000

/** Epoch ms → `YYYY-MM-DD`, read in UTC. */
function utcMsToIso(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10)
}

/** Adds `days` to a `YYYY-MM-DD` string, staying in UTC. */
export function addDaysIso(iso: string, days: number): string {
  return utcMsToIso(isoToUtcMs(iso) + days * DAY_MS)
}

/** Today as `YYYY-MM-DD` in UTC — the frame the backend timestamps in. */
export function todayIso(): string {
  return new Date().toISOString().slice(0, 10)
}

/** Whole days between two `YYYY-MM-DD` strings (`to - from`). */
export function daysBetween(from: string, to: string): number {
  return Math.round((isoToUtcMs(to) - isoToUtcMs(from)) / DAY_MS)
}

/** The Monday on or before `iso`, matching the backend's week anchor. */
function mondayOf(iso: string): string {
  const ms = isoToUtcMs(iso)
  // getUTCDay: 0=Sun … 6=Sat. Monday-anchored offset: Sun walks back 6 days.
  const dow = new Date(ms).getUTCDay()
  return utcMsToIso(ms - ((dow + 6) % 7) * DAY_MS)
}

const EMPTY_BUCKET = {
  tokens_in: 0,
  tokens_out: 0,
  tokens_total: 0,
  duration_ms: 0,
  event_count: 0,
} as const

/**
 * Expands the backend's sparse buckets into one entry per slot across
 * `[from, to]`, filling gaps with zeroes.
 *
 * This matters for honesty, not cosmetics: a trend chart that silently omits
 * the days with no usage compresses the x-axis and makes idle stretches look
 * like continuous activity. A zero day is data.
 *
 * `hour` is not expanded — an hourly range can span thousands of slots, so the
 * backend's own buckets are returned as-is and the chart plots what exists.
 */
export function fillBuckets(
  buckets: UsageBucket[],
  size: UsageBucketSize,
  from: string,
  to: string,
): UsageBucket[] {
  if (size === 'hour' || !from || !to) return buckets

  const span = daysBetween(from, to)
  if (!Number.isFinite(span) || span < 0) return buckets

  const byTs = new Map(buckets.map(b => [b.bucket_ts, b]))
  const slots: string[] = []

  if (size === 'week') {
    for (let ts = mondayOf(from); daysBetween(ts, to) >= 0; ts = addDaysIso(ts, 7)) {
      slots.push(ts)
    }
  } else {
    // Guard against a pathological range turning into an unbounded loop.
    const count = Math.min(span, 730)
    for (let i = 0; i <= count; i++) slots.push(addDaysIso(from, i))
  }

  return slots.map(ts => byTs.get(ts) ?? { bucket_ts: ts, ...EMPTY_BUCKET })
}

/** Short axis label for a bucket key: "Aug 14", "Aug 14 09:00", "Wk Aug 10". */
export function bucketLabel(ts: string, size: UsageBucketSize): string {
  const date = new Date(`${ts.slice(0, 10)}T00:00:00Z`)
  if (Number.isNaN(date.getTime())) return ts
  const day = date.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  })
  if (size === 'hour') return `${day} ${ts.slice(11, 13)}:00`
  if (size === 'week') return `Wk ${day}`
  return day
}
