import { useEffect, useMemo, lazy, Suspense } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ChevronDown, Network, Share2, X } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { usePersistedGraphState } from '../hooks/usePersistedGraphState'
import type { Project, ProjectGraphInfo } from '../types'

const OrgMemoryGraph = lazy(() => import('../components/OrgMemoryGraph'))

const SELECTED_PROJECT_KEY = 'nexusmind-graph-page-project'
const LEGEND_OPEN_KEY = 'nexusmind-graph-page-legend-open'
const STORAGE_VERSION = 1

// Keyboard focus indicator (matches the rest of the admin app): 2px --color-focus-ring
// outline with 2px offset. Uses outline (not ring) so it isn't clipped by overflow-hidden
// ancestors.
const FOCUS_CANVAS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

/**
 * BFS walk that collects the full connected family of a project: the root
 * itself + every descendant in `parent_id`. Returns ids (not names) so the
 * family matches the `project_id` contract on the new graph endpoint.
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

    // Walk down to children. parent_id -> child relationship.
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
  const isAdmin = session?.user.role === 'admin'

  // Persist the selected project id and the legend's open/closed state across
  // reloads. Versioned so future schema changes to the persisted payload
  // invalidate the cache automatically.
  const [selectedProjectId, setSelectedProjectId, resetSelectedProject] = usePersistedGraphState<string>(
    SELECTED_PROJECT_KEY,
    '',
    { version: STORAGE_VERSION },
  )
  const [legendOpen, setLegendOpen, resetLegendOpen] = usePersistedGraphState<boolean>(
    LEGEND_OPEN_KEY,
    true,
    { version: STORAGE_VERSION },
  )

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
  // but only when nothing is persisted yet. After that, the user's choice
  // wins — even if the project is later archived, we let the family walk
  // return empty and show the empty state.
  useEffect(() => {
    if (selectedProjectId || activeProjects.length === 0) return
    setSelectedProjectId(activeProjects[0].id)
    // Only re-run when the projects list shape changes — selectedProjectId
    // is intentionally excluded so the user can clear/change freely.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProjects])

  // Resolve the family. Uses the local BFS (same logic as the API) so the
  // legend can render before the network round-trip completes. The backend
  // is still the source of truth for the actual graph data and the
  // per-project colors — see OrgMemoryGraph.
  const family = useMemo(() => {
    if (!selectedProjectId || activeProjects.length === 0) return []
    return buildProjectFamily(selectedProjectId, activeProjects)
  }, [selectedProjectId, activeProjects])

  // Project info for the OrgMemoryGraph legend. We synthesize the color
  // here as a fallback (deterministic from the id) so the legend renders
  // even while the network call is in flight. The backend will replace
  // these with the canonical colors when the data arrives — we pass
  // them through unchanged.
  const familyInfo: ProjectGraphInfo[] = useMemo(
    () => family.map(p => ({
      id:        p.id,
      name:      p.name,
      color:     fallbackColorFor(p.id),
      parent_id: p.parent_id,
    })),
    [family],
  )

  const selectedProject = useMemo(
    () => family[0] ?? activeProjects.find(p => p.id === selectedProjectId) ?? null,
    [family, activeProjects, selectedProjectId],
  )

  // No project selected (first load with no projects at all, or stored
  // selection is stale) → guide the user to pick one.
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
    <div className="p-6 max-w-7xl mx-auto space-y-5">
      {/* Header + project selector */}
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">Graph</h1>
          <p className="text-[13px] text-text-secondary mt-1">
            {isAdmin
              ? 'Project-scoped knowledge graph — memories, sessions, users, collections, tags, and audit events.'
              : `${session?.org.name ?? 'Organization'} — project-scoped knowledge graph.`}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <ProjectSelect
            projects={activeProjects}
            value={selectedProjectId}
            onChange={setSelectedProjectId}
            disabled={projectsLoading}
          />
          {selectedProjectId && (
            <button
              type="button"
              onClick={() => resetSelectedProject()}
              className={`flex items-center gap-1 border border-border-primary rounded-full px-2.5 py-1.5 text-[12px] text-text-quaternary hover:text-text-secondary transition-colors ${FOCUS_CANVAS}`}
              aria-label="Clear project selection"
              title="Clear and pick another project"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>
      </div>

      {/* Legend card (open/closed persisted) — only when a project is selected */}
      {selectedProject && family.length > 0 && (
        <div className="border border-border-primary rounded-[18px] bg-background-tertiary/30">
          <button
            type="button"
            onClick={() => setLegendOpen(!legendOpen)}
            className={`w-full flex items-center justify-between gap-3 px-4 py-3 text-left ${FOCUS_CANVAS}`}
            aria-expanded={legendOpen}
            aria-controls="graph-legend-content"
          >
            <div className="flex items-center gap-2">
              <Share2 className="w-3.5 h-3.5 text-text-tertiary" />
              <p className="text-[12px] font-semibold text-text-primary">
                {family.length === 1
                  ? `Project: ${selectedProject.name}`
                  : `Project family: ${selectedProject.name} + ${family.length - 1} descendant${family.length - 1 === 1 ? '' : 's'}`}
              </p>
            </div>
            <ChevronDown
              className={`w-4 h-4 text-text-tertiary transition-transform ${legendOpen ? 'rotate-180' : ''}`}
              aria-hidden="true"
            />
          </button>
          {legendOpen && (
            <div
              id="graph-legend-content"
              className="px-4 pb-4 pt-1 space-y-2 border-t border-border-primary"
            >
              <p className="text-[11px] text-text-tertiary">
                Each project gets a stable color. Memory and project nodes in the graph
                are colored by their owning project. Click a swatch (coming soon) to
                isolate a single project.
              </p>
              <div className="flex items-center gap-2 flex-wrap" role="list" aria-label="Project legend">
                {familyInfo.map(p => (
                  <div
                    key={p.id}
                    role="listitem"
                    className="flex items-center gap-2 px-2 py-0.5 rounded-full bg-white/[0.04] border border-border-primary"
                  >
                    <span
                      className="w-3 h-3 rounded-full shrink-0"
                      style={{ backgroundColor: p.color, boxShadow: `0 0 8px ${p.color}66` }}
                      aria-hidden="true"
                    />
                    <span className="text-[11px] text-text-secondary">{p.name}</span>
                  </div>
                ))}
              </div>
              <button
                type="button"
                onClick={resetLegendOpen}
                className="text-[10px] text-text-quaternary hover:text-text-tertiary transition-colors"
              >
                Reset legend to default
              </button>
            </div>
          )}
        </div>
      )}

      {/* Graph body */}
      <Suspense fallback={
        <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
          <div className="w-5 h-5 animate-spin rounded-full border-2 border-text-quaternary border-t-transparent" />
        </div>
      }>
        {selectedProjectId ? (
          <OrgMemoryGraph
            family={familyInfo}
            familyId={selectedProjectId}
            storageKey="page"
            height={600}
            emptyTitle="No data for this project"
            emptyDescription="This project family has no memories, code, or audit events yet."
          />
        ) : (
          <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
            <Network className="w-6 h-6 text-text-quaternary/40 mx-auto" />
            <p className="text-xs font-semibold text-text-secondary">Select a project</p>
            <p className="text-xs text-text-quaternary">
              Choose a project from the dropdown to explore its knowledge graph.
            </p>
          </div>
        )}
      </Suspense>
    </div>
  )
}

// ── Project selector ─────────────────────────────────────────────────────────

function ProjectSelect({
  projects,
  value,
  onChange,
  disabled,
}: {
  projects: Project[]
  value: string
  onChange: (id: string) => void
  disabled: boolean
}) {
  return (
    <div className="relative">
      <select
        value={value}
        onChange={e => onChange(e.target.value)}
        disabled={disabled}
        aria-label="Select project"
        className={`appearance-none bg-white/[0.04] border border-border-primary rounded-[11px] pl-3 pr-9 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer disabled:opacity-50 ${FOCUS_CANVAS}`}
      >
        <option value="">Select a project…</option>
        {projects.map(p => (
          <option key={p.id} value={p.id}>
            {p.name}
            {p.parent_id ? '  (child)' : ''}
          </option>
        ))}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-quaternary" />
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
