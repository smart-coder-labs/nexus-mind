import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Harnesses from './Harnesses'

async function sha256For(content: string): Promise<string> {
  const bytes = new TextEncoder().encode(content)
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  const hex = Array.from(new Uint8Array(digest)).map(byte => byte.toString(16).padStart(2, '0')).join('')
  return `sha256:${hex}`
}

function byteLengthFor(content: string): number {
  return new TextEncoder().encode(content).length
}

const listHarnessesMock = vi.fn()
const createHarnessMock = vi.fn()
const publishHarnessVersionMock = vi.fn()
const approveHarnessInstallMock = vi.fn()
const downloadHarnessVersionMock = vi.fn()
const createHarnessConfigReviewMock = vi.fn()
const listHarnessConfigReviewsMock = vi.fn()
const listHarnessConfigReviewCommentsMock = vi.fn()
const createHarnessConfigReviewCommentMock = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listHarnesses: listHarnessesMock,
    createHarness: createHarnessMock,
    publishHarnessVersion: publishHarnessVersionMock,
    approveHarnessInstall: approveHarnessInstallMock,
    downloadHarnessVersion: downloadHarnessVersionMock,
    createHarnessConfigReview: createHarnessConfigReviewMock,
    listHarnessConfigReviews: listHarnessConfigReviewsMock,
    listHarnessConfigReviewComments: listHarnessConfigReviewCommentsMock,
    createHarnessConfigReviewComment: createHarnessConfigReviewCommentMock,
  })),
}))

const baseHarnesses = [
  {
    id: 'h-1',
    org_id: 'org-test-1',
    slug: 'claude-base',
    name: 'Claude Base',
    description: 'Shared Claude setup',
    visibility: 'org',
    status: 'published',
    created_by: 'user-admin-1',
    owner_user_id: 'user-owner-1',
    owner: { id: 'user-owner-1', name: 'Owner User', email: 'owner@example.com' },
    created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z',
      latest_version: {
        id: 'hv-1',
        version: '1.0.0',
        manifest_hash: 'sha256:abc',
        targets: ['claude'],
      format: 'hook',
      warning_metadata: {
        high_trust: true,
        requires_acknowledgement: true,
        message: 'Review executable hooks or plugin metadata before approval.',
      },
      status: 'published',
      published_at: '2026-07-01T00:00:00Z',
    },
  },
]

beforeEach(() => {
  vi.clearAllMocks()
  listHarnessesMock.mockResolvedValue(baseHarnesses)
  createHarnessMock.mockResolvedValue({ ...baseHarnesses[0], id: 'h-new', slug: 'team-open-code', name: 'Team OpenCode' })
  publishHarnessVersionMock.mockResolvedValue({
    id: 'hv-new',
    harness_id: 'h-1',
    version: '1.1.0',
    manifest: { schema_version: '1.0', targets: ['claude'] },
    manifest_hash: 'sha256:def',
    targets: ['claude'],
    provenance: { source: 'test' },
    status: 'published',
    published_by: 'user-admin-1',
    published_at: '2026-07-02T00:00:00Z',
    revoked_at: null,
  })
  approveHarnessInstallMock.mockResolvedValue({
    id: 'approval-1',
    org_id: 'org-test-1',
    user_id: 'user-admin-1',
    harness_version_id: 'hv-1',
    target_tool: 'claude',
    target_scope: 'project',
    manifest_hash: 'sha256:abc',
    status: 'approved',
    metadata: {},
    approved_at: '2026-07-02T00:00:00Z',
  })
  downloadHarnessVersionMock.mockResolvedValue({
    harness_id: 'h-1',
    version: '1.0.0',
    manifest: { schema_version: '1.0', targets: ['claude'] },
    manifest_hash: 'sha256:abc',
    approval_required: true,
  })
  createHarnessConfigReviewMock.mockResolvedValue({
    id: 'review-1',
    org_id: 'org-test-1',
    user_id: 'user-admin-1',
    source_tool: 'claude',
    redacted_config: { env: { NEXUSMIND_API_KEY: '[REDACTED:secret]' } },
    redaction_report: { secret_scan_status: 'passed', secret_count: 1, categories: { secret: 1 } },
    content_hash: 'sha256:redacted',
    status: 'shared',
    created_at: '2026-07-02T00:00:00Z',
    shared_at: '2026-07-02T00:00:00Z',
  })
  listHarnessConfigReviewsMock.mockResolvedValue([
    {
      id: 'review-1',
      org_id: 'org-test-1',
      user_id: 'user-admin-1',
      source_tool: 'claude',
      redacted_config: { env: { NEXUSMIND_API_KEY: '[REDACTED:secret]' } },
      redaction_report: { secret_scan_status: 'passed', secret_count: 1, categories: { secret: 1 } },
      content_hash: 'sha256:listedhash',
      status: 'shared',
      created_at: '2026-07-02T00:00:00Z',
      shared_at: '2026-07-02T00:00:00Z',
      author: { id: 'user-owner-1', name: 'Sarah Chen', email: 'sarah@example.com' },
    },
  ])
  listHarnessConfigReviewCommentsMock.mockResolvedValue([
    {
      id: 'comment-1',
      org_id: 'org-test-1',
      review_id: 'review-1',
      user_id: 'user-admin-1',
      body: 'This setup looks safe to reuse.',
      created_at: '2026-07-02T01:00:00Z',
      author: { id: 'user-admin-1', name: 'Admin User', email: 'admin@example.com' },
    },
  ])
  createHarnessConfigReviewCommentMock.mockResolvedValue({
    id: 'comment-2',
    org_id: 'org-test-1',
    review_id: 'review-1',
    user_id: 'user-admin-1',
    body: 'Great, approved.',
    created_at: '2026-07-02T02:00:00Z',
    author: { id: 'user-admin-1', name: 'Admin User', email: 'admin@example.com' },
  })
})

describe('Harnesses page', () => {
  it('lists harnesses and filters by target', async () => {
    renderWithProviders(<Harnesses />)

    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())
    expect(screen.getByText('sha256:abc')).toBeInTheDocument()
    expect(screen.getAllByText(/owner user/i).length).toBeGreaterThan(0)

    fireEvent.change(screen.getByLabelText(/target filter/i), { target: { value: 'claude' } })

    await waitFor(() => expect(listHarnessesMock).toHaveBeenLastCalledWith({ target: 'claude' }))

    await screen.findByRole('option', { name: /owner user/i })
    const ownerFilter = screen.getByRole('combobox', { name: /owner filter/i })
    expect(within(ownerFilter).getByRole('option', { name: /owner user/i })).toHaveAttribute('value', 'user-owner-1')
    fireEvent.change(ownerFilter, { target: { value: 'user-owner-1' } })
    await waitFor(() => expect(ownerFilter).toHaveValue('user-owner-1'))

    await waitFor(() => expect(listHarnessesMock).toHaveBeenCalledWith({ target: 'claude', owner_user_id: 'user-owner-1' }))
  })

  it('builds typed format manifests from templates and file metadata', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /publish version for claude base/i }))
    const publishDialog = await screen.findByRole('dialog', { name: /publish harness version/i })

    for (const label of ['Agent Markdown', 'Skill Markdown', 'Command Markdown', 'Hook Script', 'Output Style', 'Claude Code Plugin', 'Theme JSON']) {
      expect(within(publishDialog).getByRole('option', { name: label })).toBeInTheDocument()
    }

    fireEvent.change(within(publishDialog).getByLabelText(/version/i), { target: { value: '1.1.0' } })
    fireEvent.change(within(publishDialog).getByLabelText(/format/i), { target: { value: 'hook' } })
    expect(within(publishDialog).getByText(/executable hook/i)).toBeInTheDocument()
    const file = new File(['#!/bin/sh\nexit 0'], 'pre-commit.sh', { type: 'text/x-shellscript' })
    fireEvent.change(within(publishDialog).getByLabelText(/upload files/i), { target: { files: [file] } })
    await waitFor(() => expect(within(publishDialog).getAllByText(/pre-commit.sh/i).length).toBeGreaterThan(0))
    fireEvent.click(within(publishDialog).getByRole('button', { name: /^publish$/i }))

    await waitFor(() => expect(publishHarnessVersionMock).toHaveBeenCalledTimes(1))
    const payload = publishHarnessVersionMock.mock.calls[0][1]
    expect(payload.version).toBe('1.1.0')
    expect(payload.manifest).toEqual(expect.objectContaining({ format: 'hook', security: expect.objectContaining({ executable: true, requires_approval: true }) }))
    expect(payload.manifest.components[0]).toEqual(expect.objectContaining({
      kind: 'file',
      path: 'pre-commit.sh',
      media_type: 'text/x-shellscript',
      size_bytes: file.size,
      content: '#!/bin/sh\nexit 0',
      sha256: await sha256For('#!/bin/sh\nexit 0'),
    }))
  })

  it('builds real integrity metadata for built-in template manifests', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /publish version for claude base/i }))
    const publishDialog = await screen.findByRole('dialog', { name: /publish harness version/i })
    fireEvent.change(within(publishDialog).getByLabelText(/version/i), { target: { value: '1.1.1' } })
    await waitFor(() => expect(within(publishDialog).getByRole('button', { name: /^publish$/i })).toBeEnabled())
    fireEvent.click(within(publishDialog).getByRole('button', { name: /^publish$/i }))

    await waitFor(() => expect(publishHarnessVersionMock).toHaveBeenCalledTimes(1))
    const payload = publishHarnessVersionMock.mock.calls[0][1]
    const content = '# Example Agent'
    expect(payload.manifest.components[0]).toEqual(expect.objectContaining({
      kind: 'file',
      path: 'agents/example.md',
      media_type: 'text/markdown',
      size_bytes: byteLengthFor(content),
      sha256: await sha256For(content),
      content,
    }))
    expect(payload.manifest.components[0].sha256).not.toBe('sha256:template')
  })

  it('builds real integrity metadata for theme textarea content with utf8 bytes', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /publish version for claude base/i }))
    const publishDialog = await screen.findByRole('dialog', { name: /publish harness version/i })
    fireEvent.change(within(publishDialog).getByLabelText(/version/i), { target: { value: '1.1.2' } })
    fireEvent.change(within(publishDialog).getByLabelText(/format/i), { target: { value: 'theme' } })
    const themeJson = '{"name":"Café ☕"}'
    fireEvent.change(within(publishDialog).getByLabelText(/plugin or theme json content/i), { target: { value: themeJson } })
    await waitFor(() => expect(within(publishDialog).getByRole('button', { name: /^publish$/i })).toBeEnabled())
    fireEvent.click(within(publishDialog).getByRole('button', { name: /^publish$/i }))

    await waitFor(() => expect(publishHarnessVersionMock).toHaveBeenCalledTimes(1))
    const payload = publishHarnessVersionMock.mock.calls[0][1]
    expect(payload.manifest.components[0]).toEqual(expect.objectContaining({
      kind: 'theme_json',
      path: 'themes/example.json',
      media_type: 'application/json',
      size_bytes: byteLengthFor(themeJson),
      sha256: await sha256For(themeJson),
      content: themeJson,
    }))
    expect(payload.manifest.components[0].size_bytes).toBeGreaterThan(themeJson.length)
  })

  it('packages folder uploads with normalized entry metadata', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /publish version for claude base/i }))
    const publishDialog = await screen.findByRole('dialog', { name: /publish harness version/i })
    fireEvent.change(within(publishDialog).getByLabelText(/version/i), { target: { value: '1.2.0' } })
    fireEvent.change(within(publishDialog).getByLabelText(/format/i), { target: { value: 'skill' } })
    const skill = new File(['---\nname: reviewer\n---'], 'SKILL.md', { type: 'text/markdown' })
    Object.defineProperty(skill, 'webkitRelativePath', { value: 'reviewer/SKILL.md' })
    const notes = new File(['# Notes'], 'README.md', { type: 'text/markdown' })
    Object.defineProperty(notes, 'webkitRelativePath', { value: 'reviewer/docs/README.md' })

    fireEvent.change(within(publishDialog).getByLabelText(/upload files/i), { target: { files: [skill, notes] } })
    await waitFor(() => expect(within(publishDialog).getAllByText(/reviewer\/SKILL.md/i).length).toBeGreaterThan(0))
    fireEvent.click(within(publishDialog).getByRole('button', { name: /^publish$/i }))

    await waitFor(() => expect(publishHarnessVersionMock).toHaveBeenCalledTimes(1))
    const payload = publishHarnessVersionMock.mock.calls[0][1]
    expect(payload.version).toBe('1.2.0')
    expect(payload.manifest).toEqual(expect.objectContaining({ format: 'skill' }))
    expect(payload.manifest.components).toHaveLength(1)
    expect(payload.manifest.components[0]).toEqual(expect.objectContaining({ kind: 'folder' }))
    expect(payload.manifest.components[0].entries).toEqual([
      expect.objectContaining({
        path: 'reviewer/SKILL.md',
        media_type: 'text/markdown',
        size_bytes: skill.size,
        content: '---\nname: reviewer\n---',
        sha256: await sha256For('---\nname: reviewer\n---'),
      }),
      expect.objectContaining({
        path: 'reviewer/docs/README.md',
        media_type: 'text/markdown',
        size_bytes: notes.size,
        content: '# Notes',
        sha256: await sha256For('# Notes'),
      }),
    ])
  })

  it('reads uploaded plugin json content before publishing', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /publish version for claude base/i }))
    const publishDialog = await screen.findByRole('dialog', { name: /publish harness version/i })
    fireEvent.change(within(publishDialog).getByLabelText(/version/i), { target: { value: '1.2.1' } })
    fireEvent.change(within(publishDialog).getByLabelText(/format/i), { target: { value: 'claude_code_plugin' } })

    const pluginJson = '{"name":"reviewer","version":"1.0.0"}'
    const plugin = new File([pluginJson], 'reviewer.json', { type: 'application/json' })
    fireEvent.change(within(publishDialog).getByLabelText(/upload files/i), { target: { files: [plugin] } })
    await waitFor(() => expect(within(publishDialog).getAllByText(/reviewer.json/i).length).toBeGreaterThan(0))
    fireEvent.click(within(publishDialog).getByRole('button', { name: /^publish$/i }))

    await waitFor(() => expect(publishHarnessVersionMock).toHaveBeenCalledTimes(1))
    const payload = publishHarnessVersionMock.mock.calls[0][1]
    expect(payload.manifest.components[0]).toEqual(expect.objectContaining({
      kind: 'plugin_marketplace',
      path: 'reviewer.json',
      media_type: 'application/json',
      size_bytes: plugin.size,
      content: pluginJson,
      sha256: await sha256For(pluginJson),
    }))
  })

  it('blocks unsupported multi-file upload flows for single-file formats', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /publish version for claude base/i }))
    const publishDialog = await screen.findByRole('dialog', { name: /publish harness version/i })
    fireEvent.change(within(publishDialog).getByLabelText(/version/i), { target: { value: '1.2.2' } })
    fireEvent.change(within(publishDialog).getByLabelText(/format/i), { target: { value: 'hook' } })

    const one = new File(['#!/bin/sh\nexit 0'], 'pre-commit.sh', { type: 'text/x-shellscript' })
    const two = new File(['#!/bin/sh\necho second'], 'post-commit.sh', { type: 'text/x-shellscript' })
    await act(async () => {
      fireEvent.change(within(publishDialog).getByLabelText(/upload files/i), { target: { files: [one, two] } })
    })

    expect(await within(publishDialog).findByText(/this format accepts a single uploaded file/i)).toBeInTheDocument()
    // Wait for the hook template manifest prep to settle so the button reads "Publish", not "Preparing manifest…".
    const publishButton = await within(publishDialog).findByRole('button', { name: /^publish$/i })
    await act(async () => {
      fireEvent.click(publishButton)
    })
    expect(publishHarnessVersionMock).not.toHaveBeenCalled()
  })

  it('rejects invalid plugin and theme JSON before publishing', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /publish version for claude base/i }))
    const publishDialog = await screen.findByRole('dialog', { name: /publish harness version/i })
    fireEvent.change(within(publishDialog).getByLabelText(/version/i), { target: { value: '1.3.0' } })
    fireEvent.change(within(publishDialog).getByLabelText(/format/i), { target: { value: 'claude_code_plugin' } })
    fireEvent.change(within(publishDialog).getByLabelText(/plugin or theme json content/i), { target: { value: '{invalid' } })
    await waitFor(() => expect(within(publishDialog).getByRole('button', { name: /^publish$/i })).toBeEnabled())
    fireEvent.click(within(publishDialog).getByRole('button', { name: /^publish$/i }))
    expect(await within(publishDialog).findByText(/plugin and theme content must be valid json object/i)).toBeInTheDocument()
    expect(publishHarnessVersionMock).not.toHaveBeenCalled()

    fireEvent.change(within(publishDialog).getByLabelText(/format/i), { target: { value: 'theme' } })
    fireEvent.change(within(publishDialog).getByLabelText(/plugin or theme json content/i), { target: { value: '[]' } })
    await waitFor(() => expect(within(publishDialog).getByRole('button', { name: /^publish$/i })).toBeEnabled())
    fireEvent.click(within(publishDialog).getByRole('button', { name: /^publish$/i }))
    expect(await within(publishDialog).findByText(/plugin and theme content must be valid json object/i)).toBeInTheDocument()
    expect(publishHarnessVersionMock).not.toHaveBeenCalled()
  })

  it('creates a format-aware harness and publishes its initial version in one flow', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /new harness/i }))
    const createDialog = await screen.findByRole('dialog', { name: /create harness/i })
    fireEvent.change(within(createDialog).getByLabelText(/name/i), { target: { value: 'Team OpenCode' } })
    fireEvent.change(within(createDialog).getByLabelText(/slug/i), { target: { value: 'team-open-code' } })

    // Creation is format-aware: the seven format templates are selectable up front.
    for (const label of ['Agent Markdown', 'Skill Markdown', 'Command Markdown', 'Hook Script', 'Output Style', 'Claude Code Plugin', 'Theme JSON']) {
      expect(within(createDialog).getByRole('option', { name: label })).toBeInTheDocument()
    }

    // Wait for the initial (agent) manifest integrity metadata to be prepared.
    await waitFor(() => {
      const preview = within(createDialog).getByLabelText(/manifest json/i) as HTMLTextAreaElement
      expect(preview.value).toContain('"format": "agent"')
    })
    fireEvent.click(within(createDialog).getByRole('button', { name: /^create$/i }))

    await waitFor(() => expect(createHarnessMock).toHaveBeenCalledWith(expect.objectContaining({ slug: 'team-open-code', name: 'Team OpenCode' })))
    await waitFor(() => expect(publishHarnessVersionMock).toHaveBeenCalledWith('h-new', expect.objectContaining({
      version: '1.0.0',
      manifest: expect.objectContaining({ format: 'agent' }),
    })))
  })

  it('requires explicit approval before downloading a manifest', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /download claude base/i }))
    const dialog = await screen.findByRole('dialog', { name: /approve harness download/i })
    expect(within(dialog).getByText(/nexusmind will not mutate local files/i)).toBeInTheDocument()
    expect(within(dialog).getByText(/review executable hooks or plugin metadata before approval/i)).toBeInTheDocument()
    expect(within(dialog).getByRole('button', { name: /approve and download/i })).toBeDisabled()
    fireEvent.click(within(dialog).getByRole('checkbox', { name: /i reviewed and acknowledge/i }))
    fireEvent.click(within(dialog).getByRole('button', { name: /approve and download/i }))

    await waitFor(() => expect(approveHarnessInstallMock).toHaveBeenCalledWith('h-1', '1.0.0', expect.objectContaining({
      manifest_hash: 'sha256:abc',
      metadata: expect.objectContaining({ warning_acknowledged: true }),
    })))
    await waitFor(() => expect(downloadHarnessVersionMock).toHaveBeenCalledWith('h-1', '1.0.0'))
  })

  it('auto-redacts a pasted config locally before sharing a review snapshot', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    expect(screen.getByText(/local redaction → preview → approve/i)).toBeInTheDocument()
    expect(screen.queryByText(/raw low-level/i)).not.toBeInTheDocument()

    const raw = JSON.stringify({ env: { NEXUSMIND_API_KEY: 'nm_live_super_secret' }, model: 'claude-opus' })
    fireEvent.change(screen.getByLabelText(/paste your claude config/i), { target: { value: raw } })

    // The redaction runs in the browser and the summary + redacted preview appear.
    expect(await screen.findByText(/redaction summary/i)).toBeInTheDocument()
    expect(screen.getAllByText(/redacted preview/i).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/token/i).length).toBeGreaterThan(0)

    const submitButton = screen.getByRole('button', { name: /submit config review/i })
    await waitFor(() => expect(submitButton).toBeEnabled())
    fireEvent.click(submitButton)

    await waitFor(() => expect(createHarnessConfigReviewMock).toHaveBeenCalledTimes(1))
    const payload = createHarnessConfigReviewMock.mock.calls[0][0]
    expect(payload.source_tool).toBe('claude')
    expect(payload.content_hash).toMatch(/^sha256:/)
    expect(payload.redaction_report).toEqual(expect.objectContaining({ secret_scan_status: 'passed' }))
    // The raw secret never leaves the browser; only the redacted snapshot is shared.
    const serialized = JSON.stringify(payload.redacted_config)
    expect(serialized).not.toContain('nm_live_super_secret')
    expect(serialized).toContain('[REDACTED')
  })

  it('redacts every backend-flagged secret pattern nested anywhere in the config', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    const raw = JSON.stringify({
      openai: 'sk-abcdef1234567890',
      github: { token: 'ghp_examplevalue0000' },
      slack: ['xoxb-000-111-abc'],
      header: 'Authorization: Bearer sometoken',
      note: 'contains raw-secret marker',
      home: '/Users/cesar/.claude/config',
      safe: 'claude-opus',
    })
    fireEvent.change(screen.getByLabelText(/paste your claude config/i), { target: { value: raw } })

    const submitButton = screen.getByRole('button', { name: /submit config review/i })
    await waitFor(() => expect(submitButton).toBeEnabled())
    fireEvent.click(submitButton)

    await waitFor(() => expect(createHarnessConfigReviewMock).toHaveBeenCalledTimes(1))
    const serialized = JSON.stringify(createHarnessConfigReviewMock.mock.calls[0][0].redacted_config)
    // None of the backend has_secret_indicator triggers may survive redaction.
    for (const leaked of ['sk-abcdef', 'ghp_example', 'xoxb-000', 'Bearer sometoken', 'raw-secret', '/Users/cesar']) {
      expect(serialized).not.toContain(leaked)
    }
    // Non-sensitive values are preserved.
    expect(serialized).toContain('claude-opus')
  })

  it('lists shared config reviews and inspects the redacted snapshot', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    expect(await screen.findByText(/shared config reviews/i)).toBeInTheDocument()
    await waitFor(() => expect(listHarnessConfigReviewsMock).toHaveBeenCalled())
    expect(screen.getByText('sha256:listedhash')).toBeInTheDocument()
    expect(screen.getByText(/1 redaction \(secret ×1\)/i)).toBeInTheDocument()
    // The author of the shared config is shown.
    expect(screen.getByText(/by Sarah Chen/i)).toBeInTheDocument()

    // The redacted config is hidden until explicitly inspected.
    expect(screen.queryByText(/"NEXUSMIND_API_KEY"/)).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /inspect config review review-1/i }))
    expect(await screen.findByText(/\[REDACTED:secret\]/)).toBeInTheDocument()
  })

  it('shows comments and posts a new one on an inspected config review', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    await screen.findByText(/shared config reviews/i)
    fireEvent.click(screen.getByRole('button', { name: /inspect config review review-1/i }))

    // Existing comments load with their author.
    expect(await screen.findByText(/this setup looks safe to reuse/i)).toBeInTheDocument()
    await waitFor(() => expect(listHarnessConfigReviewCommentsMock).toHaveBeenCalledWith('review-1'))

    fireEvent.change(screen.getByLabelText(/add a comment/i), { target: { value: 'Great, approved.' } })
    fireEvent.click(screen.getByRole('button', { name: /post comment/i }))

    await waitFor(() => expect(createHarnessConfigReviewCommentMock).toHaveBeenCalledWith('review-1', { body: 'Great, approved.' }))
  })
})
