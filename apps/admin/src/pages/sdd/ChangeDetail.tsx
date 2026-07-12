import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { X, Link2 } from 'lucide-react'
import { Link } from 'react-router-dom'
import { createClient } from '../../api/client'
import { useAuth, isPrivileged } from '../../auth/AuthContext'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../../components/ui/Select/Select'
import { Badge } from '../../components/ui/Badge/Badge'
import { Markdown } from '../../components/ui/Markdown'
import { STATUS_BADGE_VARIANT } from '../Tasks'
import { SDD_PHASE_OPTIONS, SDD_STATUS_OPTIONS } from '../Sdd'
import type {
  PatchSddChangeRequest, SddArtifact, SddArtifactKind, SddPhase, SddStatus,
} from '../../types'

const client = createClient()

interface ChangeDetailProps {
  changeId: string
  onClose: () => void
}

/**
 * The tab strip. One tab per artifact kind the change actually HAS — the
 * inventory drives this, never a static array. `spec` is special: a change holds
 * one spec per capability, so the single Specs tab carries a capability sub-list.
 */
const TAB_KINDS: { kind: SddArtifactKind; label: string }[] = [
  { kind: 'exploration',    label: 'Exploration' },
  { kind: 'proposal',       label: 'Proposal' },
  { kind: 'spec',           label: 'Specs' },
  { kind: 'design',         label: 'Design' },
  { kind: 'tasks',          label: 'Tasks' },
  { kind: 'apply-progress', label: 'Apply' },
  { kind: 'verify-report',  label: 'Verify' },
  { kind: 'archive-report', label: 'Archive' },
  { kind: 'state',          label: 'State' },
]

const FOCUS =
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

function revisionLabel(rev: { revision: number; source: string; created_at: string }): string {
  return `rev ${rev.revision} · ${rev.source} · ${new Date(rev.created_at).toLocaleDateString()}`
}

export default function ChangeDetail({ changeId, onClose }: ChangeDetailProps) {
  const { session } = useAuth()
  const qc = useQueryClient()
  const isAdmin = isPrivileged(session?.user.role)
  const permissions = session?.user.permissions ?? []
  // A7: curation of change metadata and memory links is permitted — it is not
  // artifact authorship. Artifact CONTENT is read-only for everyone, always.
  const canWrite = isAdmin || permissions.includes('sdd:write')

  const [activeKind, setActiveKind] = useState<SddArtifactKind | null>(null)
  const [activeCapability, setActiveCapability] = useState<string | null>(null)
  const [viewMode, setViewMode] = useState<'raw' | 'preview'>('preview')
  const [selectedRevision, setSelectedRevision] = useState<number | null>(null)
  const [memoryToLink, setMemoryToLink] = useState('')

  const { data: change } = useQuery({
    queryKey: ['sdd-change', changeId],
    queryFn: () => client.getSddChange(changeId),
  })

  const { data: linkedTasks = [] } = useQuery({
    queryKey: ['sdd-change-tasks', changeId],
    queryFn: () => client.getSddChangeTasks(changeId),
  })

  const { data: memories = [] } = useQuery({
    queryKey: ['memories', { limit: 50 }],
    queryFn: () => client.listMemories({ limit: 50 }),
    enabled: canWrite,
  })

  const { data: sprints = [] } = useQuery({
    queryKey: ['sprints', change?.project],
    queryFn: () => client.listSprints({ project: change!.project }),
    enabled: canWrite && !!change,
  })

  const artifacts: SddArtifact[] = useMemo(() => change?.artifacts ?? [], [change])

  const tabs = useMemo(
    () => TAB_KINDS.filter(t => artifacts.some(a => a.kind === t.kind)),
    [artifacts],
  )

  // The default tab is the first kind the change actually has, in pipeline order.
  const currentKind: SddArtifactKind | null =
    (activeKind && tabs.some(t => t.kind === activeKind) ? activeKind : null) ?? tabs[0]?.kind ?? null

  const capabilities = useMemo(
    () => artifacts.filter(a => a.kind === 'spec').map(a => a.capability).sort(),
    [artifacts],
  )

  const currentCapability =
    activeCapability && capabilities.includes(activeCapability) ? activeCapability : capabilities[0]

  const currentArtifact: SddArtifact | undefined =
    currentKind === 'spec'
      ? artifacts.find(a => a.kind === 'spec' && a.capability === currentCapability)
      : artifacts.find(a => a.kind === currentKind)

  const artifactId = currentArtifact?.id

  const { data: artifactDetail } = useQuery({
    queryKey: ['sdd-artifact', artifactId],
    queryFn: () => client.getSddArtifact(artifactId!),
    enabled: !!artifactId,
  })

  const { data: revisions = [] } = useQuery({
    queryKey: ['sdd-artifact-revisions', artifactId],
    queryFn: () => client.listSddArtifactRevisions(artifactId!),
    enabled: !!artifactId,
  })

  // Only fetched when the user asks for a revision other than the latest — the
  // latest already arrived inline with the artifact detail read.
  const wantsOlderRevision =
    selectedRevision != null && selectedRevision !== currentArtifact?.latest_revision

  const { data: revisionDetail } = useQuery({
    queryKey: ['sdd-artifact-revision', artifactId, selectedRevision],
    queryFn: () => client.getSddArtifactRevision(artifactId!, selectedRevision!),
    enabled: !!artifactId && wantsOlderRevision,
  })

  const content = wantsOlderRevision
    ? revisionDetail?.content ?? ''
    : artifactDetail?.content ?? ''

  const patchMut = useMutation({
    mutationFn: (input: PatchSddChangeRequest) => client.patchSddChange(changeId, input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sdd-change', changeId] })
      qc.invalidateQueries({ queryKey: ['sdd-changes'] })
    },
  })

  const linkMemoryMut = useMutation({
    mutationFn: (memoryId: string) => client.linkSddChangeMemory(changeId, { memory_id: memoryId }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sdd-change', changeId] })
      setMemoryToLink('')
    },
  })

  const unlinkMemoryMut = useMutation({
    mutationFn: (memoryId: string) => client.unlinkSddChangeMemory(changeId, memoryId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['sdd-change', changeId] }),
  })

  const selectTab = (kind: SddArtifactKind) => {
    setActiveKind(kind)
    setActiveCapability(null)
    setSelectedRevision(null)
  }

  const selectCapability = (capability: string) => {
    setActiveCapability(capability)
    setSelectedRevision(null)
  }

  const memoryLinks = change?.memory_links ?? []
  const linkableMemories = memories.filter(m => !memoryLinks.some(l => l.id === m.id))

  return (
    <div className="relative w-full">
      <button
        onClick={onClose}
        aria-label="Close"
        className={`absolute top-0 right-0 w-8 h-8 flex items-center justify-center rounded-full bg-background-tertiary text-text-secondary hover:text-text-primary transition-colors ${FOCUS}`}
      >
        <X className="w-3.5 h-3.5" />
      </button>

      {/* Header */}
      <header className="mb-5 pr-10">
        <h2 className="text-sm font-semibold text-text-primary">{change?.name ?? '…'}</h2>
        {change?.title && (
          <p className="text-xs text-text-tertiary mt-0.5">{change.title}</p>
        )}
        {change && (
          <div className="flex items-center gap-2 mt-2">
            <Badge variant="default" size="sm">{change.project}</Badge>
            <Badge variant="primary" size="sm">{change.phase}</Badge>
            <Badge variant="default" size="sm">{change.status}</Badge>
          </div>
        )}
      </header>

      {/* Curation (A7 — change metadata, never artifact content) */}
      {canWrite && change && (
        <section className="mb-6 flex items-center gap-2 flex-wrap">
          <Select
            value={change.phase}
            onValueChange={v => patchMut.mutate({ phase: v as SddPhase })}
          >
            <SelectTrigger className="w-36 h-8 text-xs" aria-label="Phase">
              <SelectValue placeholder="Phase" />
            </SelectTrigger>
            <SelectContent>
              {SDD_PHASE_OPTIONS.map(p => (
                <SelectItem key={p} value={p}>{p}</SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select
            value={change.status}
            onValueChange={v => patchMut.mutate({ status: v as SddStatus })}
          >
            <SelectTrigger className="w-36 h-8 text-xs" aria-label="Status">
              <SelectValue placeholder="Status" />
            </SelectTrigger>
            <SelectContent>
              {SDD_STATUS_OPTIONS.map(s => (
                <SelectItem key={s} value={s}>{s}</SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select
            value={change.sprint_id ?? ''}
            onValueChange={v => patchMut.mutate({ sprint_id: v || null })}
          >
            <SelectTrigger className="w-40 h-8 text-xs" aria-label="Sprint">
              <SelectValue placeholder="No sprint" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="">No sprint</SelectItem>
              {sprints.map(s => (
                <SelectItem key={s.id} value={s.id}>{s.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </section>
      )}

      {/* Artifact tabs — one per kind that EXISTS on the change */}
      {tabs.length > 0 && (
        <div role="tablist" aria-label="Artifacts" className="flex items-center gap-1 flex-wrap border-b border-border-secondary mb-3">
          {tabs.map(({ kind, label }) => (
            <button
              key={kind}
              role="tab"
              aria-selected={kind === currentKind}
              onClick={() => selectTab(kind)}
              className={`px-3 py-1.5 text-xs transition-colors ${FOCUS} ${
                kind === currentKind
                  ? 'text-text-primary font-semibold border-b-2 border-accent-blue'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      )}

      {/* Specs: one entry per capability */}
      {currentKind === 'spec' && capabilities.length > 0 && (
        <div data-testid="spec-capabilities" className="flex items-center gap-1.5 flex-wrap mb-3">
          {capabilities.map(cap => (
            <button
              key={cap}
              onClick={() => selectCapability(cap)}
              aria-pressed={cap === currentCapability}
              className={`rounded-full px-2.5 py-1 text-[11px] border transition-colors ${FOCUS} ${
                cap === currentCapability
                  ? 'bg-accent-blue/10 text-accent-blue border-accent-blue/20 font-semibold'
                  : 'border-border-primary text-text-quaternary hover:text-text-secondary'
              }`}
            >
              {cap}
            </button>
          ))}
        </div>
      )}

      {/* Toolbar: Raw/Preview + revision selector. Deliberately OUTSIDE the
          artifact panel — the panel holds rendered content and nothing editable. */}
      {currentArtifact && (
        <div className="flex items-center justify-between gap-3 mb-2">
          <div className="bg-white/[0.04] rounded-full p-0.5 flex items-center w-fit">
            <button
              onClick={() => setViewMode('raw')}
              className={`text-[10px] px-2 py-0.5 rounded-full transition-colors ${FOCUS} ${
                viewMode === 'raw'
                  ? 'bg-background-tertiary text-text-primary font-semibold'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
            >
              Raw
            </button>
            <button
              onClick={() => setViewMode('preview')}
              className={`text-[10px] px-2 py-0.5 rounded-full transition-colors ${FOCUS} ${
                viewMode === 'preview'
                  ? 'bg-background-tertiary text-text-primary font-semibold'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
            >
              Preview
            </button>
          </div>

          {revisions.length > 0 && (
            <Select
              value={String(selectedRevision ?? currentArtifact.latest_revision)}
              onValueChange={v => setSelectedRevision(Number(v))}
            >
              <SelectTrigger className="w-56 h-8 text-xs" aria-label="Revision">
                <SelectValue placeholder={`rev ${currentArtifact.latest_revision}`} />
              </SelectTrigger>
              <SelectContent>
                {revisions.map(r => (
                  <SelectItem key={r.id} value={String(r.revision)}>
                    {revisionLabel(r)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        </div>
      )}

      {/* Artifact content — READ-ONLY (A7). No editor, no save, no delete. */}
      <section
        data-testid="artifact-panel"
        role="tabpanel"
        className="mb-6 rounded-[11px] border border-border-secondary p-4 max-h-[50vh] overflow-y-auto"
      >
        {!currentArtifact ? (
          <p className="text-xs text-text-quaternary">This change has no artifacts yet.</p>
        ) : viewMode === 'raw' ? (
          <pre
            data-testid="artifact-raw"
            className="text-xs text-text-secondary whitespace-pre-wrap font-mono"
          >{content}</pre>
        ) : (
          <div className="text-xs text-text-secondary">
            <Markdown content={content} />
          </div>
        )}
      </section>

      {/* Linked tasks */}
      <section data-testid="linked-tasks" className="mb-6">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Linked Tasks</h3>
        {linkedTasks.length === 0 ? (
          <p className="text-xs text-text-quaternary">No linked tasks</p>
        ) : (
          <ul className="space-y-1.5">
            {linkedTasks.map(t => (
              <li
                key={t.id}
                className="flex items-center justify-between gap-2 rounded-[11px] border border-border-secondary px-3 py-2"
              >
                <Link
                  to={`/tasks?task=${encodeURIComponent(t.id)}`}
                  className={`text-xs text-text-primary hover:text-accent-blue transition-colors ${FOCUS}`}
                >
                  {t.title}
                </Link>
                <Badge variant={STATUS_BADGE_VARIANT[t.status] ?? 'default'} size="sm">{t.status}</Badge>
              </li>
            ))}
          </ul>
        )}
      </section>

      {/* Linked memories */}
      <section data-testid="linked-memories">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Linked Memories</h3>
        {memoryLinks.length === 0 ? (
          <p className="text-xs text-text-quaternary mb-2">No linked memories</p>
        ) : (
          <ul className="space-y-1.5 mb-2">
            {memoryLinks.map(m => {
              const label = m.title ?? m.content.slice(0, 60)
              return (
                <li
                  key={m.id}
                  className="flex items-center justify-between gap-2 rounded-[11px] border border-border-secondary px-3 py-2"
                >
                  <Link
                    to={`/memories?id=${encodeURIComponent(m.id)}`}
                    className={`text-xs text-text-primary hover:text-accent-blue transition-colors ${FOCUS}`}
                  >
                    {label}
                  </Link>
                  <div className="flex items-center gap-2 shrink-0">
                    {m.type && <Badge variant="default" size="sm">{m.type}</Badge>}
                    {canWrite && (
                      <button
                        onClick={() => unlinkMemoryMut.mutate(m.id)}
                        aria-label={`Unlink ${label}`}
                        title="Unlink"
                        className={`text-text-quaternary hover:text-status-error transition-colors ${FOCUS}`}
                      >
                        <X className="w-3 h-3" />
                      </button>
                    )}
                  </div>
                </li>
              )
            })}
          </ul>
        )}

        {canWrite && (
          <div className="flex items-center gap-2">
            <Select value={memoryToLink} onValueChange={setMemoryToLink}>
              <SelectTrigger className="w-64 h-8 text-xs" aria-label="Link memory">
                <SelectValue placeholder="Link a memory…" />
              </SelectTrigger>
              <SelectContent>
                {linkableMemories.map(m => (
                  <SelectItem key={m.id} value={m.id}>
                    {m.title ?? m.content.slice(0, 60)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <button
              type="button"
              onClick={() => memoryToLink && linkMemoryMut.mutate(memoryToLink)}
              disabled={!memoryToLink || linkMemoryMut.isPending}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50 ${FOCUS}`}
            >
              <Link2 className="w-3 h-3" />
              Link
            </button>
          </div>
        )}
      </section>
    </div>
  )
}
