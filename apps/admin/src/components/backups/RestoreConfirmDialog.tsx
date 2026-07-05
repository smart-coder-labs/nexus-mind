import { useEffect, useRef, useState } from 'react'
import { AlertTriangle, X } from 'lucide-react'
import type { Backup } from '../../types'

interface RestoreConfirmDialogProps {
  open: boolean
  backup: Backup | null
  orgSlug: string
  loading: boolean
  onConfirm: () => void
  onClose: () => void
}

const FOCUS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

export function RestoreConfirmDialog({
  open,
  backup,
  orgSlug,
  loading,
  onConfirm,
  onClose,
}: RestoreConfirmDialogProps) {
  const [typed, setTyped] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  const dialogRef = useRef<HTMLDivElement>(null)

  // Reset on open/close and lock body scroll.
  useEffect(() => {
    if (!open) return
    setTyped('')
    document.body.style.overflow = 'hidden'
    return () => {
      document.body.style.overflow = ''
    }
  }, [open])

  // Autofocus the input on open.
  useEffect(() => {
    if (!open) return
    const t = window.setTimeout(() => inputRef.current?.focus(), 0)
    return () => window.clearTimeout(t)
  }, [open])

  // Close on Escape (but only when not loading).
  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !loading) onClose()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [open, loading, onClose])

  if (!open || !backup) return null

  const matches = typed.trim() === orgSlug

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={() => !loading && onClose()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="restore-dialog-title"
    >
      <div
        ref={dialogRef}
        className="bg-background-secondary border border-status-error/30 rounded-[18px] p-6 w-full max-w-md space-y-4"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <AlertTriangle className="w-4 h-4 text-status-error" />
            <h2 id="restore-dialog-title" className="text-text-primary font-semibold text-[15px]">
              Restore database
            </h2>
          </div>
          <button
            onClick={onClose}
            disabled={loading}
            aria-label="Close dialog"
            className={`rounded-full p-1 text-text-tertiary hover:text-text-primary transition-colors disabled:opacity-40 ${FOCUS}`}
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="rounded-[11px] border border-status-error/20 bg-status-error/5 px-3 py-2.5 text-[12px] text-status-error leading-relaxed">
          This will <strong>REPLACE</strong> the current database with the contents of backup{' '}
          <code className="font-mono text-[11px]">{backup.id}</code> from{' '}
          {new Date(backup.created_at).toLocaleString()}. All current data will be lost.
        </div>

        <div className="space-y-1.5">
          <label htmlFor="restore-confirm-slug" className="block text-[12px] text-text-tertiary">
            Type <code className="font-mono text-text-secondary">{orgSlug}</code> to confirm
          </label>
          <input
            id="restore-confirm-slug"
            ref={inputRef}
            type="text"
            value={typed}
            onChange={e => setTyped(e.target.value)}
            disabled={loading}
            autoComplete="off"
            spellCheck={false}
            className={`w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-[13px] text-text-primary px-3 py-2 placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 disabled:opacity-50 ${FOCUS}`}
            placeholder={orgSlug}
          />
        </div>

        <div className="flex gap-2 pt-1">
          <button
            onClick={onClose}
            disabled={loading}
            className={`flex-1 py-2 rounded-full border border-border-primary text-xs text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors disabled:opacity-40 ${FOCUS}`}
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={loading || !matches}
            aria-disabled={loading || !matches}
            className={`flex-1 py-2 rounded-full text-xs font-semibold transition-colors bg-status-error text-white hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed ${FOCUS}`}
          >
            {loading ? 'Restoring…' : 'Restore database'}
          </button>
        </div>
      </div>
    </div>
  )
}
