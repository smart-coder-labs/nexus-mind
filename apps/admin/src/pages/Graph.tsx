import { useEffect, useMemo, useState, lazy, Suspense } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Loader2, Network } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { usePersistedGraphState } from '../hooks/usePersistedGraphState'
import type { Project, ProjectGraphInfo } from '../types'

const OrgMemoryGraph = lazy(() => import('../components/OrgMemoryGraph'))

const SELECTED_PROJECT_KEY = 'nexusmind-graph-page-project'
const STORAGE_VERSION = 1

/**
 * BFS walk that collects the full connected family of a project: the root
 * itself + every descendant in `parent_id`. Returns the projects (not just
 * ids) so the caller can build the legend.
 *
 * Cycle-safe: tracks visited ids so a pre-existing `parent_id` cycle in the
 * data terminates without infinite looping.
 */
function buildProjectFamily(rootId: string, allProjects: Project[]): Project[] {
  const byId = new Map<string, Project>()
  allProjects.forEach(p => byId.set(p.id, p))

  const root = byId.get(rootId)
  if (!root) return []

  const family: Project[] = []
  const visitedIds = new Set<string>()
  const queue: string[] = [root.id]

  while (queue.length > 0) {
    const id = queue.shift()!
    if (visitedIds.has(id)) continue
    visitedIds.add(id)

    const p = byId.get(id)
    if (!p) continue
    family.push(p)

    for (const child of allProjects) {
      if (child.parent_id === id && !visitedIds.has(child.id)) {
        queue.push(child.id)
      }
    }
  }

  return family
}

export default function Graph() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const isAdmin = (session?.user.role === 'admin' || session?.user.role === 'super_user')

  const [selectedProjectId, setSelectedProjectId, resetSelectedProject] = usePersistedGraphState<string>(
    SELECTED_PROJECT_KEY,
    '',
    { version: STORAGE_VERSION },
  )
  const [graphFocused, setGraphFocused] = useState(false)

  const { data: projects, isLoading: projectsLoading } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    staleTime: 60_000,
  })

  const activeProjects = useMemo(
    () => projects?.filter(p => !p.archived_at) ?? [],
    [projects],
  )

  // Default the selection to the first non-archived project on first load,
  // but only when nothing is persisted yet.
  useEffect(() => {
    if (selectedProjectId || activeProjects.length === 0) return
    setSelectedProjectId(activeProjects[0].id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProjects])

  // Clear stale persisted project selection if the user can't access it.
  useEffect(() => {
    if (projectsLoading || !projects) return
    if (selectedProjectId && !projects.some(p => p.id === selectedProjectId)) {
      resetSelectedProject()
    }
  }, [projects, projectsLoading, selectedProjectId, resetSelectedProject])

  const family = useMemo(() => {
    if (!selectedProjectId || activeProjects.length === 0) return []
    return buildProjectFamily(selectedProjectId, activeProjects)
  }, [selectedProjectId, activeProjects])

  const familyInfo: ProjectGraphInfo[] = useMemo(
    () => family.map(p => ({
      id:        p.id,
      name:      p.name,
      color:     fallbackColorFor(p.id),
      parent_id: p.parent_id,
    })),
    [family],
  )

  const subtitle = isAdmin
    ? 'Project-scoped knowledge graph — memories, sessions, users, collections, tags, and audit events.'
    : `${session?.org.name ?? 'Organization'} — project-scoped knowledge graph.`

  // No projects at all → dedicated guidance (no dropdown to offer).
  if (!projectsLoading && activeProjects.length === 0) {
    return (
      <div className="p-8 max-w-6xl mx-auto">
        <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">Graph</h1>
        <p className="text-[13px] text-text-secondary mt-1">Project-scoped knowledge graph.</p>
        <div className="mt-10 border border-border-primary rounded-[18px] p-10 text-center space-y-2">
          <Network className="w-6 h-6 text-text-quaternary/40 mx-auto" />
          <p className="text-xs font-semibold text-text-secondary">No projects yet</p>
          <p className="text-xs text-text-quaternary">Create a project to visualize its memory graph.</p>
        </div>
      </div>
    )
  }

  return (
    // Full-bleed: the graph canvas spans the whole window and runs UNDER the
    // floating glass sidebar (z-30), exactly like the design. In focus mode
    // the page raises its own z-index above the sidebar — the inner overlay's
    // z-[100] alone can't outrank it (bounded by this stacking context).
    <div className={`fixed inset-0 overflow-hidden bg-[#07080c] ${graphFocused ? 'z-[100]' : 'z-10'}`}>
      <Suspense fallback={
        <div className="absolute inset-0 flex items-center justify-center">
          <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
        </div>
      }>
        <OrgMemoryGraph
          family={familyInfo}
          familyId={selectedProjectId}
          storageKey="page"
          title="Graph"
          subtitle={subtitle}
          projects={activeProjects}
          selectedProjectId={selectedProjectId}
          onSelectProject={setSelectedProjectId}
          projectsLoading={projectsLoading}
          onFocusedChange={setGraphFocused}
          emptyTitle={selectedProjectId ? 'No data for this project' : 'Select a project'}
          emptyDescription={selectedProjectId
            ? 'This project family has no memories, code, or audit events yet.'
            : 'Choose a project from the dropdown to explore its knowledge graph.'}
        />
      </Suspense>
    </div>
  )
}

// ── Color fallback (matches the backend's 8-color palette) ──────────────────
//
// The backend is the source of truth for project colors, but the legend
// needs to render before the network round-trip completes. The fallback
// here MUST match the backend's `PROJECT_COLOR_PALETTE` + `color_for_project_id`
// so the legend and the graph stay in sync while the data loads.
const FALLBACK_PALETTE = [
  '#2997ff', '#34d399', '#fb923c', '#a78bfa',
  '#facc15', '#f472b6', '#22d3ee', '#fb7185',
]

function fallbackColorFor(id: string): string {
  let hash = 0x811c9dc5
  for (const byte of new TextEncoder().encode(id)) {
    hash ^= byte
    hash = Math.imul(hash, 0x01000193) >>> 0
  }
  return FALLBACK_PALETTE[hash % FALLBACK_PALETTE.length]
}
