import { useEffect, useMemo, useState, lazy, Suspense } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Loader2, Network } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { usePersistedGraphState } from '../hooks/usePersistedGraphState'
import { GraphTabs } from '../components/graph/chrome'
import type { CodeProject, Project, ProjectGraphInfo } from '../types'

const OrgMemoryGraph = lazy(() => import('../components/OrgMemoryGraph'))
const CodeGraph = lazy(() => import('../components/CodeGraph'))

const SELECTED_PROJECT_KEY = 'nexusmind-graph-page-project'
const SELECTED_REPO_KEY = 'nexusmind-graph-page-repo'
const ACTIVE_TAB_KEY = 'nexusmind-graph-page-tab'
const STORAGE_VERSION = 1

/** Graph sources exposed by the page. */
type GraphSource = 'knowledge' | 'code'

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

  // The code graph exposes repository structure, which lives behind the same
  // permission as the Code page — `/graph` itself is open to every member, so
  // the tab must be gated explicitly rather than inherited from the route.
  const canReadCode = (session?.user.permissions ?? []).includes('code:read')

  const [selectedProjectId, setSelectedProjectId, resetSelectedProject] = usePersistedGraphState<string>(
    SELECTED_PROJECT_KEY,
    '',
    { version: STORAGE_VERSION },
  )
  const [selectedRepo, setSelectedRepo] = usePersistedGraphState<string>(
    SELECTED_REPO_KEY,
    '',
    { version: STORAGE_VERSION },
  )
  const [storedTab, setStoredTab] = usePersistedGraphState<GraphSource>(
    ACTIVE_TAB_KEY,
    'knowledge',
    { version: STORAGE_VERSION },
  )
  const [graphFocused, setGraphFocused] = useState(false)

  // A persisted 'code' tab must not resurrect for a user who lost `code:read`.
  const activeTab: GraphSource = canReadCode ? storedTab : 'knowledge'

  const { data: projects, isLoading: projectsLoading } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    staleTime: 60_000,
  })

  // Fetched as soon as the user can read code — not only on the Code tab — so
  // switching tabs is instant and the repo auto-match below has data to work
  // with. Same key + params as the Code page's non-archived query, so
  // navigating between the two reuses one cache entry instead of refetching.
  const { data: codeProjects, isLoading: codeProjectsLoading, isError: codeProjectsError } = useQuery({
    queryKey: ['code-projects', false],
    queryFn: () => client.listCodeProjects({ include_archived: false }),
    enabled: canReadCode,
    staleTime: 60_000,
  })

  const activeProjects = useMemo(
    () => projects?.filter(p => !p.archived_at) ?? [],
    [projects],
  )

  const indexedRepos = useMemo(
    () => codeProjects?.filter((p: CodeProject) => p.last_indexed != null && !p.archived_at) ?? [],
    [codeProjects],
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

  // Repository selection is independent of the project selection (code_projects
  // is a separate table keyed by name, with no FK to projects). On first entry
  // to the Code tab — or when the stored repo no longer exists — preselect the
  // repo that shares the active project's name, else the first indexed one.
  useEffect(() => {
    if (activeTab !== 'code' || indexedRepos.length === 0) return
    if (selectedRepo && indexedRepos.some(r => r.name === selectedRepo)) return
    const projectName = activeProjects.find(p => p.id === selectedProjectId)?.name
    const match = projectName ? indexedRepos.find(r => r.name === projectName) : undefined
    setSelectedRepo((match ?? indexedRepos[0]).name)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, indexedRepos, selectedProjectId, activeProjects])

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

  const knowledgeSubtitle = isAdmin
    ? 'Project-scoped knowledge graph — memories, sessions, users, collections, tags, and audit events.'
    : `${session?.org.name ?? 'Organization'} — project-scoped knowledge graph.`

  // Only rendered when the user can read code, so it never leaks the tab.
  const tabs = canReadCode ? (
    <GraphTabs<GraphSource>
      value={activeTab}
      onChange={setStoredTab}
      label="Graph source"
      tabs={[
        { id: 'knowledge', label: 'Knowledge' },
        { id: 'code', label: 'Code' },
      ]}
    />
  ) : undefined

  // No projects at all → dedicated guidance (no dropdown to offer). The Code
  // tab is unaffected: repositories don't depend on knowledge projects.
  if (!projectsLoading && activeProjects.length === 0 && activeTab === 'knowledge') {
    return (
      <div className="p-8 max-w-6xl mx-auto">
        <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">Graph</h1>
        <p className="text-[13px] text-text-secondary mt-1">Project-scoped knowledge graph.</p>
        {tabs && <div className="mt-4 w-fit">{tabs}</div>}
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
        {activeTab === 'knowledge' ? (
          <OrgMemoryGraph
            family={familyInfo}
            familyId={selectedProjectId}
            storageKey="page"
            title="Graph"
            subtitle={knowledgeSubtitle}
            projects={activeProjects}
            selectedProjectId={selectedProjectId}
            onSelectProject={setSelectedProjectId}
            projectsLoading={projectsLoading}
            tabs={tabs}
            onFocusedChange={setGraphFocused}
            emptyTitle={selectedProjectId ? 'No data for this project' : 'Select a project'}
            emptyDescription={selectedProjectId
              ? 'This project family has no memories, code, or audit events yet.'
              : 'Choose a project from the dropdown to explore its knowledge graph.'}
          />
        ) : (
          <CodeGraph
            projects={codeProjects}
            projectsLoading={codeProjectsLoading}
            projectsError={codeProjectsError}
            selectedRepo={selectedRepo}
            onSelectRepo={setSelectedRepo}
            storageKey="page"
            title="Graph"
            subtitle="Repository code graph — files, modules, and the symbols they define."
            tabs={tabs}
            onFocusedChange={setGraphFocused}
          />
        )}
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
