import { useState } from 'react'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { Modal, ModalTitle } from './Modal'

function Example({ initiallyOpen = false }) {
  const [open, setOpen] = useState(initiallyOpen)
  return <><button onClick={() => setOpen(true)}>Open editor</button><Modal open={open} onOpenChange={setOpen}>
    <ModalTitle>Edit item</ModalTitle><input aria-label="Name" /><button onClick={() => setOpen(false)}>Cancel</button>
  </Modal></>
}

describe('Modal keyboard interaction', () => {
  it('labels and focuses a dialog that is already open when its portal mounts', async () => {
    render(<Example initiallyOpen />)
    await waitFor(() => expect(screen.getByRole('dialog', { name: 'Edit item' })).toBeInTheDocument())
    expect(screen.getByRole('dialog', { name: 'Edit item' })).toHaveAttribute('aria-modal', 'true')
    expect(document.querySelector('[data-slot="dialog-overlay"]')).toHaveClass('backdrop-blur-[2px]')
    await waitFor(() => expect(screen.getByLabelText('Name')).toHaveFocus())
  })

  it('traps keyboard focus and restores the opener after Escape', async () => {
    render(<Example />)
    const opener = screen.getByText('Open editor')
    opener.focus()
    fireEvent.click(opener)
    const field = await screen.findByLabelText('Name')
    await waitFor(() => expect(field).toHaveFocus())
    fireEvent.keyDown(field, { key: 'Tab', shiftKey: true })
    expect(screen.getByText('Cancel')).toHaveFocus()
    fireEvent.keyDown(screen.getByText('Cancel'), { key: 'Tab' })
    expect(field).toHaveFocus()
    fireEvent.keyDown(field, { key: 'Escape' })
    await waitFor(() => expect(opener).toHaveFocus())
    expect(document.body.style.overflow).not.toBe('hidden')
  })
})
