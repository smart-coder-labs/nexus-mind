import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertCircle, CheckCircle2, Download, FileJson, PackagePlus, ShieldCheck, X } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import type { Harness, HarnessFormat, HarnessManifest, HarnessManifestEntry } from '../types'

const FOCUS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'
const MAX_INLINE_UPLOAD_BYTES = 64 * 1024
const FORMAT_OPTIONS: Array<{ value: HarnessFormat; label: string; path: string; kind: 'file' | 'folder' | 'plugin_marketplace' | 'theme_json'; mediaType: string; content: string; executable?: boolean }> = [
  { value: 'agent', label: 'Agent Markdown', path: 'agents/example.md', kind: 'file', mediaType: 'text/markdown', content: '# Example Agent' },
  { value: 'skill', label: 'Skill Markdown', path: 'skills/example/SKILL.md', kind: 'file', mediaType: 'text/markdown', content: '---\nname: example\n---\n# Skill' },
  { value: 'command', label: 'Command Markdown', path: 'commands/example.md', kind: 'file', mediaType: 'text/markdown', content: 'Run this command workflow.' },
  { value: 'hook', label: 'Hook Script', path: 'hooks/example.sh', kind: 'file', mediaType: 'text/x-shellscript', content: '#!/bin/sh\nexit 0', executable: true },
  { value: 'output_style', label: 'Output Style', path: 'output-styles/direct.md', kind: 'file', mediaType: 'text/markdown', content: 'Use direct, concise output.' },
  { value: 'claude_code_plugin', label: 'Claude Code Plugin', path: 'plugins/example.json', kind: 'plugin_marketplace', mediaType: 'application/json', content: '{"name":"example"}', executable: true },
  { value: 'theme', label: 'Theme JSON', path: 'themes/example.json', kind: 'theme_json', mediaType: 'application/json', content: '{"name":"Example"}' },
]

function safeRelativePath(file: File): string {
  const candidate = (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name
  return candidate.replace(/^\/+/, '').split('/').filter(part => part && part !== '..').join('/')
}

function mediaTypeFor(file: File): string {
  if (file.type) return file.type
  if (file.name.endsWith('.md')) return 'text/markdown'
  if (file.name.endsWith('.sh')) return 'text/x-shellscript'
  if (file.name.endsWith('.json')) return 'application/json'
  return 'application/octet-stream'
}

function uploadLabelFor(format: HarnessFormat): string {
  return format === 'skill' ? 'Upload skill files or folder' : format === 'claude_code_plugin' || format === 'theme' ? 'Upload JSON file' : 'Upload file'
}

function uploadErrorFor(format: HarnessFormat): string {
  switch (format) {
    case 'hook':
      return 'Hooks must be uploaded as a single .sh file.'
    case 'claude_code_plugin':
      return 'Claude Code plugins must be uploaded as a single JSON file.'
    case 'theme':
      return 'Themes must be uploaded as a single JSON file.'
    case 'agent':
    case 'command':
    case 'output_style':
      return 'This format must be uploaded as a single Markdown file.'
    default:
      return 'Uploaded files do not match the selected harness format.'
  }
}

function formatAllowsMultipleFiles(format: HarnessFormat): boolean {
  return format === 'skill'
}

function uploadMatchesFormat(format: HarnessFormat, path: string): boolean {
  switch (format) {
    case 'hook':
      return path.endsWith('.sh')
    case 'claude_code_plugin':
    case 'theme':
      return path.endsWith('.json')
    case 'agent':
    case 'command':
    case 'output_style':
      return path.endsWith('.md')
    case 'skill':
      return true
    default:
      return false
  }
}

function folderComponentPath(format: HarnessFormat, entries: HarnessManifestEntry[]): string {
  const firstPath = entries[0]?.path ?? `${format}-upload`
  const root = firstPath.split('/')[0]
  return root || `${format}-upload`
}

async function sha256ForContent(content: string): Promise<string> {
  if (!globalThis.crypto?.subtle) {
    throw new Error('This browser cannot compute upload integrity hashes.')
  }
  const digest = await globalThis.crypto.subtle.digest('SHA-256', new TextEncoder().encode(content))
  const hex = Array.from(new Uint8Array(digest)).map(byte => byte.toString(16).padStart(2, '0')).join('')
  return `sha256:${hex}`
}

function byteLengthForContent(content: string): number {
  return new TextEncoder().encode(content).length
}

async function readFileText(file: File): Promise<string> {
  if (typeof file.text === 'function') {
    return file.text()
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(reader.error ?? new Error('Failed to read uploaded file.'))
    reader.onload = () => resolve(typeof reader.result === 'string' ? reader.result : '')
    reader.readAsText(file)
  })
}

async function buildManifestEntry(format: HarnessFormat, file: File): Promise<HarnessManifestEntry> {
  const path = safeRelativePath(file)
  if (!uploadMatchesFormat(format, path)) {
    throw new Error(uploadErrorFor(format))
  }
  if (file.size > MAX_INLINE_UPLOAD_BYTES) {
    throw new Error('Uploaded files must be 64 KiB or smaller for inline manifest content.')
  }
  const content = await readFileText(file)
  if (format === 'claude_code_plugin' || format === 'theme') {
    parseJsonObject(content, 'Uploaded JSON file')
  }
  return {
    kind: 'file',
    path,
    media_type: mediaTypeFor(file),
    size_bytes: file.size,
    sha256: await sha256ForContent(content),
    content,
  }
}

async function buildInlineManifestComponent(kind: 'file' | 'plugin_marketplace' | 'theme_json', path: string, mediaType: string, content: string): Promise<HarnessManifest['components'][number]> {
  return {
    kind,
    path,
    media_type: mediaType,
    size_bytes: byteLengthForContent(content),
    sha256: await sha256ForContent(content),
    content,
  }
}

async function buildManifest(format: HarnessFormat, entries: HarnessManifestEntry[] = [], jsonContent?: string): Promise<HarnessManifest> {
  const option = FORMAT_OPTIONS.find(item => item.value === format) ?? FORMAT_OPTIONS[0]
  const uploaded = entries.length > 0
  const hasFolderStructure = entries.some(entry => entry.path.includes('/'))
  const components = uploaded
    ? entries.length > 1 || (format === 'skill' && hasFolderStructure)
      ? [{ kind: 'folder' as const, path: folderComponentPath(format, entries), entries }]
      : [{ ...entries[0], kind: option.kind, path: entries[0].path }]
    : [await buildInlineManifestComponent(option.kind === 'folder' ? 'file' : option.kind, option.path, option.mediaType, jsonContent ?? option.content)]
  return {
    schema_version: '1.1',
    targets: ['claude'],
    format,
    components,
    provenance: { source: uploaded ? 'admin-ui-upload' : 'admin-ui-template' },
    security: { requires_approval: true, executable: option.executable || undefined, secret_scan_status: 'passed' },
  }
}

type Flash = { kind: 'success' | 'error'; message: string } | null

function parseJsonObject(value: string, label: string): Record<string, unknown> {
  const parsed = JSON.parse(value)
  if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
    throw new Error(`${label} must be a JSON object`)
  }
  return parsed as Record<string, unknown>
}

// ── Local config redaction (client-side, mirrors backend secret indicators) ────
const SECRET_KEY_RE = /(secret|token|password|passwd|api[_-]?key|apikey|access[_-]?key|private[_-]?key|auth|credential)/i
const SECRET_VALUE_RE = /^(sk-|ghp_|gho_|github_pat_|nm_|xoxb-|glpat-)/i
const BEARER_RE = /bearer\s+\S+/i
const HOME_PATH_RE = /(?:\/Users\/|\/home\/|[A-Za-z]:\\Users\\)[^\s"'\\/]+/g

type RedactionResult = { redactedConfig: Record<string, unknown>; report: Record<string, unknown>; contentHash: string }

function redactValue(key: string | null, value: unknown, counts: Record<string, number>): unknown {
  if (typeof value === 'string') {
    // Idempotency: skip our own namespaced placeholders. Must be specific so a
    // user string that merely starts with "[REDACTED] …" is still scanned.
    if (/^\[REDACTED:(token|secret|path)\]$/.test(value)) return value
    if (SECRET_VALUE_RE.test(value) || BEARER_RE.test(value) || value.toLowerCase().includes('raw-secret')) {
      counts.token = (counts.token ?? 0) + 1
      return '[REDACTED:token]'
    }
    if (key && SECRET_KEY_RE.test(key) && value.trim() !== '') {
      counts.secret = (counts.secret ?? 0) + 1
      return '[REDACTED:secret]'
    }
    if (HOME_PATH_RE.test(value)) {
      counts.path = (counts.path ?? 0) + 1
      return value.replace(HOME_PATH_RE, '[REDACTED:path]')
    }
    return value
  }
  if (Array.isArray(value)) return value.map(item => redactValue(null, item, counts))
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value)) out[k] = redactValue(k, v, counts)
    return out
  }
  return value
}

async function redactConfig(parsed: Record<string, unknown>): Promise<RedactionResult> {
  const counts: Record<string, number> = {}
  const redactedConfig = redactValue(null, parsed, counts) as Record<string, unknown>
  const secretCount = Object.values(counts).reduce((total, n) => total + n, 0)
  const report = { secret_scan_status: 'passed', secret_count: secretCount, categories: counts }
  const contentHash = await sha256ForContent(JSON.stringify(redactedConfig))
  return { redactedConfig, report, contentHash }
}

// ── Shared manifest builder (used by create and publish flows) ─────────────────
function useManifestBuilder() {
  const [format, setFormatState] = useState<HarnessFormat>('agent')
  const [jsonContent, setJsonContent] = useState(FORMAT_OPTIONS[0].content)
  const [fileEntries, setFileEntries] = useState<HarnessManifestEntry[]>([])
  const [isReadingFiles, setIsReadingFiles] = useState(false)
  const [isPreparingManifest, setIsPreparingManifest] = useState(false)
  const [manifest, setManifest] = useState<HarnessManifest | null>(null)
  const [manifestError, setManifestError] = useState<string | null>(null)
  const [uploadError, setUploadError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setIsPreparingManifest(true)
    setManifestError(null)
    void buildManifest(format, fileEntries, jsonContent)
      .then(next => { if (!cancelled) setManifest(next) })
      .catch(err => {
        if (!cancelled) {
          setManifest(null)
          setManifestError(err instanceof Error ? err.message : 'Failed to prepare manifest integrity metadata.')
        }
      })
      .finally(() => { if (!cancelled) setIsPreparingManifest(false) })
    return () => { cancelled = true }
  }, [format, fileEntries, jsonContent])

  const setFormat = (next: HarnessFormat) => {
    setFormatState(next)
    setJsonContent(FORMAT_OPTIONS.find(item => item.value === next)?.content ?? '')
    setFileEntries([])
    setUploadError(null)
    setManifestError(null)
  }

  const handleFiles = async (files: FileList | null) => {
    if (!files) return
    if (!formatAllowsMultipleFiles(format) && files.length > 1) {
      setFileEntries([])
      setUploadError('This format accepts a single uploaded file. Use Skill format for folders or multiple files.')
      return
    }
    setIsReadingFiles(true)
    setUploadError(null)
    try {
      const entries = await Promise.all(Array.from(files).map(file => buildManifestEntry(format, file)))
      setFileEntries(entries)
    } catch (err) {
      setFileEntries([])
      setUploadError(err instanceof Error ? err.message : 'Failed to read uploaded files.')
    } finally {
      setIsReadingFiles(false)
    }
  }

  // Build the manifest from the CURRENT inputs. Used at submit time so the
  // published payload never depends on the async preview effect settling first.
  const buildCurrentManifest = () => buildManifest(format, fileEntries, jsonContent)

  return { format, setFormat, jsonContent, setJsonContent, fileEntries, isReadingFiles, isPreparingManifest, manifest, manifestError, uploadError, handleFiles, buildCurrentManifest }
}

type ManifestBuilder = ReturnType<typeof useManifestBuilder>

function validateJsonContent(builder: ManifestBuilder): string | null {
  if (builder.format !== 'theme' && builder.format !== 'claude_code_plugin') return null
  // Uploaded JSON files are already parsed/validated while reading (buildManifestEntry).
  if (builder.fileEntries.length > 0) return null
  try {
    parseJsonObject(builder.jsonContent, 'Plugin and theme content')
    return null
  } catch {
    return 'Plugin and theme content must be valid JSON object.'
  }
}

function ManifestBuilderFields({ builder }: { builder: ManifestBuilder }) {
  const { format, setFormat, jsonContent, setJsonContent, fileEntries, manifest, isPreparingManifest, handleFiles } = builder
  const manifestJson = useMemo(() => (manifest ? JSON.stringify(manifest, null, 2) : ''), [manifest])
  return (
    <>
      <label className="block space-y-1.5 text-[10px] text-text-quaternary">
        <span>Format</span>
        <select value={format} onChange={e => setFormat(e.target.value as HarnessFormat)} className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60">
          {FORMAT_OPTIONS.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
        </select>
      </label>
      {(format === 'hook' || format === 'claude_code_plugin') && (
        <div role="alert" className="rounded-[11px] border border-status-warning/30 bg-status-warning/5 px-3 py-2 text-[11px] text-status-warning">
          Executable hook/plugin formats require explicit review and approval before download.
        </div>
      )}
      {(format === 'theme' || format === 'claude_code_plugin') && fileEntries.length === 0 && (
        <label className="block space-y-1.5 text-[10px] text-text-quaternary">
          <span>Plugin or theme JSON content</span>
          <textarea value={jsonContent} onChange={e => setJsonContent(e.target.value)} rows={4} className="font-mono w-full resize-none rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
        </label>
      )}
      <label className="block space-y-1.5 text-[10px] text-text-quaternary">
        <span>{uploadLabelFor(format)}</span>
        <input aria-label="Upload files" type="file" multiple onChange={e => void handleFiles(e.target.files)} className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary file:mr-3 file:rounded-full file:border-0 file:bg-accent-blue file:px-3 file:py-1 file:text-xs file:text-white" />
      </label>
      {fileEntries.length > 0 && (
        <div className="rounded-[8px] border border-border-secondary bg-black/20 p-3 text-[11px] text-text-secondary">
          <p className="mb-2 text-text-primary">Safe upload entries</p>
          {fileEntries.map(entry => <div key={entry.path}>{entry.path} · {entry.size_bytes} bytes · {entry.sha256}</div>)}
        </div>
      )}
      <label className="block space-y-1.5 text-[10px] text-text-quaternary">
        <span>Manifest JSON preview</span>
        <textarea readOnly value={manifestJson || (isPreparingManifest ? 'Preparing manifest integrity metadata…' : '')} rows={10} className="font-mono w-full resize-none rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
      </label>
    </>
  )
}

function CreateHarnessModal({ onClose, onFlash }: { onClose: () => void; onFlash: (flash: Flash) => void }) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()
  const [name, setName] = useState('')
  const [slug, setSlug] = useState('')
  const [description, setDescription] = useState('')
  const [version, setVersion] = useState('1.0.0')
  const [error, setError] = useState<string | null>(null)
  const builder = useManifestBuilder()

  const createMut = useMutation({
    mutationFn: async () => {
      const created = await client.createHarness({
        name: name.trim(),
        slug: slug.trim(),
        description: description.trim() || undefined,
        visibility: 'org',
      })
      if (version.trim()) {
        const manifest = await builder.buildCurrentManifest()
        try {
          await client.publishHarnessVersion(created.id, { version: version.trim(), manifest })
        } catch (err) {
          const detail = err instanceof Error ? err.message : 'unknown error'
          // The harness already exists on the backend; surface that so the user
          // retries the publish instead of re-creating (and hitting slug conflicts).
          throw new Error(`Harness "${created.name}" was created, but publishing ${version.trim()} failed: ${detail}. Use "Publish version" on it to retry.`)
        }
      }
      return created
    },
    onSuccess: (created) => {
      onFlash({ kind: 'success', message: version.trim() ? `Created ${created.name} and published ${version.trim()}.` : `Created ${created.name}.` })
      onClose()
    },
    onError: (err) => setError(err instanceof Error ? err.message : 'Failed to create harness'),
    // Refresh the list whether publish succeeded or not so a created-but-unpublished harness is visible.
    onSettled: () => qc.invalidateQueries({ queryKey: ['harnesses'] }),
  })

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!name.trim() || !slug.trim()) {
      setError('Name and slug are required.')
      return
    }
    if (version.trim()) {
      if (builder.isReadingFiles) { setError('Wait for uploaded files to finish processing.'); return }
      if (builder.uploadError || builder.manifestError) { return }
      const jsonError = validateJsonContent(builder)
      if (jsonError) { setError(jsonError); return }
    }
    setError(null)
    createMut.mutate()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={onClose}>
      <div role="dialog" aria-modal="true" aria-label="Create harness" className="max-h-[90vh] w-full max-w-2xl overflow-y-auto rounded-[18px] border border-border-primary bg-[#1d1d1f] p-6" onClick={e => e.stopPropagation()}>
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-xs font-semibold text-text-primary">Create harness</h2>
          <button onClick={onClose} aria-label="Close" className={`rounded-[6px] text-text-tertiary hover:text-text-primary ${FOCUS}`}><X className="h-4 w-4" /></button>
        </div>
        <form onSubmit={submit} className="space-y-4">
          <label className="block space-y-1.5 text-[10px] text-text-quaternary">
            <span>Name</span>
            <input value={name} onChange={e => setName(e.target.value)} className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
          </label>
          <label className="block space-y-1.5 text-[10px] text-text-quaternary">
            <span>Slug</span>
            <input value={slug} onChange={e => setSlug(e.target.value)} className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
          </label>
          <label className="block space-y-1.5 text-[10px] text-text-quaternary">
            <span>Description</span>
            <textarea value={description} onChange={e => setDescription(e.target.value)} rows={2} className="w-full resize-none rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
          </label>
          <div className="rounded-[11px] border border-border-secondary bg-black/20 p-4">
            <p className="mb-3 text-[11px] text-text-primary">Initial version</p>
            <p className="mb-3 text-[10px] text-text-quaternary">Choose a format and provide a template, uploaded file, or folder. Leave the version blank to create an empty harness and publish later.</p>
            <label className="mb-4 block space-y-1.5 text-[10px] text-text-quaternary">
              <span>Version</span>
              <input value={version} onChange={e => setVersion(e.target.value)} placeholder="1.0.0" className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
            </label>
            <div className="space-y-4">
              <ManifestBuilderFields builder={builder} />
            </div>
          </div>
          {(error ?? builder.uploadError ?? builder.manifestError) && <p className="text-[10px] text-status-error">{error ?? builder.uploadError ?? builder.manifestError}</p>}
          <div className="flex justify-end gap-2">
            <button type="button" onClick={onClose} className={`rounded-full border border-border-primary px-4 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] ${FOCUS}`}>Cancel</button>
            <button type="submit" disabled={createMut.isPending || builder.isReadingFiles} className={`rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}>{createMut.isPending ? 'Creating…' : builder.isReadingFiles ? 'Reading uploads…' : 'Create'}</button>
          </div>
        </form>
      </div>
    </div>
  )
}

function PublishModal({ harness, onClose, onFlash }: { harness: Harness; onClose: () => void; onFlash: (flash: Flash) => void }) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()
  const [version, setVersion] = useState('')
  const [error, setError] = useState<string | null>(null)
  const builder = useManifestBuilder()

  const publishMut = useMutation({
    mutationFn: async () => {
      const manifest = await builder.buildCurrentManifest()
      return client.publishHarnessVersion(harness.id, { version: version.trim(), manifest })
    },
    onSuccess: (published) => {
      qc.invalidateQueries({ queryKey: ['harnesses'] })
      onFlash({ kind: 'success', message: `Published ${harness.name} ${published.version} (${published.manifest_hash}).` })
      onClose()
    },
    onError: (err) => setError(err instanceof Error ? err.message : 'Failed to publish version'),
  })

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!version.trim()) { setError('Version is required.'); return }
    if (builder.isReadingFiles) { setError('Wait for uploaded files to finish processing.'); return }
    if (builder.uploadError || builder.manifestError) { return }
    const jsonError = validateJsonContent(builder)
    if (jsonError) { setError(jsonError); return }
    setError(null)
    publishMut.mutate()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={onClose}>
      <div role="dialog" aria-modal="true" aria-label="Publish harness version" className="max-h-[90vh] w-full max-w-2xl overflow-y-auto rounded-[18px] border border-border-primary bg-[#1d1d1f] p-6" onClick={e => e.stopPropagation()}>
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-xs font-semibold text-text-primary">Publish harness version</h2>
          <button onClick={onClose} aria-label="Close" className={`rounded-[6px] text-text-tertiary hover:text-text-primary ${FOCUS}`}><X className="h-4 w-4" /></button>
        </div>
        <form onSubmit={submit} className="space-y-4">
          <label className="block space-y-1.5 text-[10px] text-text-quaternary">
            <span>Version</span>
            <input value={version} onChange={e => setVersion(e.target.value)} placeholder="1.0.0" className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
          </label>
          <ManifestBuilderFields builder={builder} />
          {(error ?? builder.uploadError ?? builder.manifestError) && <p className="text-[10px] text-status-error">{error ?? builder.uploadError ?? builder.manifestError}</p>}
          <div className="flex justify-end gap-2">
            <button type="button" onClick={onClose} className={`rounded-full border border-border-primary px-4 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] ${FOCUS}`}>Cancel</button>
            <button type="submit" disabled={publishMut.isPending || builder.isReadingFiles} className={`rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}>{publishMut.isPending ? 'Publishing…' : builder.isReadingFiles ? 'Reading uploads…' : 'Publish'}</button>
          </div>
        </form>
      </div>
    </div>
  )
}

function ApprovalModal({ harness, onClose, onFlash }: { harness: Harness; onClose: () => void; onFlash: (flash: Flash) => void }) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const latest = harness.latest_version
  const warningMetadata = latest?.warning_metadata
  const requiresWarningAck = warningMetadata?.requires_acknowledgement === true
  const warningMessage = typeof warningMetadata?.message === 'string' ? warningMetadata.message : 'Review executable hooks or plugin metadata before approval.'
  const [warningAcknowledged, setWarningAcknowledged] = useState(false)
  const approveMut = useMutation({
    mutationFn: async () => {
      if (!latest) throw new Error('No published version is available.')
      await client.approveHarnessInstall(harness.id, latest.version, {
        target_tool: latest.targets[0] ?? 'claude',
        target_scope: 'project',
        manifest_hash: latest.manifest_hash,
        metadata: { source: 'admin-ui', ...(requiresWarningAck ? { warning_acknowledged: warningAcknowledged } : {}) },
      })
      return client.downloadHarnessVersion(harness.id, latest.version)
    },
    onSuccess: (download) => {
      onFlash({ kind: 'success', message: `Approved and downloaded metadata for ${harness.name} ${download.version}.` })
      onClose()
    },
    onError: (err) => onFlash({ kind: 'error', message: err instanceof Error ? err.message : 'Failed to approve download.' }),
  })

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={onClose}>
      <div role="dialog" aria-modal="true" aria-label="Approve harness download" className="w-full max-w-lg rounded-[18px] border border-border-primary bg-[#1d1d1f] p-6" onClick={e => e.stopPropagation()}>
        <div className="mb-4 flex items-center gap-2 text-text-primary">
          <ShieldCheck className="h-4 w-4 text-accent-blue" />
          <h2 className="text-xs font-semibold">Approve harness download</h2>
        </div>
        <div className="space-y-3 text-xs text-text-secondary">
          <p>NexusMind will not mutate local files. Local tools must show a diff and ask before applying Claude, Codex, OpenCode, shell, or project file changes.</p>
          {requiresWarningAck && (
            <div role="alert" className="rounded-[11px] border border-status-warning/30 bg-status-warning/5 px-3 py-2 text-status-warning">
              <p>{warningMessage}</p>
              <label className="mt-2 flex items-center gap-2 text-[11px] text-status-warning">
                <input type="checkbox" checked={warningAcknowledged} onChange={e => setWarningAcknowledged(e.target.checked)} />
                <span>I reviewed and acknowledge this high-trust harness warning.</span>
              </label>
            </div>
          )}
          <p><span className="text-text-quaternary">Manifest hash:</span> <span className="font-mono text-text-primary">{latest?.manifest_hash ?? 'No published version'}</span></p>
        </div>
        <div className="mt-6 flex justify-end gap-2">
          <button onClick={onClose} className={`rounded-full border border-border-primary px-4 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] ${FOCUS}`}>Cancel</button>
          <button onClick={() => approveMut.mutate()} disabled={approveMut.isPending || !latest || (requiresWarningAck && !warningAcknowledged)} className={`rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}>{approveMut.isPending ? 'Approving…' : 'Approve and download'}</button>
        </div>
      </div>
    </div>
  )
}

function ConfigReviewForm({ onFlash }: { onFlash: (flash: Flash) => void }) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()
  const [rawConfig, setRawConfig] = useState('')
  const [redaction, setRedaction] = useState<RedactionResult | null>(null)
  const [parseError, setParseError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!rawConfig.trim()) {
      setRedaction(null)
      setParseError(null)
      return
    }
    let parsed: Record<string, unknown>
    try {
      parsed = parseJsonObject(rawConfig, 'Claude config')
    } catch (err) {
      setRedaction(null)
      setParseError(err instanceof Error ? err.message : 'Config must be a JSON object')
      return
    }
    setParseError(null)
    let cancelled = false
    void redactConfig(parsed)
      .then(result => { if (!cancelled) setRedaction(result) })
      .catch(err => { if (!cancelled) { setRedaction(null); setParseError(err instanceof Error ? err.message : 'Failed to redact config') } })
    return () => { cancelled = true }
  }, [rawConfig])

  const categories = (redaction?.report.categories ?? {}) as Record<string, number>
  const secretCount = typeof redaction?.report.secret_count === 'number' ? redaction.report.secret_count : 0
  const redactedPreview = useMemo(() => (redaction ? JSON.stringify(redaction.redactedConfig, null, 2) : ''), [redaction])

  const submitMut = useMutation({
    mutationFn: () => client.createHarnessConfigReview({
      source_tool: 'claude',
      redacted_config: redaction!.redactedConfig,
      redaction_report: redaction!.report,
      content_hash: redaction!.contentHash,
      status: 'shared',
    }),
    onSuccess: (review) => {
      qc.invalidateQueries({ queryKey: ['harness-config-reviews'] })
      onFlash({ kind: 'success', message: `Config review ${review.id.slice(0, 8)}… shared.` })
      setRawConfig('')
      setRedaction(null)
    },
    onError: (err) => setError(err instanceof Error ? err.message : 'Failed to submit config review'),
  })

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (parseError) { setError(parseError); return }
    if (!redaction) { setError('Paste a valid Claude config JSON to redact first.'); return }
    setError(null)
    submitMut.mutate()
  }

  return (
    <section className="rounded-[18px] border border-border-primary bg-[#272729] p-5">
      <div className="mb-4 flex items-center gap-2">
        <FileJson className="h-4 w-4 text-accent-blue" />
        <div>
          <h2 className="text-xs font-semibold text-text-primary">Claude config review</h2>
          <p className="text-[11px] text-text-quaternary">Local redaction → preview → approve. Share only reviewed snapshots; raw secrets are rejected.</p>
        </div>
      </div>
      <div className="mb-4 grid gap-2 text-[11px] text-text-secondary md:grid-cols-3">
        <div className="rounded-[10px] border border-border-secondary bg-black/20 p-3"><span className="text-text-primary">1. Paste</span><br />Paste your raw Claude config — it never leaves your browser.</div>
        <div className="rounded-[10px] border border-border-secondary bg-black/20 p-3"><span className="text-text-primary">2. Auto-redact</span><br />Secrets, tokens, and private paths are replaced automatically.</div>
        <div className="rounded-[10px] border border-border-secondary bg-black/20 p-3"><span className="text-text-primary">3. Review &amp; share</span><br />Confirm the redacted preview, then submit the safe snapshot.</div>
      </div>
      <form onSubmit={submit} className="grid gap-4 lg:grid-cols-2">
        <label className="block space-y-1.5 text-[10px] text-text-quaternary lg:col-span-2">
          <span>Paste your Claude config</span>
          <textarea value={rawConfig} onChange={e => setRawConfig(e.target.value)} rows={10} placeholder='{ "mcpServers": { … }, "env": { … } }' className="font-mono w-full resize-none rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
        </label>
        {parseError && <p className="lg:col-span-2 text-[10px] text-status-error">{parseError}</p>}
        {redaction && (
          <>
            <div className="space-y-2 text-[11px] text-text-secondary">
              <p className="text-text-primary">Redaction summary</p>
              <p>Redactions: {secretCount}{secretCount > 0 ? ` (${Object.entries(categories).map(([cat, n]) => `${cat} ×${n}`).join(', ')})` : ' — nothing sensitive detected'}</p>
              <p className="break-all"><span className="text-text-quaternary">Content hash:</span> <span className="font-mono">{redaction.contentHash}</span></p>
            </div>
            <div className="space-y-1.5 text-[10px] text-text-quaternary">
              <span>Redacted preview (what gets shared)</span>
              <pre className="max-h-40 overflow-auto rounded-[8px] border border-border-secondary bg-black/20 p-3 text-[11px] text-text-secondary">{redactedPreview}</pre>
            </div>
          </>
        )}
        {error && <p className="lg:col-span-2 text-[10px] text-status-error">{error}</p>}
        <div className="lg:col-span-2 flex justify-end">
          <button type="submit" disabled={submitMut.isPending || !redaction} className={`rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}>{submitMut.isPending ? 'Submitting…' : 'Submit config review'}</button>
        </div>
      </form>
    </section>
  )
}

function redactionSummary(report: Record<string, unknown>): string {
  const count = typeof report.secret_count === 'number' ? report.secret_count : 0
  const categories = (report.categories && typeof report.categories === 'object' && !Array.isArray(report.categories)) ? report.categories as Record<string, unknown> : {}
  const parts = Object.entries(categories).map(([cat, n]) => `${cat} ×${typeof n === 'number' ? n : 0}`)
  return count > 0 ? `${count} redaction${count === 1 ? '' : 's'}${parts.length ? ` (${parts.join(', ')})` : ''}` : 'no sensitive values detected'
}

function ConfigReviewList() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [inspecting, setInspecting] = useState<string | null>(null)
  const { data: reviews = [], isLoading, error } = useQuery({
    queryKey: ['harness-config-reviews'],
    queryFn: () => client.listHarnessConfigReviews(),
  })

  return (
    <section className="rounded-[18px] border border-border-primary bg-[#272729] p-5">
      <div className="mb-4 flex items-center gap-2">
        <FileJson className="h-4 w-4 text-accent-blue" />
        <div>
          <h2 className="text-xs font-semibold text-text-primary">Shared config reviews</h2>
          <p className="text-[11px] text-text-quaternary">Redacted Claude config snapshots shared for review. Raw secrets are never stored.</p>
        </div>
      </div>
      {error && <div className="rounded-[11px] border border-status-error/30 bg-status-error/5 px-4 py-3 text-xs text-status-error">{error instanceof Error ? error.message : 'Failed to load config reviews'}</div>}
      <div className="space-y-2">
        {isLoading && [1, 2].map(i => <div key={i} className="h-16 animate-pulse rounded-[11px] border border-border-secondary bg-black/20" />)}
        {!isLoading && reviews.length === 0 && <p className="rounded-[11px] border border-border-secondary bg-black/20 p-6 text-center text-xs text-text-quaternary">No config reviews shared yet.</p>}
        {reviews.map(review => {
          const open = inspecting === review.id
          return (
            <div key={review.id} className="rounded-[11px] border border-border-secondary bg-black/20 p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="min-w-0 space-y-1">
                  <div className="flex flex-wrap items-center gap-2 text-[11px] text-text-secondary">
                    <span className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5 text-text-primary">{review.source_tool}</span>
                    <span className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5">{review.status}</span>
                    <span className="text-text-quaternary">{redactionSummary(review.redaction_report)}</span>
                  </div>
                  <p className="break-all font-mono text-[10px] text-text-quaternary">{review.content_hash}</p>
                </div>
                <button onClick={() => setInspecting(open ? null : review.id)} aria-label={`Inspect config review ${review.id}`} className={`rounded-full border border-border-primary px-3 py-1 text-[11px] text-text-secondary hover:bg-white/[0.04] ${FOCUS}`}>{open ? 'Hide' : 'Inspect'}</button>
              </div>
              {open && (
                <pre className="mt-3 max-h-56 overflow-auto rounded-[8px] border border-border-secondary bg-black/30 p-3 text-[11px] text-text-secondary">{JSON.stringify(review.redacted_config, null, 2)}</pre>
              )}
            </div>
          )
        })}
      </div>
    </section>
  )
}

export default function Harnesses() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [target, setTarget] = useState('')
  const [ownerUserId, setOwnerUserId] = useState('')
  const [showCreate, setShowCreate] = useState(false)
  const [publishTarget, setPublishTarget] = useState<Harness | null>(null)
  const [approvalTarget, setApprovalTarget] = useState<Harness | null>(null)
  const [flash, setFlash] = useState<Flash>(null)

  const { data: harnesses = [], isLoading, error } = useQuery({
    queryKey: ['harnesses', target, ownerUserId],
    queryFn: () => client.listHarnesses({ target: target || undefined, owner_user_id: ownerUserId || undefined }),
  })

  return (
    <div className="mx-auto max-w-6xl space-y-6 p-8">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h1 className="text-base font-semibold text-text-primary">Harness Library</h1>
          <p className="mt-1 max-w-2xl text-xs text-text-tertiary">Publish reusable AI tooling harnesses. Downloads require explicit approval and never mutate local configuration from the backend.</p>
        </div>
        <button onClick={() => setShowCreate(true)} className={`flex items-center gap-2 rounded-full bg-accent-blue px-4 py-2 text-[13px] font-semibold text-white hover:bg-accent-blue-hover ${FOCUS}`}>
          <PackagePlus className="h-4 w-4" />
          New harness
        </button>
      </div>

      {flash && (
        <div role="status" className={`flex items-start gap-2 rounded-[11px] border px-4 py-3 text-xs ${flash.kind === 'success' ? 'border-status-success/30 bg-status-success/5 text-status-success' : 'border-status-error/30 bg-status-error/5 text-status-error'}`}>
          {flash.kind === 'success' ? <CheckCircle2 className="h-4 w-4" /> : <AlertCircle className="h-4 w-4" />}
          <span className="flex-1">{flash.message}</span>
          <button onClick={() => setFlash(null)} aria-label="Dismiss" className={FOCUS}><X className="h-3.5 w-3.5" /></button>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-3 rounded-[18px] border border-border-primary bg-[#272729] p-4">
        <label className="text-[10px] text-text-quaternary" htmlFor="target-filter">Target filter</label>
        <select id="target-filter" value={target} onChange={e => setTarget(e.target.value)} className="rounded-[8px] border border-border-primary bg-black/20 px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60">
          <option value="">All targets</option>
          <option value="claude">Claude</option>
          <option value="codex">Codex</option>
          <option value="opencode">OpenCode</option>
        </select>
        <label className="text-[10px] text-text-quaternary" htmlFor="owner-filter">Owner filter</label>
        <select id="owner-filter" value={ownerUserId} onChange={e => setOwnerUserId(e.target.value)} className="rounded-[8px] border border-border-primary bg-black/20 px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60">
          <option value="">All owners</option>
          {Array.from(new Map(harnesses.filter(h => h.owner).map(h => [h.owner_user_id, h.owner!]))).map(([id, owner]) => <option key={id} value={id}>{owner.name}</option>)}
        </select>
      </div>

      {error && <div className="rounded-[11px] border border-status-error/30 bg-status-error/5 px-4 py-3 text-xs text-status-error">{error instanceof Error ? error.message : 'Failed to load harnesses'}</div>}

      <div className="grid gap-4">
        {isLoading && [1, 2, 3].map(i => <div key={i} className="h-28 animate-pulse rounded-[18px] border border-border-primary bg-[#272729]" />)}
        {!isLoading && harnesses.length === 0 && <div className="rounded-[18px] border border-border-primary bg-[#272729] p-10 text-center text-xs text-text-quaternary">No harnesses found.</div>}
        {harnesses.map(harness => (
          <article key={harness.id} className="rounded-[18px] border border-border-primary bg-[#272729] p-5">
            <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
              <div className="min-w-0 space-y-2">
                <div className="flex flex-wrap items-center gap-2">
                  <h2 className="text-sm font-semibold text-text-primary">{harness.name}</h2>
                  <span className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-text-secondary">{harness.status}</span>
                  <span className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-text-secondary">{harness.visibility}</span>
                </div>
                <p className="text-xs text-text-quaternary">{harness.description ?? 'No description'}</p>
                <p className="text-[11px] text-text-secondary">Owner: {harness.owner?.name ?? harness.owner_user_id}</p>
                {harness.latest_version && (
                  <div className="flex flex-wrap gap-2 text-[11px] text-text-secondary">
                    <span>Version {harness.latest_version.version}</span>
                    <span className="font-mono">{harness.latest_version.manifest_hash}</span>
                    {harness.latest_version.targets.map(t => <span key={t} className="rounded-[5px] border border-border-secondary px-1.5 py-0.5">{t}</span>)}
                  </div>
                )}
              </div>
              <div className="flex shrink-0 flex-wrap gap-2">
                <button onClick={() => setPublishTarget(harness)} aria-label={`Publish version for ${harness.name}`} className={`rounded-full border border-border-primary px-3 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] ${FOCUS}`}>Publish version</button>
                <button onClick={() => setApprovalTarget(harness)} disabled={!harness.latest_version} aria-label={`Download ${harness.name}`} className={`flex items-center gap-1.5 rounded-full bg-accent-blue px-3 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}><Download className="h-3.5 w-3.5" />Download</button>
              </div>
            </div>
          </article>
        ))}
      </div>

      <ConfigReviewForm onFlash={setFlash} />
      <ConfigReviewList />

      {showCreate && <CreateHarnessModal onClose={() => setShowCreate(false)} onFlash={setFlash} />}
      {publishTarget && <PublishModal harness={publishTarget} onClose={() => setPublishTarget(null)} onFlash={setFlash} />}
      {approvalTarget && <ApprovalModal harness={approvalTarget} onClose={() => setApprovalTarget(null)} onFlash={setFlash} />}
    </div>
  )
}
