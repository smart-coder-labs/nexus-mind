import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { RestoreConfirmDialog } from './RestoreConfirmDialog'
import type { Backup } from '../../types'

const sampleBackup: Backup = {
  id: 'bk-abc-123',
  org_id: 'org-1',
  created_at: new Date('2026-06-01T12:00:00Z').toISOString(),
  kind: 'manual',
  status: 'completed',
  size_bytes: 5_242_880,
  metadata: null,
}

function renderDialog(props: Partial<React.ComponentProps<typeof RestoreConfirmDialog>> = {}) {
  const onConfirm = vi.fn()
  const onClose = vi.fn()
  const utils = render(
    <RestoreConfirmDialog
      open
      backup={sampleBackup}
      orgSlug="acme-corp"
      loading={false}
      onConfirm={onConfirm}
      onClose={onClose}
      {...props}
    />,
  )
  return { ...utils, onConfirm, onClose }
}

describe('RestoreConfirmDialog', () => {
  it('does not render when closed', () => {
    renderDialog({ open: false })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('does not render when no backup is provided', () => {
    renderDialog({ backup: null })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('shows the backup id and created date in the warning', () => {
    renderDialog()
    const dialog = screen.getByRole('dialog', { name: /restore database/i })
    expect(dialog).toBeInTheDocument()
    expect(dialog.textContent).toContain('bk-abc-123')
    expect(dialog.textContent).toContain('REPLACE')
  })

  it('confirm button is disabled until the user types the exact org slug', () => {
    const { onConfirm } = renderDialog()
    const confirmBtn = screen.getByRole('button', { name: /restore database/i })
    const input = screen.getByLabelText(/type/i) as HTMLInputElement

    expect(confirmBtn).toBeDisabled()

    fireEvent.change(input, { target: { value: 'acme' } })
    expect(confirmBtn).toBeDisabled()

    fireEvent.change(input, { target: { value: 'ACME-CORP' } }) // case-sensitive
    expect(confirmBtn).toBeDisabled()

    fireEvent.change(input, { target: { value: '  acme-corp  ' } }) // trims whitespace
    expect(confirmBtn).toBeEnabled()

    fireEvent.click(confirmBtn)
    expect(onConfirm).toHaveBeenCalledTimes(1)
  })

  it('cancel button calls onClose and does not call onConfirm', () => {
    const { onConfirm, onClose } = renderDialog()
    const cancelBtn = screen.getByRole('button', { name: /cancel/i })
    fireEvent.click(cancelBtn)
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(onConfirm).not.toHaveBeenCalled()
  })

  it('Escape key closes the dialog (when not loading)', () => {
    const { onClose } = renderDialog()
    fireEvent.keyDown(document, { key: 'Escape' })
    expect(onClose).toHaveBeenCalled()
  })

  it('disables both buttons while loading and ignores Escape', () => {
    const { onClose } = renderDialog({ loading: true })
    const cancelBtn = screen.getByRole('button', { name: /cancel/i }) as HTMLButtonElement
    const confirmBtn = screen.getByRole('button', { name: /restoring…/i })
    expect(cancelBtn).toBeDisabled()
    expect(confirmBtn).toBeDisabled()

    fireEvent.keyDown(document, { key: 'Escape' })
    expect(onClose).not.toHaveBeenCalled()
  })

  it('clicking the backdrop calls onClose (when not loading)', () => {
    const { onClose, container } = renderDialog()
    // The backdrop is the first fixed-positioned div rendered before the dialog panel.
    const backdrop = container.querySelector('div.fixed.inset-0') as HTMLElement
    expect(backdrop).toBeTruthy()
    fireEvent.click(backdrop)
    expect(onClose).toHaveBeenCalled()
  })
})
