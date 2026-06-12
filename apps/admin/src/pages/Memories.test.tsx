import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Memories from './Memories'

// Mock download module
vi.mock('../lib/download', () => ({
  downloadExport: vi.fn().mockResolvedValue(undefined),
  todayStamp: () => '2026-06-11',
}))

// Mock api client
vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listMemories: vi.fn().mockResolvedValue([]),
    searchMemories: vi.fn().mockResolvedValue([]),
    listUsers: vi.fn().mockResolvedValue([]),
    deleteMemory: vi.fn().mockResolvedValue(undefined),
  })),
}))

import * as downloadModule from '../lib/download'

describe('Memories', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('JSON export — downloadExport called with URL ending /v1/memory/export?format=json and matching filename', async () => {
    renderWithProviders(<Memories />)

    // Wait for the page to render
    await waitFor(() => {
      expect(screen.getByPlaceholderText('Search memories…')).toBeInTheDocument()
    })

    const exportJsonButton = screen.getByRole('button', { name: /export memories as json/i })
    fireEvent.click(exportJsonButton)

    await waitFor(() => {
      expect(downloadModule.downloadExport).toHaveBeenCalledTimes(1)
      const [url, filename] = (downloadModule.downloadExport as ReturnType<typeof vi.fn>).mock.calls[0]
      expect(url).toMatch(/\/v1\/memory\/export\?format=json/)
      expect(filename).toMatch(/^memories-\d{4}-\d{2}-\d{2}\.json$/)
    })
  })
})
