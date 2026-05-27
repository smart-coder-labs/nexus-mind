export interface Org {
  id: string
  name: string
  slug: string
  created_at: string
}

export interface OrgWithStats extends Org {
  user_count: number
  memory_count: number
}

export interface User {
  id: string
  org_id: string
  email: string
  name: string
  role: string
  status: 'active' | 'invited' | 'suspended'
  created_at: string
}

export interface AuditEntry {
  id: string
  org_id: string
  user_id: string
  timestamp: string
  action: string
  resource_type: string
  resource_id: string | null
  metadata: Record<string, unknown>
}

export interface GlobalMetrics {
  total_orgs: number
  total_users: number
  total_memories: number
  active_users_24h: number
}

// What the backend returns when creating an org
export interface CreateOrgResponse {
  org: Org
  user: User
  api_key: string
}

export type ApiError = {
  error: string
  code: string
}
