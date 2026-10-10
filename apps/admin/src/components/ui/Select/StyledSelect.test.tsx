import { useState } from 'react'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { StyledSelect } from './StyledSelect'

function Example() {
  const [value, setValue] = useState('')
  return <>
    <StyledSelect aria-label="Memory type" value={value} onChange={event => setValue(event.target.value)}>
      <option value="" data-description="Include every type">All types</option>
      <option value="decision" data-description="Important choices" data-color="#b9d82b">Decision</option>
    </StyledSelect>
    <output aria-label="Selected type">{value || 'all'}</output>
  </>
}

describe('StyledSelect', () => {
  it('keeps native select call sites controlled while rendering descriptive options', async () => {
    HTMLElement.prototype.scrollIntoView = () => {}
    const user = userEvent.setup()
    render(<Example />)

    await user.click(screen.getByRole('combobox', { name: 'Memory type' }))
    expect(await screen.findByText('Important choices')).toBeInTheDocument()
    await user.click(screen.getByRole('option', { name: /Decision/ }))

    await waitFor(() => expect(screen.getByLabelText('Selected type')).toHaveTextContent('decision'))
  })
})
