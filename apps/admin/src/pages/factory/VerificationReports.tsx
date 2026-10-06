import { ShieldCheck } from 'lucide-react'
import type { StoredVerificationReport, VerificationCheck } from '../../types'
import { cn } from '../../lib/utils'

/** `cmd:npm test` → `npm test`; `ci:build` → `CI · build`. */
function checkLabel(name: string): string {
  if (name.startsWith('cmd:')) return name.slice(4)
  if (name.startsWith('ci:')) return `CI · ${name.slice(3)}`
  return name
}

/** Status tokens only (DESIGN_DIRECTION §5 badge grammar): tint 10%, border 20%. */
const TONE = {
  success: 'bg-status-success/10 text-status-success border-status-success/20',
  error: 'bg-status-error/10 text-status-error border-status-error/20',
  warning: 'bg-status-warning/10 text-status-warning border-status-warning/20',
  neutral: 'bg-white/[0.06] text-text-secondary border-white/[0.09]',
} as const

const CHECK: Record<VerificationCheck['status'], { label: string; tone: keyof typeof TONE }> = {
  PASS: { label: 'Passed', tone: 'success' },
  FAIL: { label: 'Failed', tone: 'error' },
  ERROR: { label: 'Errored', tone: 'warning' },
  SKIP: { label: 'Skipped', tone: 'neutral' },
}

const PILL = 'inline-flex h-5 shrink-0 items-center justify-center rounded-full border px-2 text-[11px] font-medium'

/** Merge eligibility is only meaningful when the report includes CI checks (the
 *  merge path's report); a run's own report covers the sandbox commands. */
function verdict(report: StoredVerificationReport['report']): string {
  const hasCi = report.checks.some(check => check.name.startsWith('ci:'))
  if (!hasCi) return report.passed ? 'Sandbox commands only' : 'Sandbox commands failed'
  return report.eligible_for_merge ? 'Eligible for merge' : 'Needs a human before merge'
}

function seconds(ms?: number): string | null {
  return typeof ms === 'number' ? `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s` : null
}

/**
 * The F1 verification gate reports of one run: the commands run in the sandbox
 * and the required CI checks, for the exact head they apply to. Rendered only when
 * the run produced evidence.
 */
export function VerificationReports({ reports }: { reports: StoredVerificationReport[] }) {
  if (!reports.length) return null
  return (
    <section aria-labelledby="verification-title" className="space-y-3">
      <h3 id="verification-title" className="flex items-center gap-1.5 text-[13px] font-semibold text-text-primary">
        <ShieldCheck className="h-4 w-4 text-text-secondary" aria-hidden="true" />Verification
      </h3>
      {reports.map(({ head_sha, created_at, report }) => (
        <div key={`${head_sha}-${created_at}`} className="space-y-3 rounded-[11px] border border-border-primary p-3.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[13px]">
            <span className={cn(PILL, report.passed ? TONE.success : TONE.error)}>{report.passed ? 'All checks passed' : 'Blocked'}</span>
            <span className="text-text-secondary">{verdict(report)}</span>
            <span className="text-[12px] text-text-tertiary">on commit</span>
            <code className="font-mono text-[12px] text-text-secondary">{head_sha.slice(0, 12)}</code>
          </div>
          <ul className="m-0 list-none space-y-1.5 p-0">
            {report.checks.map((check, index) => (
              <li key={`${index}-${check.name}`} className="flex items-center gap-2.5 text-[13px]">
                <span className={cn(PILL, 'w-16', TONE[CHECK[check.status].tone])}>{CHECK[check.status].label}</span>
                <span className="min-w-0 break-all font-mono text-[12px] text-text-primary">{checkLabel(check.name)}</span>
                {seconds(check.duration_ms) && <span className="ml-auto shrink-0 text-[12px] tabular-nums text-text-tertiary">{seconds(check.duration_ms)}</span>}
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  )
}
