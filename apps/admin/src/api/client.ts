import type {
  Org,
  OrgSettings,
  User,
  AuthSession,
  Memory,
  AuditEntry,
  OrgStats,
  MemoryFilters,
  AuditFilters,
  CustomRole,
  Project,
  ProjectMember,
  ProjectAccess,
} from '../types'

export class NexusMindClient {
  constructor(private readonly baseUrl: string) {}

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      credentials: 'include',
      headers: {
        'Content-Type': 'application/json',
        ...init?.headers,
      },
    })
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText, code: 'unknown' }))
      throw Object.assign(new Error(body.error ?? res.statusText), {
        code: body.code,
        status: res.status,
      })
    }
    // 204 No Content or empty body (Safari throws on JSON.parse(""))
    if (res.status === 204) return undefined as T
    const text = await res.text()
    if (!text) return undefined as T
    return JSON.parse(text) as T
  }

  // Auth
  getMe(): Promise<AuthSession> {
    return this.request('/v1/admin/auth/me')
  }

  logout(): Promise<void> {
    return this.request('/v1/admin/auth/logout', { method: 'POST' })
  }

  getStats(): Promise<OrgStats> {
    return this.request('/v1/admin/stats')
  }

  getOrg(): Promise<Org> {
    return this.request('/v1/admin/org')
  }

  updateOrg(data: { name: string }): Promise<Org> {
    return this.request('/v1/admin/org', { method: 'PATCH', body: JSON.stringify(data) })
  }

  getOrgSettings(): Promise<OrgSettings> {
    return this.request('/v1/admin/org/settings')
  }

  updateOrgSettings(data: OrgSettings): Promise<OrgSettings> {
    return this.request('/v1/admin/org/settings', { method: 'PATCH', body: JSON.stringify(data) })
  }

  listUsers(): Promise<User[]> {
    return this.request('/v1/users')
  }

  inviteUser(data: {
    email: string
    name: string
    role: string
    project_access?: ProjectAccess
  }): Promise<{ user: User; api_key: string }> {
    return this.request('/v1/users/invite', { method: 'POST', body: JSON.stringify(data) })
  }

  removeUser(id: string): Promise<void> {
    return this.request(`/v1/users/${id}`, { method: 'DELETE' })
  }

  rotateKey(userId: string): Promise<{ api_key: string }> {
    return this.request(`/v1/users/${userId}/rotate-key`, { method: 'POST' })
  }

  updateUserRole(userId: string, role: string): Promise<void> {
    return this.request(`/v1/users/${userId}/role`, {
      method: 'PATCH',
      body: JSON.stringify({ role }),
    })
  }

  listRoles(): Promise<CustomRole[]> {
    return this.request('/v1/roles')
  }

  createRole(data: {
    name: string
    display_name: string
    permissions: string[]
    description?: string
  }): Promise<CustomRole> {
    return this.request('/v1/roles', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  deleteRole(id: string): Promise<void> {
    return this.request(`/v1/roles/${id}`, { method: 'DELETE' })
  }

  listProjects(): Promise<Project[]> {
    return this.request('/v1/projects')
  }

  createProject(data: { name: string; description?: string; parent_id?: string }): Promise<Project> {
    return this.request('/v1/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  updateProject(id: string, data: { parent_id: string | null }): Promise<void> {
    return this.request(`/v1/projects/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    })
  }

  deleteProject(id: string): Promise<void> {
    return this.request(`/v1/projects/${id}`, { method: 'DELETE' })
  }

  listProjectMembers(projectId: string): Promise<ProjectMember[]> {
    return this.request(`/v1/projects/${projectId}/members`)
  }

  upsertProjectMember(projectId: string, userId: string, role: string): Promise<void> {
    return this.request(`/v1/projects/${projectId}/members`, {
      method: 'POST',
      body: JSON.stringify({ user_id: userId, role }),
    })
  }

  deleteProjectMember(projectId: string, userId: string): Promise<void> {
    return this.request(`/v1/projects/${projectId}/members/${userId}`, { method: 'DELETE' })
  }

  listMemories(filters: MemoryFilters = {}): Promise<Memory[]> {
    const params = new URLSearchParams()
    Object.entries(filters).forEach(([k, v]) => v != null && params.set(k, String(v)))
    return this.request(`/v1/memory?${params}`)
  }

  searchMemories(query: string, limit = 20, mode: 'keyword' | 'hybrid' | 'semantic' = 'hybrid'): Promise<Memory[]> {
    return this.request('/v1/memory/search', {
      method: 'POST',
      body: JSON.stringify({ query, limit, mode }),
    })
  }

  deleteMemory(id: string): Promise<void> {
    return this.request(`/v1/memory/${id}`, { method: 'DELETE' })
  }

  changePassword(data: { current_password: string; new_password: string }): Promise<{ message: string }> {
    return this.request('/v1/admin/auth/change-password', { method: 'POST', body: JSON.stringify(data) })
  }

  getAuditLog(filters: AuditFilters = {}): Promise<AuditEntry[]> {
    const params = new URLSearchParams()
    Object.entries(filters).forEach(([k, v]) => v != null && params.set(k, String(v)))
    return this.request(`/v1/audit?${params}`)
  }
}

export function createClient(): NexusMindClient {
  const baseUrl = import.meta.env.VITE_API_URL ?? ''
  return new NexusMindClient(baseUrl)
}

export async function listOrgs(superuserKey: string): Promise<Org[]> {
  const baseUrl = import.meta.env.VITE_API_URL ?? ''
  const res = await fetch(`${baseUrl}/v1/orgs`, {
    headers: { Authorization: `Bearer ${superuserKey}` },
  })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }))
    throw Object.assign(new Error(body.error ?? res.statusText), { code: body.code, status: res.status })
  }
  return res.json()
}

export async function createOrg(
  superuserKey: string,
  data: { org_name: string; org_slug: string; admin_email: string; admin_name: string },
): Promise<{ org: Org; user: User; api_key: string }> {
  const baseUrl = import.meta.env.VITE_API_URL ?? ''
  const res = await fetch(`${baseUrl}/v1/orgs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${superuserKey}` },
    body: JSON.stringify(data),
  })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }))
    throw Object.assign(new Error(body.error ?? res.statusText), { code: body.code, status: res.status })
  }
  return res.json()
}

export async function loginWithEmail(
  email: string,
  password: string,
): Promise<{ org: Org; user: User }> {
  const baseUrl = import.meta.env.VITE_API_URL ?? ''
  const res = await fetch(`${baseUrl}/v1/admin/auth/login`, {
    method: 'POST',
    credentials: 'include',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }))
    throw Object.assign(new Error(body.error ?? res.statusText), {
      code: body.code,
      status: res.status,
    })
  }
  return res.json()
}

export async function loginWithApiKey(
  apiKey: string,
): Promise<{ org: Org; user: User }> {
  const baseUrl = import.meta.env.VITE_API_URL ?? ''
  const res = await fetch(`${baseUrl}/v1/admin/auth/login`, {
    method: 'POST',
    credentials: 'include',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ api_key: apiKey }),
  })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }))
    throw Object.assign(new Error(body.error ?? res.statusText), {
      code: body.code,
      status: res.status,
    })
  }
  return res.json()
}
