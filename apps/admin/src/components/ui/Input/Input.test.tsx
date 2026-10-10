import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { Input, Textarea } from './Input'

describe('Input accessibility', () => {
  it('connects the label, error and caller description to the field', () => {
    render(<><p id="policy">Use a work address</p><Input label="Email" error="Email is required" aria-describedby="policy" /></>)
    const field = screen.getByLabelText('Email')
    expect(field).toHaveAttribute('aria-invalid', 'true')
    expect(field).toHaveAccessibleDescription('Use a work address Email is required')
    expect(screen.getByRole('alert')).toHaveTextContent('Email is required')
  })

  it('keeps explicit IDs and generates distinct IDs for repeated labels', () => {
    render(<><Input id="first-name" label="Name" /><Input label="Name" /></>)
    const fields = screen.getAllByLabelText('Name')
    expect(fields[0]).toHaveAttribute('id', 'first-name')
    expect(fields[1].id).not.toBe(fields[0].id)
  })

  it('uses generated Tailwind resize classes and connects textarea help', () => {
    render(<Textarea label="Description" resize="horizontal" helperText="Explain the change" />)
    const field = screen.getByLabelText('Description')
    expect(field).toHaveClass('resize-x')
    expect(field).toHaveAccessibleDescription('Explain the change')
  })
})
