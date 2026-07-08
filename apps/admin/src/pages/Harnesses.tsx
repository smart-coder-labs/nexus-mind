import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertCircle, CheckCircle2, Download, FileJson, PackagePlus, ShieldCheck, X } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import type { Harness } from '../types'

const FOCUS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'
const DEFAULT_MANIFEST = JSON.stringify({
  schema_version: '1.0',
  targets: ['claude'],
  components: [],
  compatibility: {},
  provenance: { source: 'admin-ui' },
  security: { requires_approval: true },
}, null, 2)

type Flash = { kind: 'success' | 'error'; message: string } | null

function parseJsonObject(value: string, label: string): Record<string, unknown> {
  const parsed = JSON.parse(value)
  if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
    throw new Error(`${label} must be a JSON object`)
  }
  return parsed as Record<string, unknown>
}

function CreateHarnessModal({ onClose }: { onClose: () => void }) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()
  const [name, setName] = useState('')
  const [slug, setSlug] = useState('')
  const [description, setDescription] = useState('')
  const [error, setError] = useState<string | null>(null)

  const createMut = useMutation({
    mutationFn: () => client.createHarness({
      name: name.trim(),
      slug: slug.trim(),
      description: description.trim() || undefined,
      visibility: 'org',
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['harnesses'] })
      onClose()
    },
    onError: (err) => setError(err instanceof Error ? err.message : 'Failed to create harness'),
  })

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!name.trim() || !slug.trim()) {
      setError('Name and slug are required.')
      return
    }
    setError(null)
    createMut.mutate()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={onClose}>
      <div role="dialog" aria-modal="true" aria-label="Create harness" className="w-full max-w-md rounded-[18px] border border-border-primary bg-[#1d1d1f] p-6" onClick={e => e.stopPropagation()}>
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
            <textarea value={description} onChange={e => setDescription(e.target.value)} rows={3} className="w-full resize-none rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
          </label>
          {error && <p className="text-[10px] text-status-error">{error}</p>}
          <div className="flex justify-end gap-2">
            <button type="button" onClick={onClose} className={`rounded-full border border-border-primary px-4 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] ${FOCUS}`}>Cancel</button>
            <button type="submit" disabled={createMut.isPending} className={`rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}>{createMut.isPending ? 'Creating…' : 'Create'}</button>
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
  const [manifestJson, setManifestJson] = useState(DEFAULT_MANIFEST)
  const [error, setError] = useState<string | null>(null)

  const publishMut = useMutation({
    mutationFn: () => client.publishHarnessVersion(harness.id, { version: version.trim(), manifest: parseJsonObject(manifestJson, 'Manifest JSON') }),
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
    try { parseJsonObject(manifestJson, 'Manifest JSON') } catch (err) { setError(err instanceof Error ? err.message : 'Invalid manifest JSON'); return }
    setError(null)
    publishMut.mutate()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" onClick={onClose}>
      <div role="dialog" aria-modal="true" aria-label="Publish harness version" className="w-full max-w-2xl rounded-[18px] border border-border-primary bg-[#1d1d1f] p-6" onClick={e => e.stopPropagation()}>
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-xs font-semibold text-text-primary">Publish harness version</h2>
          <button onClick={onClose} aria-label="Close" className={`rounded-[6px] text-text-tertiary hover:text-text-primary ${FOCUS}`}><X className="h-4 w-4" /></button>
        </div>
        <form onSubmit={submit} className="space-y-4">
          <label className="block space-y-1.5 text-[10px] text-text-quaternary">
            <span>Version</span>
            <input value={version} onChange={e => setVersion(e.target.value)} placeholder="1.0.0" className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
          </label>
          <label className="block space-y-1.5 text-[10px] text-text-quaternary">
            <span>Manifest JSON</span>
            <textarea value={manifestJson} onChange={e => setManifestJson(e.target.value)} rows={12} className="font-mono w-full resize-none rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
          </label>
          {error && <p className="text-[10px] text-status-error">{error}</p>}
          <div className="flex justify-end gap-2">
            <button type="button" onClick={onClose} className={`rounded-full border border-border-primary px-4 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] ${FOCUS}`}>Cancel</button>
            <button type="submit" disabled={publishMut.isPending} className={`rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}>{publishMut.isPending ? 'Publishing…' : 'Publish'}</button>
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
  const approveMut = useMutation({
    mutationFn: async () => {
      if (!latest) throw new Error('No published version is available.')
      await client.approveHarnessInstall(harness.id, latest.version, {
        target_tool: latest.targets[0] ?? 'claude',
        target_scope: 'project',
        manifest_hash: latest.manifest_hash,
        metadata: { source: 'admin-ui' },
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
          <p><span className="text-text-quaternary">Manifest hash:</span> <span className="font-mono text-text-primary">{latest?.manifest_hash ?? 'No published version'}</span></p>
        </div>
        <div className="mt-6 flex justify-end gap-2">
          <button onClick={onClose} className={`rounded-full border border-border-primary px-4 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] ${FOCUS}`}>Cancel</button>
          <button onClick={() => approveMut.mutate()} disabled={approveMut.isPending || !latest} className={`rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}>{approveMut.isPending ? 'Approving…' : 'Approve and download'}</button>
        </div>
      </div>
    </div>
  )
}

function ConfigReviewForm({ onFlash }: { onFlash: (flash: Flash) => void }) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [redactedConfigJson, setRedactedConfigJson] = useState('')
  const [redactionReportJson, setRedactionReportJson] = useState('')
  const [contentHash, setContentHash] = useState('')
  const [error, setError] = useState<string | null>(null)

  const reportPreview = useMemo(() => {
    if (!redactionReportJson.trim()) return null
    try { return JSON.stringify(parseJsonObject(redactionReportJson, 'Redaction report JSON'), null, 2) } catch { return null }
  }, [redactionReportJson])

  const submitMut = useMutation({
    mutationFn: () => client.createHarnessConfigReview({
      source_tool: 'claude',
      redacted_config: parseJsonObject(redactedConfigJson, 'Redacted config JSON'),
      redaction_report: parseJsonObject(redactionReportJson, 'Redaction report JSON'),
      content_hash: contentHash.trim(),
      status: 'shared',
    }),
    onSuccess: (review) => onFlash({ kind: 'success', message: `Config review ${review.id.slice(0, 8)}… shared.` }),
    onError: (err) => setError(err instanceof Error ? err.message : 'Failed to submit config review'),
  })

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    try {
      parseJsonObject(redactedConfigJson, 'Redacted config JSON')
      parseJsonObject(redactionReportJson, 'Redaction report JSON')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Invalid JSON')
      return
    }
    if (!contentHash.trim()) { setError('Content hash is required.'); return }
    setError(null)
    submitMut.mutate()
  }

  return (
    <section className="rounded-[18px] border border-border-primary bg-[#272729] p-5">
      <div className="mb-4 flex items-center gap-2">
        <FileJson className="h-4 w-4 text-accent-blue" />
        <div>
          <h2 className="text-xs font-semibold text-text-primary">Claude config review</h2>
          <p className="text-[11px] text-text-quaternary">Paste only locally redacted snapshots. Raw secrets are rejected.</p>
        </div>
      </div>
      <form onSubmit={submit} className="grid gap-4 lg:grid-cols-2">
        <label className="block space-y-1.5 text-[10px] text-text-quaternary">
          <span>Redacted config JSON</span>
          <textarea value={redactedConfigJson} onChange={e => setRedactedConfigJson(e.target.value)} rows={8} className="font-mono w-full resize-none rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
        </label>
        <label className="block space-y-1.5 text-[10px] text-text-quaternary">
          <span>Redaction report JSON</span>
          <textarea value={redactionReportJson} onChange={e => setRedactionReportJson(e.target.value)} rows={8} className="font-mono w-full resize-none rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
        </label>
        <label className="block space-y-1.5 text-[10px] text-text-quaternary lg:col-span-2">
          <span>Content hash</span>
          <input value={contentHash} onChange={e => setContentHash(e.target.value)} placeholder="sha256:…" className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60" />
        </label>
        {reportPreview && <pre className="lg:col-span-2 max-h-40 overflow-auto rounded-[8px] border border-border-secondary bg-black/20 p-3 text-[11px] text-text-secondary">{reportPreview}</pre>}
        {error && <p className="lg:col-span-2 text-[10px] text-status-error">{error}</p>}
        <div className="lg:col-span-2 flex justify-end">
          <button type="submit" disabled={submitMut.isPending} className={`rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:bg-accent-blue-hover disabled:opacity-50 ${FOCUS}`}>{submitMut.isPending ? 'Submitting…' : 'Submit config review'}</button>
        </div>
      </form>
    </section>
  )
}

export default function Harnesses() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [target, setTarget] = useState('')
  const [showCreate, setShowCreate] = useState(false)
  const [publishTarget, setPublishTarget] = useState<Harness | null>(null)
  const [approvalTarget, setApprovalTarget] = useState<Harness | null>(null)
  const [flash, setFlash] = useState<Flash>(null)

  const { data: harnesses = [], isLoading, error } = useQuery({
    queryKey: ['harnesses', target],
    queryFn: () => client.listHarnesses({ target: target || undefined }),
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

      <div className="flex items-center gap-3 rounded-[18px] border border-border-primary bg-[#272729] p-4">
        <label className="text-[10px] text-text-quaternary" htmlFor="target-filter">Target filter</label>
        <select id="target-filter" value={target} onChange={e => setTarget(e.target.value)} className="rounded-[8px] border border-border-primary bg-black/20 px-3 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60">
          <option value="">All targets</option>
          <option value="claude">Claude</option>
          <option value="codex">Codex</option>
          <option value="opencode">OpenCode</option>
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

      {showCreate && <CreateHarnessModal onClose={() => setShowCreate(false)} />}
      {publishTarget && <PublishModal harness={publishTarget} onClose={() => setPublishTarget(null)} onFlash={setFlash} />}
      {approvalTarget && <ApprovalModal harness={approvalTarget} onClose={() => setApprovalTarget(null)} onFlash={setFlash} />}
    </div>
  )
}
