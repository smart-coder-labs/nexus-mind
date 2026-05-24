import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { FolderGit, Trash2, Plus, Users, UserPlus, UserMinus, FolderOpen } from 'lucide-react'

export default function Projects() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  // UI State
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null)
  
  // Create Project Form State
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [errorMsg, setErrorMsg] = useState('')

  // Add Member Form State
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

  const { data: projectMembers, isLoading: membersLoading } = useQuery({
    queryKey: ['projects', selectedProjectId, 'members'],
    queryFn: () => client.listProjectMembers(selectedProjectId!),
    enabled: !!selectedProjectId,
  })

  // Mutations
  const createProjectMut = useMutation({
    mutationFn: (data: { name: string; description?: string }) => client.createProject(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      setName('')
      setDescription('')
      setErrorMsg('')
    },
    onError: (err: any) => {
      setErrorMsg(err.message || 'Failed to create project')
    },
  })

  const deleteProjectMut = useMutation({
    mutationFn: (id: string) => client.deleteProject(id),
    onSuccess: (_, deletedId) => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      if (selectedProjectId === deletedId) {
        setSelectedProjectId(null)
      }
    },
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
    onError: (err: any) => {
      setMemberErrorMsg(err.message || 'Failed to add project member')
    },
  })

  const deleteMemberMut = useMutation({
    mutationFn: (data: { projectId: string; userId: string }) =>
      client.deleteProjectMember(data.projectId, data.userId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['projects', selectedProjectId, 'members'] })
    },
  })

  const handleCreateProject = (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) {
      setErrorMsg('Project Name is required.')
      return
    }
    const formattedName = name.trim().toLowerCase().replace(/\s+/g, '-')
    createProjectMut.mutate({
      name: formattedName,
      description: description.trim() || undefined,
    })
  }

  const handleAddMember = (e: React.FormEvent) => {
    e.preventDefault()
    if (!selectedProjectId) return
    if (!selectedUserId) {
      setMemberErrorMsg('Please select a user.')
      return
    }
    addMemberMut.mutate({
      projectId: selectedProjectId,
      userId: selectedUserId,
      role: selectedRole,
    })
  }

  const selectedProject = useMemo(() => {
    return projects?.find(p => p.id === selectedProjectId)
  }, [projects, selectedProjectId])

  // Get list of standard + custom roles to choose from
  const allAvailableRoles = useMemo(() => {
    const standard = ['admin', 'member', 'viewer']
    const custom = roles?.map(r => r.name) || []
    return Array.from(new Set([...standard, ...custom]))
  }, [roles])

  // Filter users that are not already members of this project to avoid double adding
  const nonMemberUsers = useMemo(() => {
    if (!users || !projectMembers) return users || []
    const memberIds = new Set(projectMembers.map(m => m.user_id))
    return users.filter(u => !memberIds.has(u.id) && u.status === 'active')
  }, [users, projectMembers])

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div>
        <h1 className="text-lg font-semibold text-text-primary">Projects & Scopes</h1>
        <p className="text-[12px] text-text-tertiary mt-0.5">
          Manage organization projects and configure dynamic per-project user role overrides.
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Left/Middle Column: Projects list & project details */}
        <div className="lg:col-span-2 space-y-8">
          <div className="border border-border-primary rounded-xl overflow-hidden bg-bg-primary">
            <div className="px-4 py-3 border-b border-border-secondary bg-surface-secondary/40">
              <span className="text-xs font-semibold text-text-secondary">Projects</span>
            </div>
            <div className="divide-y divide-border-secondary">
              {projectsLoading ? (
                <div className="p-4 text-center text-sm text-text-tertiary">Loading projects...</div>
              ) : projects?.length === 0 ? (
                <div className="p-4 text-center text-sm text-text-tertiary">No projects defined yet.</div>
              ) : (
                projects?.map(project => {
                  const isSelected = project.id === selectedProjectId
                  return (
                    <div
                      key={project.id}
                      onClick={() => setSelectedProjectId(project.id)}
                      className={`p-4 flex items-start justify-between gap-4 cursor-pointer transition-colors ${
                        isSelected ? 'bg-surface-secondary/40' : 'hover:bg-surface-secondary/20'
                      }`}
                    >
                      <div className="space-y-1">
                        <div className="flex items-center gap-2">
                          <FolderOpen className={`w-4 h-4 ${isSelected ? 'text-accent-blue' : 'text-text-tertiary'}`} />
                          <span className="font-semibold text-text-primary">{project.name}</span>
                        </div>
                        {project.description && (
                          <p className="text-xs text-text-tertiary">{project.description}</p>
                        )}
                        <span className="text-[10px] text-text-tertiary block">
                          Created at: {new Date(project.created_at).toLocaleDateString()}
                        </span>
                      </div>

                      <button
                        onClick={(e) => {
                          e.stopPropagation()
                          if (
                            confirm(
                              `Are you sure you want to delete project "${project.name}"? This will detach all associated memories.`
                            )
                          ) {
                            deleteProjectMut.mutate(project.id)
                          }
                        }}
                        className="p-1.5 rounded-lg text-text-tertiary hover:text-status-error hover:bg-surface-secondary/60 transition-colors"
                        title="Delete Project"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  )
                })
              )}
            </div>
          </div>

          {/* Project Details: Members & Overrides */}
          {selectedProject && (
            <div className="border border-border-primary rounded-xl overflow-hidden bg-bg-primary p-6 space-y-6">
              <div>
                <h2 className="text-sm font-semibold text-text-primary flex items-center gap-2">
                  <Users className="w-4 h-4 text-accent-blue" />
                  Role Overrides for Project: <span className="text-accent-blue font-mono">{selectedProject.name}</span>
                </h2>
                <p className="text-[11px] text-text-tertiary mt-0.5">
                  Configure specific user roles that override their global organizational roles within this project.
                </p>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                {/* Members list */}
                <div className="md:col-span-2 space-y-3">
                  <span className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider block">
                    Assigned Users
                  </span>
                  
                  {membersLoading ? (
                    <div className="text-center py-4 text-xs text-text-tertiary">Loading members...</div>
                  ) : projectMembers?.length === 0 ? (
                    <div className="text-center py-4 text-xs text-text-tertiary border border-dashed border-border-secondary rounded-lg">
                      No overrides set. All users inherit their global roles.
                    </div>
                  ) : (
                    <div className="space-y-2 max-h-64 overflow-y-auto pr-1">
                      {projectMembers?.map(member => (
                        <div
                          key={member.id}
                          className="flex items-center justify-between p-3 border border-border-secondary rounded-lg bg-surface-secondary/10"
                        >
                          <div>
                            <div className="font-medium text-xs text-text-primary">{member.name || member.email}</div>
                            <div className="text-[10px] text-text-tertiary">{member.email}</div>
                          </div>
                          
                          <div className="flex items-center gap-3">
                            <span className="text-[10px] bg-accent-blue/10 text-accent-blue px-2 py-0.5 rounded font-mono font-medium uppercase">
                              {member.role}
                            </span>
                            <button
                              onClick={() => {
                                if (
                                  confirm(
                                    `Remove override for ${member.name || member.email} in project "${
                                      selectedProject.name
                                    }"?`
                                  )
                                ) {
                                  deleteMemberMut.mutate({
                                    projectId: selectedProject.id,
                                    userId: member.user_id,
                                  })
                                }
                              }}
                              className="p-1 rounded text-text-tertiary hover:text-status-error hover:bg-surface-secondary transition-colors"
                              title="Remove Override"
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
                        <select
                          value={selectedUserId}
                          onChange={e => setSelectedUserId(e.target.value)}
                          className="w-full bg-bg-secondary border border-border-primary rounded-lg px-2 py-1.5 text-text-primary focus:outline-none"
                          required
                        >
                          <option value="">-- Choose User --</option>
                          {nonMemberUsers.map(u => (
                            <option key={u.id} value={u.id}>
                              {u.name} ({u.email})
                            </option>
                          ))}
                        </select>
                      )}
                    </div>

                    <div className="space-y-1">
                      <label className="text-[10px] text-text-tertiary">Override Role</label>
                      <select
                        value={selectedRole}
                        onChange={e => setSelectedRole(e.target.value)}
                        className="w-full bg-bg-secondary border border-border-primary rounded-lg px-2 py-1.5 text-text-primary focus:outline-none"
                        required
                      >
                        {allAvailableRoles.map(r => (
                          <option key={r} value={r}>
                            {r}
                          </option>
                        ))}
                      </select>
                    </div>

                    <button
                      type="submit"
                      disabled={addMemberMut.isPending}
                      className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg bg-accent-blue hover:bg-accent-blue-hover text-white font-medium transition-colors text-xs"
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

        {/* Right Column: Create Project Form */}
        <div className="space-y-4">
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
    </div>
  )
}
