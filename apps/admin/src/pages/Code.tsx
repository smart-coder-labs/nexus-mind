import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Loader2 } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { CodeProject } from '../types'

const INPUT_CLS =
  'w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-border-focus transition-colors'

function StatusChip({ project }: { project: CodeProject }) {
  const indexed = project.last_indexed != null
  if (indexed) {
    return (
      <span className="text-[10px] font-semibold border rounded-full px-2 py-0.5 text-status-success bg-status-success/10 border-status-success/20">
        indexed
      </span>
    )
  }
  return (
    <span className="text-[10px] font-semibold border rounded-full px-2 py-0.5 text-text-quaternary bg-surface-secondary border-border-secondary">
      not indexed
    </span>
  )
}

function SkeletonRow() {
  return (
    <div className="border border-border-primary rounded-[18px] p-5 animate-pulse">
      <div className="h-4 bg-surface-secondary rounded w-1/3 mb-2" />
      <div className="h-3 bg-surface-secondary rounded w-1/2 mb-2" />
      <div className="h-3 bg-surface-secondary rounded w-2/3" />
    </div>
  )
}

export default function Code() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [showForm, setShowForm] = useState(false)
  const [projectName, setProjectName] = useState('')
  const [rootPath, setRootPath] = useState('')
  const [indexError, setIndexError] = useState<string | null>(null)

  const { data: projects, isLoading } = useQuery({
    queryKey: ['code-projects'],
    queryFn: () => client.listCodeProjects(),
  })

  const indexMut = useMutation({
    mutationFn: (data: { project: string; root_path: string }) => client.indexProject(data),
    onSuccess: () => {
      setProjectName('')
      setRootPath('')
      setShowForm(false)
      setIndexError(null)
      qc.invalidateQueries({ queryKey: ['code-projects'] })
    },
    onError: (err: Error) => setIndexError(err.message),
  })

  const reindexMut = useMutation({
    mutationFn: (p: CodeProject) => client.indexProject({ project: p.name, root_path: p.root_path }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  const deleteMut = useMutation({
    mutationFn: (name: string) => client.deleteCodeProject(name),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  const handleIndex = (e: React.FormEvent) => {
    e.preventDefault()
    setIndexError(null)
    indexMut.mutate({ project: projectName.trim(), root_path: rootPath.trim() })
  }

  const handleDelete = (p: CodeProject) => {
    if (!window.confirm(`Delete "${p.name}"? This removes all indexed chunks.`)) return
    deleteMut.mutate(p.name)
  }

  const formatDate = (iso: string) => {
    try {
      return new Date(iso).toLocaleString()
    } catch {
      return iso
    }
  }

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-8">
      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">
            Code Repositories
          </h1>
          <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">
            Connect and index codebases for AI-assisted search and context retrieval.
          </p>
        </div>
        {!showForm && (
          <button
            onClick={() => setShowForm(true)}
            className="shrink-0 bg-accent-blue text-white rounded-full px-4 py-1.5 text-sm font-normal hover:opacity-90 transition-opacity"
          >
            Add Repository
          </button>
        )}
      </div>

      {/* Add form */}
      {showForm && (
        <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
          <p className="text-[12px] tracking-[-0.12px] text-text-tertiary">Add Repository</p>
          <form onSubmit={handleIndex} className="space-y-3">
            <div>
              <label className="block text-[12px] tracking-[-0.12px] text-text-tertiary mb-1.5">
                Project name
              </label>
              <input
                className={INPUT_CLS}
                placeholder="nexus-mind"
                value={projectName}
                onChange={e => setProjectName(e.target.value)}
                disabled={indexMut.isPending}
                required
              />
            </div>
            <div>
              <label className="block text-[12px] tracking-[-0.12px] text-text-tertiary mb-1.5">
                Root path
              </label>
              <input
                className={INPUT_CLS}
                placeholder="/absolute/path/to/repo"
                value={rootPath}
                onChange={e => setRootPath(e.target.value)}
                disabled={indexMut.isPending}
                required
              />
            </div>
            {indexError && (
              <p className="text-xs text-status-error/80">{indexError}</p>
            )}
            <div className="flex gap-2 pt-1">
              <button
                type="button"
                onClick={() => { setShowForm(false); setIndexError(null) }}
                disabled={indexMut.isPending}
                className="rounded-full border border-border-primary px-4 py-1.5 text-sm text-text-secondary hover:text-text-primary transition-colors disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={indexMut.isPending}
                className="flex items-center gap-1.5 bg-accent-blue text-white rounded-full px-4 py-1.5 text-sm font-normal hover:opacity-90 transition-opacity disabled:opacity-60"
              >
                {indexMut.isPending && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                {indexMut.isPending ? 'Indexing…' : 'Index'}
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Projects list */}
      {isLoading ? (
        <div className="space-y-3">
          <SkeletonRow />
          <SkeletonRow />
          <SkeletonRow />
        </div>
      ) : !projects || projects.length === 0 ? (
        <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
          <p className="text-sm font-semibold text-text-primary">No repositories indexed yet.</p>
          <p className="text-[13px] text-text-tertiary">
            Add a repository to enable semantic code search and context retrieval.
          </p>
          {!showForm && (
            <button
              onClick={() => setShowForm(true)}
              className="mt-3 bg-accent-blue text-white rounded-full px-4 py-1.5 text-sm font-normal hover:opacity-90 transition-opacity"
            >
              Add Repository
            </button>
          )}
        </div>
      ) : (
        <div className="space-y-3">
          {projects.map(p => {
            const isReindexing = reindexMut.isPending && reindexMut.variables?.name === p.name
            return (
              <div
                key={p.id}
                className="group border border-border-primary rounded-[18px] p-5 flex items-start justify-between gap-4"
              >
                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="font-semibold text-text-primary text-sm">{p.name}</span>
                    <StatusChip project={p} />
                  </div>
                  <p className="text-xs text-text-tertiary font-mono truncate">{p.root_path}</p>
                  <p className="text-xs text-text-tertiary">
                    {p.file_count.toLocaleString()} files
                    {' · '}
                    {p.chunk_count.toLocaleString()} chunks
                    {p.last_indexed
                      ? ` · Last indexed: ${formatDate(p.last_indexed)}`
                      : ' · Never indexed'}
                  </p>
                </div>
                <div className="flex items-center gap-2 shrink-0 opacity-0 group-hover:opacity-100 sm:opacity-100 transition-opacity">
                  <button
                    onClick={() => reindexMut.mutate(p)}
                    disabled={isReindexing}
                    className="flex items-center gap-1 text-xs border border-border-primary rounded-full px-3 py-1 text-text-secondary hover:text-text-primary transition-colors disabled:opacity-50"
                  >
                    {isReindexing && <Loader2 className="w-3 h-3 animate-spin" />}
                    Re-index
                  </button>
                  <button
                    onClick={() => handleDelete(p)}
                    disabled={deleteMut.isPending}
                    className="text-xs border border-status-error/20 rounded-full px-3 py-1 text-status-error/60 hover:text-status-error transition-colors disabled:opacity-50"
                  >
                    Delete
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
