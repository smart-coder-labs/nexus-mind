import { describe, expect, it } from 'vitest'
import { addDaysIso, bucketLabel, compactNumber, daysBetween, fillBuckets, formatDuration } from './format'
import type { UsageBucket } from '../../types'

function bucket(bucket_ts: string, tokens_total = 10): UsageBucket {
  return {
    bucket_ts,
    tokens_in: tokens_total,
    tokens_out: 0,
    tokens_total,
    duration_ms: 0,
    event_count: 1,
  }
}

describe('date helpers', () => {
  it('adds days without drifting across a DST boundary', () => {
    // US DST ends 2026-11-01. A local-time implementation lands on Oct 31.
    expect(addDaysIso('2026-10-31', 1)).toBe('2026-11-01')
    expect(addDaysIso('2026-11-01', 1)).toBe('2026-11-02')
    // And across a year end.
    expect(addDaysIso('2026-12-31', 1)).toBe('2027-01-01')
    expect(addDaysIso('2027-01-01', -1)).toBe('2026-12-31')
  })

  it('measures whole days between two dates', () => {
    expect(daysBetween('2026-08-01', '2026-08-31')).toBe(30)
    expect(daysBetween('2026-08-14', '2026-08-14')).toBe(0)
  })
})

describe('fillBuckets', () => {
  it('inserts zero buckets for days the backend omitted', () => {
    const filled = fillBuckets([bucket('2026-08-12'), bucket('2026-08-15')], 'day', '2026-08-12', '2026-08-15')

    expect(filled.map(b => b.bucket_ts)).toEqual([
      '2026-08-12', '2026-08-13', '2026-08-14', '2026-08-15',
    ])
    // The idle days are real zeroes, not dropped points.
    expect(filled[1].tokens_total).toBe(0)
    expect(filled[1].event_count).toBe(0)
    expect(filled[3].tokens_total).toBe(10)
  })

  it('covers the whole requested range even when data starts late', () => {
    const filled = fillBuckets([bucket('2026-08-20')], 'day', '2026-08-18', '2026-08-21')
    expect(filled).toHaveLength(4)
    expect(filled[0].bucket_ts).toBe('2026-08-18')
    expect(filled[0].tokens_total).toBe(0)
  })

  it('anchors week slots on Monday, matching the backend bucket key', () => {
    // 2026-08-10 and 2026-08-17 are Mondays; the range starts mid-week.
    const filled = fillBuckets([bucket('2026-08-17', 5)], 'week', '2026-08-12', '2026-08-20')
    expect(filled.map(b => b.bucket_ts)).toEqual(['2026-08-10', '2026-08-17'])
    expect(filled[1].tokens_total).toBe(5)
  })

  it('leaves hourly buckets untouched — an hour range can span thousands of slots', () => {
    const raw = [bucket('2026-08-12 09'), bucket('2026-08-12 15')]
    expect(fillBuckets(raw, 'hour', '2026-08-12', '2026-08-12')).toBe(raw)
  })

  it('returns the raw buckets when the range is unbounded', () => {
    const raw = [bucket('2026-08-12')]
    expect(fillBuckets(raw, 'day', '', '')).toBe(raw)
  })
})

describe('bucketLabel', () => {
  it('reads the date in UTC, the frame the backend timestamps in', () => {
    // A local-time reading shows "Aug 11" for anyone west of Greenwich.
    expect(bucketLabel('2026-08-12', 'day')).toBe('Aug 12')
    expect(bucketLabel('2026-08-12 09', 'hour')).toBe('Aug 12 09:00')
    expect(bucketLabel('2026-08-10', 'week')).toBe('Wk Aug 10')
  })
})

describe('formatters', () => {
  it('formats durations across the unit boundaries', () => {
    expect(formatDuration(0)).toBe('0ms')
    expect(formatDuration(820)).toBe('820ms')
    expect(formatDuration(4200)).toBe('4.2s')
    expect(formatDuration(63000)).toBe('1m 3s')
    expect(formatDuration(3_780_000)).toBe('1h 3m')
  })

  it('keeps small numbers exact and shortens large ones', () => {
    expect(compactNumber(999)).toBe('999')
    expect(compactNumber(1284)).toBe('1.3K')
    expect(compactNumber(42_000)).toBe('42K')
    expect(compactNumber(4_200_000)).toBe('4.2M')
  })
})
