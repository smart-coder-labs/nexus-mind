import { useCallback, useMemo, useRef, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, X, CheckCircle2, AlertCircle, Loader2 } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { Backup, BackupDetail, BackupRestoreSummary } from '../types'
import { BackupsTable, formatBytes } from '../components/backups/BackupsTable'
import { RestoreConfirmDialog } from '../components/backups/RestoreConfirmDialog'

const FOCUS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

type Flash =
  | { kind: 'success'; message: string }
  | { kind: 'error'; message: string }
  | null

const FLASH_TIMEOUT_MS = 5000

export default function Backups() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()

  const [expandedId, setExpandedId] = useState<string | null>(null)
  const [detailCache, setDetailCache] = useState<Record<string, BackupDetail | undefined>>({})
  const [detailLoading, setDetailLoading] = useState<Record<string, boolean | undefined>>({})
  const [detailError, setDetailError] = useState<Record<string, string | undefined>>({})
  const [downloadingId, setDownloadingId] = useState<string | null>(null)
  const [restoreTarget, setRestoreTarget] = useState<Backup | null>(null)
  const [flash, setFlash] = useState<Flash>(null)
  const flashTimerRef = useRef<number | null>(null)

  const orgSlug = session?.org.slug ?? ''

  const showFlash = useCallback((next: Flash) => {
    if (flashTimerRef.current) {
      window.clearTimeout(flashTimerRef.current)
      flashTimerRef.current = null
    }
    setFlash(next)
    if (next) {
      flashTimerRef.current = window.setTimeout(() => {
        setFlash(null)
        flashTimerRef.current = null
      }, FLASH_TIMEOUT_MS)
    }
  }, [])

  const {
    data: backups = [],
    isLoading,
    error: listError,
    refetch,
  } = useQuery<Backup[]>({
    queryKey: ['backups'],
    queryFn: () => client.listBackups(),
    refetchInterval: 30_000,
    refetchIntervalInBackground: false,
    retry: 0,
  })

  const listErrorMessage = listError
    ? (listError instanceof Error ? listError.message : 'Failed to load backups')
    : null

  // ── Create ──────────────────────────────────────────────────────────────────
  const createMut = useMutation({
    mutationFn: () => client.createBackup(),
    onSuccess: (created) => {
      qc.invalidateQueries({ queryKey: ['backups'] })
      showFlash({
        kind: 'success',
        message:
          created.status === 'completed'
            ? `Backup ${created.id.slice(0, 8)}… created (${formatBytes(created.size_bytes)}).`
            : `Backup ${created.id.slice(0, 8)}… queued. The list will refresh automatically.`,
      })
    },
    onError: (err) => {
      showFlash({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Failed to create backup.',
      })
    },
  })

  // ── Restore ─────────────────────────────────────────────────────────────────
  const restoreMut = useMutation<BackupRestoreSummary, Error, string>({
    mutationFn: (id) => client.restoreBackup(id),
    onSuccess: (summary) => {
      showFlash({
        kind: 'success',
        message: `Restore complete — ${summary.tables_restored} tables / ${summary.rows_restored.toLocaleString()} rows.`,
      })
      qc.invalidateQueries({ queryKey: ['backups'] })
    },
    onError: (err) => {
      showFlash({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Failed to restore backup.',
      })
    },
  })

  // ── Expand ──────────────────────────────────────────────────────────────────
  const fetchDetail = useCallback(
    async (id: string) => {
      setDetailLoading(prev => ({ ...prev, [id]: true }))
      setDetailError(prev => ({ ...prev, [id]: undefined }))
      try {
        const detail = await client.getBackup(id)
        setDetailCache(prev => ({ ...prev, [id]: detail }))
      } catch (err) {
        setDetailError(prev => ({
          ...prev,
          [id]: err instanceof Error ? err.message : 'Failed to load tables',
        }))
      } finally {
        setDetailLoading(prev => ({ ...prev, [id]: false }))
      }
    },
    [client],
  )

  const toggleExpand = useCallback(
    (id: string) => {
      setExpandedId(prev => {
        if (prev === id) return null
        if (!detailCache[id] && !detailLoading[id]) {
          void fetchDetail(id)
        }
        return id
      })
    },
    [detailCache, detailLoading, fetchDetail],
  )

  // ── Download ────────────────────────────────────────────────────────────────
  const downloadMut = useMutation({
    mutationFn: (id: string) => client.downloadBackup(id),
    onSuccess: (blob, id) => {
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `backup-${id}.json`
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    },
    onError: (err) => {
      showFlash({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Failed to download backup.',
      })
    },
    onSettled: (_data, _err, id) => setDownloadingId(curr => (curr === id ? null : curr)),
  })

  const handleDownload = (id: string) => {
    setDownloadingId(id)
    downloadMut.mutate(id)
  }

  // ── Restore flow ──────────────────────────────────────────────────────────
  const openRestore = (b: Backup) => {
    if (b.status === 'pending' || b.status === 'running') return
    setRestoreTarget(b)
  }

  const handleRestore = () => {
    if (!restoreTarget) return
    restoreMut.mutate(restoreTarget.id, {
      onSuccess: () => setRestoreTarget(null),
    })
  }

  return (
    <div className="p-6 max-w-6xl mx-auto space-y-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">
            Backups
          </h1>
          <p className="mt-1 text-[13px] text-text-secondary max-w-xl">
            Manage Postgres database backups. Restoring from a backup{' '}
            <span className="text-status-error font-semibold">REPLACES</span> the current database.
          </p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            onClick={() => createMut.mutate()}
            disabled={createMut.isPending}
            className={`flex items-center gap-1.5 rounded-full bg-accent-blue px-4 py-1.5 text-[13px] font-semibold text-white hover:bg-accent-blue-hover transition-colors disabled:opacity-40 ${FOCUS}`}
          >
            {createMut.isPending ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Plus className="w-3.5 h-3.5" />
            )}
            {createMut.isPending ? 'Creating…' : 'Create backup'}
          </button>
        </div>
      </div>

      {flash && (
        <div
          role="status"
          aria-live="polite"
          className={`flex items-start gap-3 rounded-[11px] border px-4 py-3 ${
            flash.kind === 'success'
              ? 'border-status-success/30 bg-status-success/5 text-status-success'
              : 'border-status-error/30 bg-status-error/5 text-status-error'
          }`}
        >
          {flash.kind === 'success' ? (
            <CheckCircle2 className="w-4 h-4 mt-0.5 shrink-0" />
          ) : (
            <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
          )}
          <p className="flex-1 text-[13px] leading-relaxed">{flash.message}</p>
          <button
            onClick={() => showFlash(null)}
            aria-label="Dismiss"
            className={`rounded-full p-1 hover:bg-white/[0.06] transition-colors ${FOCUS}`}
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {listErrorMessage && (
        <div className="rounded-[11px] border border-status-warning/30 bg-status-warning/5 px-4 py-3 text-[13px] text-status-warning flex items-start gap-2">
          <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
          <div className="flex-1">
            <p className="font-semibold">Backup API not available.</p>
            <p className="text-[12px] mt-0.5 text-text-secondary">{listErrorMessage}</p>
          </div>
          <button
            onClick={() => refetch()}
            className={`text-[12px] text-text-secondary border border-border-primary rounded-full px-2.5 py-1 hover:text-text-primary hover:bg-white/[0.04] transition-colors ${FOCUS}`}
          >
            Retry
          </button>
        </div>
      )}

      <BackupsTable
        backups={backups}
        loading={isLoading}
        downloadingId={downloadingId}
        expandedId={expandedId}
        detailCache={detailCache}
        detailLoading={detailLoading}
        detailError={detailError}
        onToggleExpand={toggleExpand}
        onDownload={handleDownload}
        onRestore={openRestore}
      />

      {!isLoading && backups.length > 0 && (
        <p className="text-[11px] text-text-quaternary text-right">
          Auto-refresh every 30s.
        </p>
      )}

      <RestoreConfirmDialog
        open={!!restoreTarget}
        backup={restoreTarget}
        orgSlug={orgSlug}
        loading={restoreMut.isPending}
        onConfirm={handleRestore}
        onClose={() => !restoreMut.isPending && setRestoreTarget(null)}
      />
    </div>
  )
}
