import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'
import { RunScreenshots } from './RunScreenshots'

afterEach(cleanup)

const screenshots = Object.fromEntries(
  Array.from({ length: 8 }, (_, i) => [`shot-${i + 1}.png`, `https://signed.example/${i + 1}`]),
)
const findings = [
  { title: 'Checkout total is wrong', summary: 'Shows $0 after coupon.', severity: 'high', screenshot: 'shot-2.png' },
]

describe('RunScreenshots', () => {
  it('shows every screenshot and browses them in a modal without leaving the run', async () => {
    render(<RunScreenshots runId="run-1" screenshots={screenshots} findings={findings} />)
    const thumbs = screen.getAllByRole('button', { name: /^Open screenshot/ })
    expect(thumbs).toHaveLength(8)

    await userEvent.click(thumbs[1])
    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByRole('heading', { name: 'Checkout total is wrong' })).toBeInTheDocument()
    expect(within(dialog).getByText('Shows $0 after coupon.')).toBeInTheDocument()
    expect(within(dialog).getByRole('img', { name: 'Screenshot shot-2.png' })).toHaveAttribute(
      'src',
      expect.stringContaining('/evidence/run-1/shot-2.png'),
    )
    expect(within(dialog).getByText('2 / 8', { exact: false })).toBeInTheDocument()

    await userEvent.click(within(dialog).getByRole('button', { name: 'Next screenshot' }))
    expect(within(dialog).getByRole('heading', { name: 'shot-3.png' })).toBeInTheDocument()

    // Keys reach the gallery from whatever is focused inside the modal.
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowLeft' })
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowLeft' })
    expect(within(dialog).getByRole('heading', { name: 'shot-1.png' })).toBeInTheDocument()
    await userEvent.click(within(dialog).getByRole('button', { name: 'Previous screenshot' }))
    expect(within(dialog).getByRole('heading', { name: 'shot-8.png' })).toBeInTheDocument()
  })
})
