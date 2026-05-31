export interface Org {
  id: string
  name: string
  slug: string
  created_at: string
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

export interface CustomRole {
  id: string
  org_id: string | null
  name: string
  display_name: string
  description: string | null
  extends: string[]
  permissions: string[]
  color: string | null
  icon: string | null
  version: number
  enabled: boolean
  is_template: boolean
  created_at: string
  updated_at: string
}

export interface AuthSession {
  org: Org
  user: User
}

export interface Memory {
  id: string
  org_id: string
  user_id: string
  project: string
  tool: string
  content: string
  tags: string[]
  created_at: string
  // v2 fields
  title?: string
  type?: string
  scope?: string
  topic_key?: string
  session_id?: string
  revision_count?: number
  normalized_hash?: string
  project_id?: string
}

export interface Project {
  id: string
  org_id: string
  name: string
  description: string | null
  parent_id: string | null
  created_at: string
}

export interface ProjectMember {
  id: string
  project_id: string
  user_id: string
  email: string
  name: string
  role: string
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

export interface OrgStats {
  total_memories: number
  active_users_24h: number
  searches_today: number
  top_tools: { tool: string; count: number }[]
}

export interface ApiError {
  error: string
  code: string
}

export interface MemoryFilters {
  user_id?: string
  tool?: string
  project?: string
  limit?: number
  offset?: number
}

export interface AuditFilters {
  user_id?: string
  action?: string
  resource_type?: string
  from?: string
  to?: string
  limit?: number
  offset?: number
}

export type ProjectAccess =
  | { type: 'all' }
  | { type: 'specific'; project_ids: string[] }
