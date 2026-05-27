import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import {
  FolderGit, Trash2, Plus, Users, UserPlus, UserMinus,
  FolderOpen, ChevronRight, Brain, GitBranch,
} from 'lucide-react'
import {
  Modal, ModalCloseButton,
} from '../components/ui/Modal/Modal'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'

export default function Projects() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  // Sheet state
  const [sheetOpen, setSheetOpen] = useState(false)
  const [sheetTab, setSheetTab] = useState<'memories' | 'overrides'>('memories')
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null)

  // Create Project Form
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [parentId, setParentId] = useState('')
  const [errorMsg, setErrorMsg] = useState('')

  // Add Member Form
  const [selectedUserId, setSelectedUserId] = useState('')
  const [selectedRole, setSelectedRole] = useState('viewer')
  const [memberErrorMsg, setMemberErrorMsg] = useState('')

  // Queries
  const { data: projects, isLoading: projectsLoading } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
  })

  const { data: users, isLoading: usersLoading } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
  })

  const { data: roles } = useQuery({
    queryKey: ['roles'],
    queryFn: () => client.listRoles(),
  })

  const selectedProject = useMemo(
    () => projects?.find(p => p.id === selectedProjectId) ?? null,
    [projects, selectedProjectId],
  )

  const { data: projectMembers, isLoading: membersLoading } = useQuery({
    queryKey: ['projects', selectedProjectId, 'members'],
    queryFn: () => client.listProjectMembers(selectedProjectId!),
    enabled: !!selectedProjectId && sheetOpen && sheetTab === 'overrides',
  })

  const { data: projectMemories, isLoading: memoriesLoading } = useQuery({
    queryKey: ['memories', 'project', selectedProject?.name],
    queryFn: () => client.listMemories({ project: selectedProject!.name, limit: 30 }),
    enabled: !!selectedProject && sheetOpen && sheetTab === 'memories',
  })

  // Hierarchical tree
  const { rootProjects, childrenMap } = useMemo(() => {
    const projectIds = new Set((projects ?? []).map(p => p.id))
    const roots = (projects ?? []).filter(p => !p.parent_id || !projectIds.has(p.parent_id))
    const map: Record<string, typeof projects> = {}
    ;(projects ?? []).forEach(p => {
      if (p.parent_id && projectIds.has(p.parent_id)) {
        if (!map[p.parent_id]) map[p.parent_id] = []
        map[p.parent_id]!.push(p)
      }
    })
    return { rootProjects: roots, childrenMap: map }
  }, [projects])

  const parentOptions = useMemo(
    () => (projects ?? []).filter(p => p.id !== selectedProjectId),
    [projects, selectedProjectId],
  )

  const allAvailableRoles = useMemo(() => {
    const standard = ['admin', 'member', 'viewer']
    const custom = roles?.map(r => r.name) || []
    return Array.from(new Set([...standard, ...custom]))
  }, [roles])

  const nonMemberUsers = useMemo(() => {
    if (!users || !projectMembers) return users || []
    const memberIds = new Set(projectMembers.map(m => m.user_id))
    return users.filter(u => !memberIds.has(u.id) && u.status === 'active')
  }, [users, projectMembers])

  // Mutations
  const createProjectMut = useMutation({
    mutationFn: (data: { name: string; description?: string; parent_id?: string }) =>
      client.createProject(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      setName('')
      setDescription('')
      setParentId('')
      setErrorMsg('')
    },
    onError: (err: any) => setErrorMsg(err.message || 'Failed to create project'),
  })

  const deleteProjectMut = useMutation({
    mutationFn: (id: string) => client.deleteProject(id),
    onSuccess: (_, deletedId) => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      if (selectedProjectId === deletedId) {
        setSheetOpen(false)
        setSelectedProjectId(null)
      }
    },
  })

  const updateProjectMut = useMutation({
    mutationFn: ({ id, parent_id }: { id: string; parent_id: string | null }) =>
      client.updateProject(id, { parent_id }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const addMemberMut = useMutation({
    mutationFn: (data: { projectId: string; userId: string; role: string }) =>
      client.upsertProjectMember(data.projectId, data.userId, data.role),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['projects', selectedProjectId, 'members'] })
      setSelectedUserId('')
      setSelectedRole('viewer')
      setMemberErrorMsg('')
    },
    onError: (err: any) => setMemberErrorMsg(err.message || 'Failed to add project member'),
  })

  const deleteMemberMut = useMutation({
    mutationFn: (data: { projectId: string; userId: string }) =>
      client.deleteProjectMember(data.projectId, data.userId),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: ['projects', selectedProjectId, 'members'] }),
  })

  const handleProjectClick = (projectId: string) => {
    if (selectedProjectId !== projectId) {
      setSheetTab('memories')
      setSelectedUserId('')
      setMemberErrorMsg('')
    }
    setSelectedProjectId(projectId)
    setSheetOpen(true)
  }

  const handleCreateProject = (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) { setErrorMsg('Project Name is required.'); return }
    createProjectMut.mutate({
      name: name.trim().toLowerCase().replace(/\s+/g, '-'),
      description: description.trim() || undefined,
      parent_id: parentId || undefined,
    })
  }

  const handleAddMember = (e: React.FormEvent) => {
    e.preventDefault()
    if (!selectedProjectId) return
    if (!selectedUserId) { setMemberErrorMsg('Please select a user.'); return }
    addMemberMut.mutate({ projectId: selectedProjectId, userId: selectedUserId, role: selectedRole })
  }

  const renderProjectRow = (project: NonNullable<typeof projects>[number], indent = false) => (
    <div
      key={project.id}
      onClick={() => handleProjectClick(project.id)}
      className={`p-4 flex items-start justify-between gap-4 cursor-pointer transition-colors hover:bg-surface-secondary/20 ${
        indent ? 'pl-10 border-l-2 border-border-secondary ml-4' : ''
      }`}
    >
      <div className="flex items-center gap-2 flex-1 min-w-0">
        {indent
          ? <ChevronRight className="w-3.5 h-3.5 text-text-tertiary flex-shrink-0" />
          : <FolderOpen className="w-4 h-4 text-text-tertiary flex-shrink-0" />
        }
        <div className="min-w-0">
          <span className="font-semibold text-text-primary text-sm truncate block">{project.name}</span>
          {project.description && (
            <p className="text-xs text-text-tertiary truncate">{project.description}</p>
          )}
          <span className="text-[10px] text-text-tertiary">{new Date(project.created_at).toLocaleDateString()}</span>
        </div>
      </div>

      <div className="flex items-center gap-1 flex-shrink-0">
        <button
          onClick={e => { e.stopPropagation(); handleProjectClick(project.id) }}
          className="p-1.5 rounded-lg text-text-tertiary hover:text-accent-blue hover:bg-surface-secondary/60 transition-colors"
          title="Open project"
        >
          <ChevronRight className="w-4 h-4" />
        </button>
        <button
          onClick={e => {
            e.stopPropagation()
            if (confirm(`Delete project "${project.name}"? This will detach all associated memories.`)) {
              deleteProjectMut.mutate(project.id)
            }
          }}
          className="p-1.5 rounded-lg text-text-tertiary hover:text-status-error hover:bg-surface-secondary/60 transition-colors"
          title="Delete Project"
        >
          <Trash2 className="w-4 h-4" />
        </button>
      </div>
    </div>
  )

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div>
        <h1 className="text-lg font-semibold text-text-primary">Projects & Scopes</h1>
        <p className="text-[12px] text-text-tertiary mt-0.5">
          Manage organization projects and configure dynamic per-project user role overrides.
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Project List */}
        <div className="lg:col-span-2">
          <div className="border border-border-primary rounded-xl overflow-hidden bg-bg-primary">
            <div className="px-4 py-3 border-b border-border-secondary bg-surface-secondary/40">
              <span className="text-xs font-semibold text-text-secondary">Projects</span>
            </div>
            <div className="divide-y divide-border-secondary">
              {projectsLoading ? (
                <div className="p-4 text-center text-sm text-text-tertiary">Loading projects...</div>
              ) : !projects?.length ? (
                <div className="p-4 text-center text-sm text-text-tertiary">No projects defined yet.</div>
              ) : (
                rootProjects.map(root => (
                  <div key={root.id} className="divide-y divide-border-secondary">
                    {renderProjectRow(root)}
                    {childrenMap[root.id]?.map(child => (
                      <div key={child.id}>{renderProjectRow(child, true)}</div>
                    ))}
                  </div>
                ))
              )}
            </div>
          </div>
        </div>

        {/* Create Project Form */}
        <div>
          <div className="border border-border-primary rounded-xl p-5 bg-bg-primary space-y-4">
            <div>
              <h3 className="text-sm font-semibold text-text-primary flex items-center gap-2">
                <FolderGit className="w-4 h-4 text-accent-blue" />
                Create Project
              </h3>
              <p className="text-[11px] text-text-tertiary mt-0.5">
                Register a new workspace scope inside the organization.
              </p>
            </div>

            {errorMsg && (
              <div className="p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-lg">
                {errorMsg}
              </div>
            )}

            <form onSubmit={handleCreateProject} className="space-y-4 text-xs">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
                  Project Name (slug)
                </label>
                <input
                  type="text"
                  placeholder="e.g. core-payments"
                  value={name}
                  onChange={e => setName(e.target.value)}
                  className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/40"
                  required
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
                  Description
                </label>
                <textarea
                  placeholder="What is this scope about?"
                  value={description}
                  onChange={e => setDescription(e.target.value)}
                  className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/40 h-20 resize-none"
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
                  Parent Project
                </label>
                <Select value={parentId} onValueChange={setParentId}>
                  <SelectTrigger className="h-8 text-xs">
                    <SelectValue placeholder="— None (root) —" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="">— None (root) —</SelectItem>
                    {(projects ?? []).map(p => (
                      <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <button
                type="submit"
                disabled={createProjectMut.isPending}
                className="w-full flex items-center justify-center gap-2 px-3 py-2.5 rounded-lg bg-accent-blue hover:bg-accent-blue-hover text-white font-medium transition-colors disabled:opacity-50"
              >
                <Plus className="w-4 h-4" />
                Create Project
              </button>
            </form>
          </div>
        </div>
      </div>

      {/* Project Sheet */}
      <Modal open={sheetOpen} onOpenChange={setSheetOpen} position="right" size="lg">
        <ModalCloseButton />

        {selectedProject && (
          <div className="flex flex-col h-full pt-2">
            {/* Header */}
            <div className="mb-5">
              <div className="flex items-center gap-2 mb-1">
                <FolderOpen className="w-5 h-5 text-accent-blue flex-shrink-0" />
                <h2 className="text-lg font-semibold text-text-primary font-mono truncate">
                  {selectedProject.name}
                </h2>
              </div>
              {selectedProject.description && (
                <p className="text-sm text-text-tertiary mb-3">{selectedProject.description}</p>
              )}

              {/* Parent selector */}
              <div className="flex items-center gap-2 mt-3">
                <GitBranch className="w-3.5 h-3.5 text-text-tertiary flex-shrink-0" />
                <div className="flex-1">
                  <Select
                    key={selectedProject.id}
                    value={selectedProject.parent_id ?? ''}
                    onValueChange={v =>
                      updateProjectMut.mutate({
                        id: selectedProject.id,
                        parent_id: v || null,
                      })
                    }
                  >
                    <SelectTrigger className="h-8 text-xs">
                      <SelectValue placeholder="— No parent —" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="">— No parent —</SelectItem>
                      {parentOptions.map(p => (
                        <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>
            </div>

            {/* Tabs */}
            <div className="flex gap-0 border-b border-border-secondary mb-4">
              {(['memories', 'overrides'] as const).map(tab => (
                <button
                  key={tab}
                  onClick={() => setSheetTab(tab)}
                  className={`flex items-center gap-1.5 px-4 py-2 text-xs font-medium border-b-2 -mb-px transition-colors ${
                    sheetTab === tab
                      ? 'text-accent-blue border-accent-blue'
                      : 'text-text-tertiary border-transparent hover:text-text-secondary'
                  }`}
                >
                  {tab === 'memories'
                    ? <><Brain className="w-3.5 h-3.5" /> Memories</>
                    : <><Users className="w-3.5 h-3.5" /> Role Overrides</>
                  }
                </button>
              ))}
            </div>

            {/* Tab Content */}
            <div className="flex-1 overflow-y-auto min-h-0">

              {/* Memories Tab */}
              {sheetTab === 'memories' && (
                <div className="space-y-2">
                  {memoriesLoading ? (
                    <div className="text-center py-8 text-sm text-text-tertiary">Loading memories...</div>
                  ) : !projectMemories?.length ? (
                    <div className="text-center py-8 text-sm text-text-tertiary border border-dashed border-border-secondary rounded-xl">
                      No memories stored for this project.
                    </div>
                  ) : (
                    projectMemories.map(memory => (
                      <div
                        key={memory.id}
                        className="p-3 border border-border-secondary rounded-lg bg-surface-secondary/10 space-y-1"
                      >
                        {memory.title && (
                          <div className="text-xs font-semibold text-text-primary">{memory.title}</div>
                        )}
                        <p className="text-xs text-text-secondary line-clamp-3">{memory.content}</p>
                        <div className="flex items-center gap-2 pt-1">
                          {memory.type && (
                            <span className="text-[10px] bg-accent-blue/10 text-accent-blue px-1.5 py-0.5 rounded font-mono">
                              {memory.type}
                            </span>
                          )}
                          <span className="text-[10px] text-text-tertiary ml-auto">
                            {new Date(memory.created_at).toLocaleDateString()}
                          </span>
                        </div>
                      </div>
                    ))
                  )}
                </div>
              )}

              {/* Role Overrides Tab */}
              {sheetTab === 'overrides' && (
                <div className="space-y-6">
                  <p className="text-[11px] text-text-tertiary">
                    Configure user roles that override their global roles within this project.
                  </p>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                    {/* Members list */}
                    <div className="space-y-3">
                      <span className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider block">
                        Assigned Users
                      </span>

                      {membersLoading ? (
                        <div className="text-center py-4 text-xs text-text-tertiary">Loading...</div>
                      ) : !projectMembers?.length ? (
                        <div className="text-center py-4 text-xs text-text-tertiary border border-dashed border-border-secondary rounded-lg">
                          No overrides set. Users inherit global roles.
                        </div>
                      ) : (
                        <div className="space-y-2 max-h-64 overflow-y-auto pr-1">
                          {projectMembers.map(member => (
                            <div
                              key={member.id}
                              className="flex items-center justify-between p-3 border border-border-secondary rounded-lg bg-surface-secondary/10"
                            >
                              <div>
                                <div className="font-medium text-xs text-text-primary">
                                  {member.name || member.email}
                                </div>
                                <div className="text-[10px] text-text-tertiary">{member.email}</div>
                              </div>
                              <div className="flex items-center gap-3">
                                <span className="text-[10px] bg-accent-blue/10 text-accent-blue px-2 py-0.5 rounded font-mono font-medium uppercase">
                                  {member.role}
                                </span>
                                <button
                                  onClick={() => {
                                    if (confirm(`Remove override for ${member.name || member.email}?`)) {
                                      deleteMemberMut.mutate({
                                        projectId: selectedProject.id,
                                        userId: member.user_id,
                                      })
                                    }
                                  }}
                                  className="p-1 rounded text-text-tertiary hover:text-status-error hover:bg-surface-secondary transition-colors"
                                >
                                  <UserMinus className="w-3.5 h-3.5" />
                                </button>
                              </div>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>

                    {/* Add member form */}
                    <div className="space-y-4 border-t md:border-t-0 md:border-l border-border-secondary pt-4 md:pt-0 md:pl-6">
                      <span className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider block">
                        Assign Override
                      </span>

                      {memberErrorMsg && (
                        <div className="p-2 text-[10px] bg-status-error/10 border border-status-error/20 text-status-error rounded">
                          {memberErrorMsg}
                        </div>
                      )}

                      <form onSubmit={handleAddMember} className="space-y-3 text-xs">
                        <div className="space-y-1">
                          <label className="text-[10px] text-text-tertiary">Select User</label>
                          {usersLoading ? (
                            <div className="text-[10px] text-text-tertiary">Loading users...</div>
                          ) : (
                            <Select value={selectedUserId} onValueChange={setSelectedUserId}>
                              <SelectTrigger className="h-8 text-xs">
                                <SelectValue placeholder="-- Choose User --" />
                              </SelectTrigger>
                              <SelectContent>
                                {nonMemberUsers.map(u => (
                                  <SelectItem key={u.id} value={u.id}>
                                    {u.name} ({u.email})
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                          )}
                        </div>

                        <div className="space-y-1">
                          <label className="text-[10px] text-text-tertiary">Override Role</label>
                          <Select value={selectedRole} onValueChange={setSelectedRole}>
                            <SelectTrigger className="h-8 text-xs">
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                              {allAvailableRoles.map(r => (
                                <SelectItem key={r} value={r}>{r}</SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        </div>

                        <button
                          type="submit"
                          disabled={addMemberMut.isPending}
                          className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg bg-accent-blue hover:bg-accent-blue-hover text-white font-medium transition-colors text-xs disabled:opacity-50"
                        >
                          <UserPlus className="w-3.5 h-3.5" />
                          Save Override
                        </button>
                      </form>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </Modal>
    </div>
  )
}
