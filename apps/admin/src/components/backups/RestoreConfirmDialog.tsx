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
        className="relative border border-status-error/30 bg-[#0f1117]/[0.94] backdrop-blur-[22px] rounded-[18px] p-6 w-full max-w-md"
        onClick={e => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          disabled={loading}
          aria-label="Close dialog"
          className={`absolute top-4 right-4 rounded-full p-1 text-text-tertiary hover:text-text-primary transition-colors disabled:opacity-40 ${FOCUS}`}
        >
          <X className="w-4 h-4" />
        </button>

        {/* Icon + title — NexusMind UI Kit "Confirmación destructiva": 36px
            icon tile next to an 800-weight title, matching ConfirmModal. */}
        <div className="space-y-4 pr-6">
          <div className="flex items-start gap-3">
            <div className="w-9 h-9 rounded-[11px] bg-status-error/10 flex items-center justify-center shrink-0">
              <AlertTriangle className="w-[17px] h-[17px] text-status-error" />
            </div>
            <h2 id="restore-dialog-title" className="text-[14px] font-extrabold text-text-primary pt-1.5">
              Restore database
            </h2>
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

          {/* Button row — ghost cancel + solid danger confirm, right-aligned */}
          <div className="flex justify-end gap-2">
            <button
              onClick={onClose}
              disabled={loading}
              className={`flex items-center h-[34px] px-[14px] rounded-[9px] text-[12.5px] font-semibold text-text-tertiary hover:text-text-primary transition-colors disabled:opacity-40 ${FOCUS}`}
            >
              Cancel
            </button>
            <button
              onClick={onConfirm}
              disabled={loading || !matches}
              aria-disabled={loading || !matches}
              className={`flex items-center h-[34px] px-4 rounded-[9px] text-[12.5px] font-bold transition-colors bg-status-error text-white hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed ${FOCUS}`}
            >
              {loading ? 'Restoring…' : 'Restore database'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
