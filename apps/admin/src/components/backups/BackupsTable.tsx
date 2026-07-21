import { ChevronDown, ChevronRight, Database, Download, RefreshCw, Table2 } from 'lucide-react'
import { formatDistanceToNow } from 'date-fns'
import type { Backup, BackupDetail, BackupTableInfo } from '../../types'
import { BackupStatusBadge } from './BackupStatusBadge'

const FOCUS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

export function formatBytes(n: number | null | undefined): string {
  if (n == null || Number.isNaN(n)) return '—'
  if (n < 1024) return `${n} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = n / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`
}

export function formatNumber(n: number | null | undefined): string {
  if (n == null || Number.isNaN(n)) return '—'
  return n.toLocaleString()
}

interface BackupsTableProps {
  backups: Backup[]
  loading: boolean
  downloadingId: string | null
  expandedId: string | null
  detailCache: Record<string, BackupDetail | undefined>
  detailLoading: Record<string, boolean | undefined>
  detailError: Record<string, string | undefined>
  onToggleExpand: (id: string) => void
  onDownload: (id: string) => void
  onRestore: (b: Backup) => void
}

function TableList({ tables }: { tables: BackupTableInfo[] }) {
  if (tables.length === 0) {
    return (
      <p className="text-[12px] text-text-quaternary italic px-4 py-2">No tables in this backup.</p>
    )
  }
  return (
    <table className="w-full text-[12px]">
      <thead>
        <tr className="text-text-quaternary">
          <th className="px-4 py-1.5 text-left font-medium uppercase tracking-wide text-[10px]">Table</th>
          <th className="px-4 py-1.5 text-right font-medium uppercase tracking-wide text-[10px]">Rows</th>
        </tr>
      </thead>
      <tbody>
        {tables.map(t => (
          <tr key={t.table_name} className="border-t border-border-secondary/40">
            <td className="px-4 py-1.5 text-text-secondary font-mono">{t.table_name}</td>
            <td className="px-4 py-1.5 text-text-secondary text-right tabular-nums">{formatNumber(t.row_count)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function ExpandedPanel({
  detail,
  loading,
  error,
}: {
  detail: BackupDetail | undefined
  loading: boolean
  error: string | undefined
}) {
  if (loading) {
    return (
      <div className="px-4 py-3 text-[12px] text-text-quaternary flex items-center gap-2">
        <RefreshCw className="w-3 h-3 animate-spin" />
        Loading tables…
      </div>
    )
  }
  if (error) {
    return <div className="px-4 py-3 text-[12px] text-status-error">{error}</div>
  }
  if (!detail) return null
  return <TableList tables={detail.table_list} />
}

export function BackupsTable({
  backups,
  loading,
  downloadingId,
  expandedId,
  detailCache,
  detailLoading,
  detailError,
  onToggleExpand,
  onDownload,
  onRestore,
}: BackupsTableProps) {
  if (!loading && backups.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] rounded-[18px] px-6">
        <Database className="w-8 h-8 text-text-quaternary mb-3" />
        <p className="text-[15px] font-semibold text-text-secondary">No backups yet</p>
        <p className="text-[13px] text-text-quaternary mt-1 max-w-xs">
          Create a manual backup or wait for the next scheduled one.
        </p>
      </div>
    )
  }

  return (
    <div className="rounded-[18px] border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] overflow-hidden">
      <div className="overflow-x-auto">
        <table className="w-full text-[13px]">
          <thead>
            <tr className="border-b border-white/[0.07] bg-white/[0.03]">
              <th className="w-8 px-3 py-3" aria-label="Expand" />
              <th className="px-4 py-3 text-left text-[12px] font-medium text-text-tertiary uppercase tracking-wider">Created</th>
              <th className="px-4 py-3 text-left text-[12px] font-medium text-text-tertiary uppercase tracking-wider">Kind</th>
              <th className="px-4 py-3 text-left text-[12px] font-medium text-text-tertiary uppercase tracking-wider">Status</th>
              <th className="px-4 py-3 text-right text-[12px] font-medium text-text-tertiary uppercase tracking-wider">Size</th>
              <th className="px-4 py-3 text-right text-[12px] font-medium text-text-tertiary uppercase tracking-wider">Actions</th>
            </tr>
          </thead>
          <tbody>
            {loading && Array.from({ length: 4 }).map((_, i) => (
              <tr key={`skel-${i}`} className="border-b border-border-primary last:border-0">
                <td className="px-3 py-3" />
                <td className="px-4 py-3"><div className="animate-pulse h-3.5 bg-white/[0.04] rounded-[8px] w-28" /></td>
                <td className="px-4 py-3"><div className="animate-pulse h-3.5 bg-white/[0.04] rounded-[8px] w-16" /></td>
                <td className="px-4 py-3"><div className="animate-pulse h-5 bg-white/[0.04] rounded-full w-20" /></td>
                <td className="px-4 py-3"><div className="animate-pulse h-3.5 bg-white/[0.04] rounded-[8px] w-16 ml-auto" /></td>
                <td className="px-4 py-3"><div className="animate-pulse h-6 bg-white/[0.04] rounded-[8px] w-32 ml-auto" /></td>
              </tr>
            ))}

            {backups.map(b => {
              const isExpanded = expandedId === b.id
              const isInProgress = b.status === 'pending' || b.status === 'running'
              const isDisabled = isInProgress
              return (
                <Row
                  key={b.id}
                  backup={b}
                  isExpanded={isExpanded}
                  isDownloading={downloadingId === b.id}
                  detail={detailCache[b.id]}
                  detailLoading={!!detailLoading[b.id]}
                  detailError={detailError[b.id]}
                  isDisabled={isDisabled}
                  onToggleExpand={() => onToggleExpand(b.id)}
                  onDownload={() => onDownload(b.id)}
                  onRestore={() => onRestore(b)}
                />
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function Row({
  backup,
  isExpanded,
  isDownloading,
  detail,
  detailLoading,
  detailError,
  isDisabled,
  onToggleExpand,
  onDownload,
  onRestore,
}: {
  backup: Backup
  isExpanded: boolean
  isDownloading: boolean
  detail: BackupDetail | undefined
  detailLoading: boolean
  detailError: string | undefined
  isDisabled: boolean
  onToggleExpand: () => void
  onDownload: () => void
  onRestore: () => void
}) {
  return (
    <>
      <tr className="border-b border-border-primary last:border-0 hover:bg-accent-blue/[0.05] transition-colors">
        <td className="px-3 py-3 align-top">
          <button
            onClick={onToggleExpand}
            aria-label={isExpanded ? 'Collapse tables' : 'Expand tables'}
            aria-expanded={isExpanded}
            className={`p-1 rounded-[6px] text-text-tertiary hover:text-text-primary hover:bg-white/[0.06] transition-colors ${FOCUS}`}
          >
            {isExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
          </button>
        </td>
        <td className="px-4 py-3 align-top">
          <div className="text-[13px] text-text-primary font-mono">
            {new Date(backup.created_at).toLocaleString()}
          </div>
          <div className="text-[11px] text-text-quaternary mt-0.5">
            {formatDistanceToNow(new Date(backup.created_at), { addSuffix: true })}
          </div>
        </td>
        <td className="px-4 py-3 align-top">
          <span className="text-[12px] text-text-secondary capitalize">{backup.kind}</span>
        </td>
        <td className="px-4 py-3 align-top">
          <BackupStatusBadge status={backup.status} />
        </td>
        <td className="px-4 py-3 align-top text-right tabular-nums text-text-secondary">
          {formatBytes(backup.size_bytes)}
        </td>
        <td className="px-4 py-3 align-top">
          <div className="flex items-center justify-end gap-1.5">
            <button
              onClick={onToggleExpand}
              aria-label="View tables"
              title="View tables"
              className={`inline-flex items-center gap-1 rounded-[8px] border border-border-primary px-2.5 py-1 text-[12px] text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors ${FOCUS}`}
            >
              <Table2 className="w-3 h-3" />
              Tables
            </button>
            <button
              onClick={onDownload}
              disabled={isDownloading || isDisabled}
              aria-label="Download backup"
              title="Download backup JSON"
              className={`inline-flex items-center gap-1 rounded-[8px] border border-border-primary px-2.5 py-1 text-[12px] text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors disabled:opacity-40 ${FOCUS}`}
            >
              <Download className="w-3 h-3" />
              {isDownloading ? '…' : 'Download'}
            </button>
            <button
              onClick={onRestore}
              disabled={isDisabled}
              aria-label="Restore from backup"
              title="Restore database"
              className={`inline-flex items-center gap-1 rounded-[8px] border border-status-error/30 px-2.5 py-1 text-[12px] text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-40 ${FOCUS}`}
            >
              <RefreshCw className="w-3 h-3" />
              Restore
            </button>
          </div>
        </td>
      </tr>
      {isExpanded && (
        <tr className="border-b border-border-primary last:border-0 bg-white/[0.02]">
          <td colSpan={6} className="px-2 py-2">
            <ExpandedPanel detail={detail} loading={detailLoading} error={detailError} />
          </td>
        </tr>
      )}
    </>
  )
}
