import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import AuditLog from './AuditLog'

// Mock download module (needed for export tests)
vi.mock('../lib/download', () => ({
  downloadExport: vi.fn().mockResolvedValue(undefined),
  todayStamp: () => '2026-06-11',
}))

// Create a stable mock client that tests can configure
const mockGetAuditLog = vi.fn()
const mockListUsers = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    getAuditLog: mockGetAuditLog,
    listUsers: mockListUsers,
  })),
}))

import * as downloadModule from '../lib/download'

const emptyEntries: never[] = []

describe('AuditLog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockGetAuditLog.mockResolvedValue(emptyEntries)
    mockListUsers.mockResolvedValue([])
  })

  it('filter apply — getAuditLog called with selected action and page reset to 0', async () => {
    renderWithProviders(<AuditLog />)

    // Wait for initial query to settle
    await waitFor(() => {
      expect(mockGetAuditLog).toHaveBeenCalled()
    })

    // Select the "store" action in the action filter dropdown
    const actionSelect = screen.getAllByRole('combobox')[1] // second select is action
    fireEvent.change(actionSelect, { target: { value: 'store' } })

    // Click Apply
    const applyButton = screen.getByRole('button', { name: /apply/i })
    fireEvent.click(applyButton)

    await waitFor(() => {
      const calls = mockGetAuditLog.mock.calls
      const lastCall = calls[calls.length - 1][0]
      expect(lastCall).toMatchObject({ action: 'store' })
      expect(lastCall.offset).toBe(0)
    })
  })

  it('filter clear — resets draft state and next getAuditLog call has no filter fields', async () => {
    renderWithProviders(<AuditLog />)

    await waitFor(() => expect(mockGetAuditLog).toHaveBeenCalled())

    // Apply an action filter first
    const actionSelect = screen.getAllByRole('combobox')[1]
    fireEvent.change(actionSelect, { target: { value: 'delete' } })
    const applyButton = screen.getByRole('button', { name: /apply/i })
    fireEvent.click(applyButton)

    await waitFor(() => {
      const calls = mockGetAuditLog.mock.calls
      const lastCall = calls[calls.length - 1][0]
      expect(lastCall).toMatchObject({ action: 'delete' })
    })

    // Click Clear
    const clearButton = screen.getByRole('button', { name: /clear/i })
    fireEvent.click(clearButton)

    await waitFor(() => {
      const calls = mockGetAuditLog.mock.calls
      const lastCall = calls[calls.length - 1][0]
      expect(lastCall.action).toBeUndefined()
      expect(lastCall.user_id).toBeUndefined()
      expect(lastCall.resource_type).toBeUndefined()
    })
  })

  it('CSV export — downloadExport called with URL ending /v1/audit/export?format=csv and matching filename', async () => {
    renderWithProviders(<AuditLog />)

    await waitFor(() => expect(mockGetAuditLog).toHaveBeenCalled())

    const exportCsvButton = screen.getByRole('button', { name: /export audit log as csv/i })
    fireEvent.click(exportCsvButton)

    await waitFor(() => {
      expect(downloadModule.downloadExport).toHaveBeenCalledTimes(1)
      const [url, filename] = (downloadModule.downloadExport as ReturnType<typeof vi.fn>).mock.calls[0]
      expect(url).toMatch(/\/v1\/audit\/export\?format=csv/)
      expect(filename).toMatch(/^audit-\d{4}-\d{2}-\d{2}\.csv$/)
    })
  })
})
