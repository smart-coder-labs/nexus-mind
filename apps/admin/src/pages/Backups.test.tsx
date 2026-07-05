import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor, within } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Backups from './Backups'

// DOM stubs for client-side download (anchor click + object URLs)
const createObjectURLSpy = vi.fn(() => 'blob:mock-url')
const revokeObjectURLSpy = vi.fn()
const anchorClickSpy = vi.fn()
const origCreateElement = document.createElement.bind(document)
let lastAnchor: HTMLAnchorElement | null = null

beforeEach(() => {
  vi.clearAllMocks()
  lastAnchor = null

  Object.defineProperty(URL, 'createObjectURL', { value: createObjectURLSpy, configurable: true, writable: true })
  Object.defineProperty(URL, 'revokeObjectURL', { value: revokeObjectURLSpy, configurable: true, writable: true })

  vi.spyOn(document, 'createElement').mockImplementation((tag: string, ...rest) => {
    const el = origCreateElement(tag, ...rest)
    if (tag === 'a') {
      lastAnchor = el as HTMLAnchorElement
      vi.spyOn(el as HTMLAnchorElement, 'click').mockImplementation(anchorClickSpy)
    }
    return el
  })
})

const mockList = vi.fn()
const mockGet = vi.fn()
const mockCreate = vi.fn()
const mockRestore = vi.fn()
const mockDownload = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listBackups: mockList,
    getBackup: mockGet,
    createBackup: mockCreate,
    restoreBackup: mockRestore,
    downloadBackup: mockDownload,
  })),
}))

const baseBackups = [
  {
    id: 'bk-1',
    org_id: 'org-test-1',
    created_at: new Date(Date.now() - 60 * 60_000).toISOString(),
    kind: 'manual',
    status: 'completed',
    size_bytes: 12_582_912,
    metadata: null,
  },
  {
    id: 'bk-2',
    org_id: 'org-test-1',
    created_at: new Date(Date.now() - 24 * 60 * 60_000).toISOString(),
    kind: 'scheduled',
    status: 'failed',
    size_bytes: 0,
    metadata: null,
  },
]

describe('Backups page', () => {
  beforeEach(() => {
    mockList.mockResolvedValue(baseBackups)
    mockGet.mockResolvedValue({
      ...baseBackups[0],
      table_list: [
        { table_name: 'memories', row_count: 100 },
        { table_name: 'users',    row_count: 5 },
      ],
    })
    mockCreate.mockResolvedValue({ ...baseBackups[0], id: 'bk-new' })
    mockRestore.mockResolvedValue({
      backup_id: 'bk-1',
      restored_at: new Date().toISOString(),
      tables_restored: 2,
      rows_restored: 105,
    })
    mockDownload.mockResolvedValue(new Blob(['{}'], { type: 'application/json' }))
  })

  it('renders a row per backup with formatted size and relative time', async () => {
    renderWithProviders(<Backups />)

    // Wait for the actual data, not the skeleton.
    await waitFor(() => expect(screen.getByText('12 MB')).toBeInTheDocument())

    // bk-2 has size 0
    expect(screen.getByText('0 B')).toBeInTheDocument()
    // bk-1 is ~1h old → "about 1 hour ago"
    expect(screen.getByText(/about 1 hour ago/i)).toBeInTheDocument()
    // status badges render
    expect(screen.getAllByText('completed').length).toBeGreaterThan(0)
    expect(screen.getAllByText('failed').length).toBeGreaterThan(0)
  })

  it('triggers a manual backup on button click and shows a success flash', async () => {
    renderWithProviders(<Backups />)

    await waitFor(() => expect(screen.getByText('12 MB')).toBeInTheDocument())

    const createBtn = screen.getByRole('button', { name: /create backup/i })
    fireEvent.click(createBtn)

    await waitFor(() => expect(mockCreate).toHaveBeenCalled())
    await waitFor(() =>
      expect(screen.getByText(/backup bk-new… created/i)).toBeInTheDocument(),
    )
  })

  it('expands a row to show tables when clicking the toggle', async () => {
    renderWithProviders(<Backups />)

    await waitFor(() => expect(screen.getByText('12 MB')).toBeInTheDocument())

    // There are at least two "Tables" buttons (one per row). Use the first row.
    const tablesButtons = screen.getAllByRole('button', { name: /view tables/i })
    fireEvent.click(tablesButtons[0])

    await waitFor(() => expect(mockGet).toHaveBeenCalledWith('bk-1'))
    await waitFor(() => expect(screen.getByText('memories')).toBeInTheDocument())
    expect(screen.getByText('users')).toBeInTheDocument()
  })

  it('downloads a backup as JSON when clicking Download', async () => {
    renderWithProviders(<Backups />)

    await waitFor(() => expect(screen.getByText('12 MB')).toBeInTheDocument())

    const downloadBtn = screen.getAllByRole('button', { name: /download backup/i })[0]
    fireEvent.click(downloadBtn)

    await waitFor(() => expect(mockDownload).toHaveBeenCalledWith('bk-1'))
    await waitFor(() => expect(anchorClickSpy).toHaveBeenCalled())
    expect(lastAnchor?.download).toBe('backup-bk-1.json')
    expect(createObjectURLSpy).toHaveBeenCalled()
  })

  it('opens the destructive restore dialog and requires typing the org slug', async () => {
    renderWithProviders(<Backups />)

    await waitFor(() => expect(screen.getByText('12 MB')).toBeInTheDocument())

    const restoreBtn = screen.getAllByRole('button', { name: /restore from backup/i })[0]
    fireEvent.click(restoreBtn)

    const dialog = await screen.findByRole('dialog', { name: /restore database/i })
    expect(within(dialog).getByText('test-org')).toBeInTheDocument()

    // Confirm button starts disabled
    const confirmBtn = within(dialog).getByRole('button', { name: /restore database/i })
    expect(confirmBtn).toBeDisabled()

    // Wrong value
    const input = within(dialog).getByLabelText(/type/i) as HTMLInputElement
    fireEvent.change(input, { target: { value: 'wrong' } })
    expect(confirmBtn).toBeDisabled()

    // Correct value
    fireEvent.change(input, { target: { value: 'test-org' } })
    expect(confirmBtn).toBeEnabled()

    fireEvent.click(confirmBtn)
    await waitFor(() => expect(mockRestore).toHaveBeenCalledWith('bk-1'))
  })

  it('shows an inline warning when the backup API is unavailable', async () => {
    // Use mockRejectedValue (not Once) so the query's retry also fails.
    mockList.mockRejectedValue(new Error('404 Not Found'))

    renderWithProviders(<Backups />)

    await waitFor(() =>
      expect(screen.getByText(/backup api not available/i)).toBeInTheDocument(),
    )
    expect(screen.getByText('404 Not Found')).toBeInTheDocument()

    // Restore the default for subsequent tests in the suite.
    mockList.mockResolvedValue(baseBackups)
  })
})
