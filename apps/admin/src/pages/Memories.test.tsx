import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Memories from './Memories'

// ── DOM stubs for client-side export ──────────────────────────────────────────

const mockObjectUrl = 'blob:mock-url'
const createObjectURLSpy = vi.fn(() => mockObjectUrl)
const revokeObjectURLSpy = vi.fn()
const anchorClickSpy = vi.fn()

// Keep a reference to the last <a> created so we can inspect its attributes
let lastAnchor: HTMLAnchorElement | null = null
const origCreateElement = document.createElement.bind(document)

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

// Mock download utility (todayStamp only — downloadExport is no longer used)
vi.mock('../lib/download', () => ({
  downloadExport: vi.fn(),
  todayStamp: () => '2026-06-20',
}))

// Mock API client
const listMemoriesMock = vi.fn().mockResolvedValue([])
const searchMemoriesMock = vi.fn().mockResolvedValue([])

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listMemories: listMemoriesMock,
    searchMemories: searchMemoriesMock,
    listUsers: vi.fn().mockResolvedValue([]),
    deleteMemory: vi.fn().mockResolvedValue(undefined),
  })),
}))

describe('Memories — client-side export', () => {
  it('renders Export button', async () => {
    renderWithProviders(<Memories />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /export memories/i })).toBeInTheDocument()
    })
  })

  it('opens dropdown with JSON and CSV options on click', async () => {
    renderWithProviders(<Memories />)

    const exportBtn = await screen.findByRole('button', { name: /export memories/i })
    fireEvent.click(exportBtn)

    expect(screen.getByRole('menuitem', { name: /export json/i })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: /export csv/i })).toBeInTheDocument()
  })

  it('JSON export — calls listMemories with limit 5000 and triggers download', async () => {
    renderWithProviders(<Memories />)

    const exportBtn = await screen.findByRole('button', { name: /export memories/i })
    fireEvent.click(exportBtn)

    const jsonOption = screen.getByRole('menuitem', { name: /export json/i })
    fireEvent.click(jsonOption)

    await waitFor(() => {
      expect(listMemoriesMock).toHaveBeenCalledWith(
        expect.objectContaining({ limit: 5000, offset: 0 }),
      )
    })

    await waitFor(() => {
      expect(createObjectURLSpy).toHaveBeenCalledTimes(1)
      expect(anchorClickSpy).toHaveBeenCalledTimes(1)
      expect(revokeObjectURLSpy).toHaveBeenCalledWith(mockObjectUrl)
    })

    expect(lastAnchor?.download).toMatch(/^memories-2026-06-20\.json$/)
  })

  it('CSV export — calls listMemories with limit 5000 and triggers download with .csv filename', async () => {
    renderWithProviders(<Memories />)

    const exportBtn = await screen.findByRole('button', { name: /export memories/i })
    fireEvent.click(exportBtn)

    const csvOption = screen.getByRole('menuitem', { name: /export csv/i })
    fireEvent.click(csvOption)

    await waitFor(() => {
      expect(listMemoriesMock).toHaveBeenCalledWith(
        expect.objectContaining({ limit: 5000, offset: 0 }),
      )
    })

    await waitFor(() => {
      expect(anchorClickSpy).toHaveBeenCalledTimes(1)
    })

    expect(lastAnchor?.download).toMatch(/^memories-2026-06-20\.csv$/)
  })

  it('CSV export — memory with commas and quotes in content is escaped', async () => {
    const fixture = [
      {
        id: 'abc',
        org_id: 'org1',
        user_id: 'u1',
        project: 'proj',
        tool: 'claude-code',
        content: 'He said "hello, world"',
        tags: ['tag1', 'tag2'],
        created_at: '2026-06-20T10:00:00Z',
        type: 'decision',
        scope: 'project',
      },
    ]

    // First call (initial render, limit 50) returns [], second call (export, limit 5000) returns fixture
    listMemoriesMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce(fixture)

    // Capture the Blob passed to createObjectURL
    let capturedBlob: Blob | undefined
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ;(createObjectURLSpy as any).mockImplementation((b: Blob) => {
      capturedBlob = b
      return mockObjectUrl
    })

    renderWithProviders(<Memories />)

    // Wait for initial render then open dropdown and click CSV
    const exportBtn = await screen.findByRole('button', { name: /export memories/i })
    fireEvent.click(exportBtn)
    fireEvent.click(screen.getByRole('menuitem', { name: /export csv/i }))

    await waitFor(() => expect(capturedBlob).toBeDefined())

    // JSDOM does not implement Blob.text(); read via FileReader instead
    const text = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => resolve(reader.result as string)
      reader.onerror = reject
      reader.readAsText(capturedBlob!)
    })
    expect(text).toContain('"He said ""hello, world"""')
    expect(text).toContain('"tag1, tag2"')
  })
})
