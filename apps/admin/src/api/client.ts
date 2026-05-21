import type {
  Org,
  User,
  Memory,
  AuditEntry,
  OrgStats,
  MemoryFilters,
  AuditFilters,
} from '../types'

export class NexusMindClient {
  constructor(
    private readonly baseUrl: string,
    private readonly apiKey: string,
  ) {}

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${this.apiKey}`,
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
    // 204 No Content
    if (res.status === 204) return undefined as T
    return res.json()
  }

  // Auth (no Bearer needed for this one)
  async validateKey(): Promise<{ org: Org; user: User }> {
    const res = await fetch(`${this.baseUrl}/v1/admin/org`, {
      headers: { Authorization: `Bearer ${this.apiKey}` },
    })
    if (!res.ok) throw new Error('Invalid API key')
    const org: Org = await res.json()
    // Derive a minimal user from org context (role is admin if key validates)
    return {
      org,
      user: {
        id: '',
        org_id: org.id,
        email: '',
        name: 'Admin',
        role: 'admin',
        status: 'active',
        created_at: '',
      },
    }
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

  listUsers(): Promise<User[]> {
    return this.request('/v1/users')
  }

  inviteUser(data: {
    email: string
    name: string
    role: string
  }): Promise<{ user: User; api_key: string }> {
    return this.request('/v1/users/invite', { method: 'POST', body: JSON.stringify(data) })
  }

  removeUser(id: string): Promise<void> {
    return this.request(`/v1/users/${id}`, { method: 'DELETE' })
  }

  rotateKey(userId: string): Promise<{ api_key: string }> {
    return this.request(`/v1/users/${userId}/rotate-key`, { method: 'POST' })
  }

  listMemories(filters: MemoryFilters = {}): Promise<Memory[]> {
    const params = new URLSearchParams()
    Object.entries(filters).forEach(([k, v]) => v != null && params.set(k, String(v)))
    return this.request(`/v1/memory?${params}`)
  }

  searchMemories(query: string, limit = 20): Promise<Memory[]> {
    return this.request('/v1/memory/search', {
      method: 'POST',
      body: JSON.stringify({ query, limit }),
    })
  }

  deleteMemory(id: string): Promise<void> {
    return this.request(`/v1/memory/${id}`, { method: 'DELETE' })
  }

  getAuditLog(filters: AuditFilters = {}): Promise<AuditEntry[]> {
    const params = new URLSearchParams()
    Object.entries(filters).forEach(([k, v]) => v != null && params.set(k, String(v)))
    return this.request(`/v1/audit?${params}`)
  }
}

export function createClient(apiKey: string): NexusMindClient {
  const baseUrl = import.meta.env.VITE_API_URL ?? ''
  return new NexusMindClient(baseUrl, apiKey)
}
