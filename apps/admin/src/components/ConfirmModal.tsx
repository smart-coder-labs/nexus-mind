import { X } from 'lucide-react'

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
  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-surface-primary border border-border-primary rounded-[18px] p-6 w-full max-w-sm space-y-4">
        <div className="flex items-center justify-between">
          <p className="text-text-primary font-medium">{title}</p>
          <button onClick={onClose} className="text-text-tertiary hover:text-text-primary transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>
        <p className="text-sm text-text-secondary">{description}</p>
        <div className="flex gap-2">
          <button
            onClick={onClose}
            disabled={loading}
            className="flex-1 py-2 rounded-lg border border-border-primary text-sm text-text-secondary hover:text-text-primary hover:bg-surface-secondary transition-colors disabled:opacity-40"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={loading}
            className={`flex-1 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-40 ${
              danger
                ? 'bg-status-error text-white hover:opacity-90'
                : 'bg-accent-blue hover:bg-accent-blue-hover text-white'
            }`}
          >
            {loading ? '…' : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}
