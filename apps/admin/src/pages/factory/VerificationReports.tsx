import { ShieldCheck } from 'lucide-react'
import type { StoredVerificationReport, VerificationCheck } from '../../types'

/** `cmd:npm test` → `npm test`; `ci:build` → `CI · build`. */
function checkLabel(name: string): string {
  if (name.startsWith('cmd:')) return name.slice(4)
  if (name.startsWith('ci:')) return `CI · ${name.slice(3)}`
  return name
}

const STATUS_STYLE: Record<VerificationCheck['status'], string> = {
  PASS: 'text-emerald-400 border-emerald-400/40',
  FAIL: 'text-red-400 border-red-400/40',
  ERROR: 'text-amber-400 border-amber-400/40',
  SKIP: 'text-text-tertiary border-border-primary',
}

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
      <p id="verification-title" className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">
        <ShieldCheck className="w-3.5 h-3.5" />Verification
      </p>
      {reports.map(({ head_sha, created_at, report }) => (
        <div key={`${head_sha}-${created_at}`} className="rounded-lg border border-border-primary p-3 space-y-2">
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <span className={`rounded border px-1.5 py-0.5 font-medium ${report.passed ? STATUS_STYLE.PASS : STATUS_STYLE.FAIL}`}>
              {report.passed ? 'Passed' : 'Blocked'}
            </span>
            <span className="text-text-tertiary">head</span>
            <code className="font-mono text-text-secondary">{head_sha.slice(0, 12)}</code>
            <span className="text-text-tertiary">· {verdict(report)}</span>
          </div>
          <ul className="m-0 p-0 list-none space-y-1">
            {report.checks.map((check, index) => (
              <li key={`${index}-${check.name}`} className="flex items-center gap-2 text-xs">
                <span className={`w-12 shrink-0 rounded border px-1 text-center font-mono text-[10px] ${STATUS_STYLE[check.status]}`}>{check.status}</span>
                <span className="font-mono text-text-primary break-all">{checkLabel(check.name)}</span>
                {seconds(check.duration_ms) && <span className="ml-auto shrink-0 text-text-tertiary">{seconds(check.duration_ms)}</span>}
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  )
}
