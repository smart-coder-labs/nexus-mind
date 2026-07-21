import { useEffect, useRef } from 'react'
import { AlertTriangle, HelpCircle, X } from 'lucide-react'

interface Props {
  open: boolean
  title: string
  description: string
  confirmLabel: string
  danger?: boolean
  loading?: boolean
  onConfirm: () => void
  onClose: () => void
}

export function ConfirmModal({ open, title, description, confirmLabel, danger, loading, onConfirm, onClose }: Props) {
  const modalRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    document.body.style.overflow = 'hidden'
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handleEscape)
    return () => {
      document.body.style.overflow = ''
      document.removeEventListener('keydown', handleEscape)
    }
  }, [open, onClose])

  // Focus trap
  useEffect(() => {
    if (!open) return
    const modal = modalRef.current
    if (!modal) return
    const focusable = modal.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    )
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    first?.focus()
    const trap = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return
      if (e.shiftKey) {
        if (document.activeElement === first) { e.preventDefault(); last?.focus() }
      } else {
        if (document.activeElement === last) { e.preventDefault(); first?.focus() }
      }
    }
    document.addEventListener('keydown', trap)
    return () => document.removeEventListener('keydown', trap)
  }, [open])

  if (!open) return null

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <div
        ref={modalRef}
        className="relative border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] rounded-[18px] p-6 w-full max-w-sm"
        onClick={e => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          aria-label={`Close ${title} dialog`}
          className="absolute top-4 right-4 text-text-tertiary hover:text-text-primary transition-colors"
        >
          <X className="w-4 h-4" />
        </button>

        {/* Icon + title/message — NexusMind UI Kit "Confirmación destructiva":
            36px icon tile, 800-weight title, 12.5px message below it. */}
        <div className="space-y-4 pr-6">
          <div className="flex items-start gap-3">
            <div className={`w-9 h-9 rounded-[11px] flex items-center justify-center shrink-0 ${
              danger ? 'bg-status-error/10' : 'bg-accent-blue-tint'
            }`}>
              {danger
                ? <AlertTriangle className="w-[17px] h-[17px] text-status-error" />
                : <HelpCircle className="w-[17px] h-[17px] text-accent-blue" />
              }
            </div>
            <div className="flex flex-col gap-0.5 min-w-0 pt-0.5">
              <p className="text-[14px] font-extrabold text-text-primary">{title}</p>
              <p className="text-[12.5px] text-text-secondary leading-relaxed">{description}</p>
            </div>
          </div>

          {/* Button row — ghost cancel + solid danger/primary confirm, right-aligned */}
          <div className="flex justify-end gap-2">
            <button
              onClick={onClose}
              disabled={loading}
              className="flex items-center h-[34px] px-[14px] rounded-[9px] text-[12.5px] font-semibold text-text-tertiary hover:text-text-primary transition-colors disabled:opacity-40"
            >
              Cancel
            </button>
            <button
              onClick={onConfirm}
              disabled={loading}
              className={`flex items-center h-[34px] px-4 rounded-[9px] text-[12.5px] font-bold text-white transition-colors disabled:opacity-40 ${
                danger
                  ? 'bg-status-error hover:opacity-90'
                  : 'bg-accent-blue hover:bg-accent-blue-hover'
              }`}
            >
              {loading ? '…' : confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
