import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { Table } from './Table'

describe('Table controls', () => {
  it('keeps an empty table on page one and disables unavailable actions', () => {
    render(<Table columns={[{ key: 'name', header: 'Name', width: '220px' }]} data={[] as {name:string}[]} selectable />)
    expect(screen.getByText('Page 1 of 1')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Next page' })).toBeDisabled()
    expect(screen.getByRole('checkbox', { name: 'Select all rows' })).toBeDisabled()
    expect(screen.getByRole('columnheader', { name: 'Name' })).toHaveStyle({ width: '220px' })
  })

  it('selects a row without activating its detail action', () => {
    const onRowClick = vi.fn()
    render(<Table columns={[{key:'name',header:'Name'}]} data={[{name:'Example'}]} selectable onRowClick={onRowClick} />)
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select row 1' }))
    expect(screen.getByRole('checkbox', { name: 'Select row 1' })).toBeChecked()
    expect(onRowClick).not.toHaveBeenCalled()
  })
})
