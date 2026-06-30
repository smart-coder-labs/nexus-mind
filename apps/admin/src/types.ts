export interface Org {
  id: string
  name: string
  slug: string
  created_at: string
}

export interface AgentEventSettings {
  resolve_issues: boolean
  review_prs: boolean
  respond_comments: boolean
  auto_index: boolean
  scanner: boolean
}

export interface OrgSettings {
  events: AgentEventSettings
  retention_days?: number | null
  custom_instructions?: string | null
  min_password_length?: number | null
  announcement?: string | null
  announcement_type?: string | null
  logo_url?: string | null
}

export interface User {
  id: string
  org_id: string
  email: string
  name: string
  role: string
  status: 'active' | 'invited' | 'suspended'
  created_at: string
  last_active?: string | null
  disabled_at?: string | null
  // v32 admin note (admin-only, never returned to agents)
  admin_note?: string | null
  // v33 last login tracking
  last_login_at?: string | null
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
  // v17 soft-archive
  archived_at?: string | null
  // v19 pinning
  pinned?: boolean
  // v25 collections
  collection_id?: string | null
  // v29 admin note (admin-only, never returned to agents)
  admin_note?: string | null
  // v30 scheduled deletion
  delete_after?: string | null
}

export interface Project {
  id: string
  org_id: string
  name: string
  description: string | null
  parent_id: string | null
  created_at: string
  archived_at?: string | null
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
  type?: string
  scope?: string
  session_id?: string
  limit?: number
  offset?: number
  include_archived?: boolean
  /** ISO 8601 date string (e.g. "2025-01-01"). Only memories created on or after this date. */
  from_date?: string
  /** ISO 8601 date string (e.g. "2025-01-31"). Only memories created on or before this date (inclusive). */
  to_date?: string
  /** When set, only memories belonging to this collection are returned. */
  collection_id?: string
}

export interface UsageStats {
  memories: number
  sessions: number
  users: number
  projects: number
  code_repos: number
}

export interface AuditFilters {
  user_id?: string
  action?: string
  resource_type?: string
  resource_id?: string
  from?: string
  to?: string
  search?: string
  limit?: number
  offset?: number
}

export type ProjectAccess =
  | { type: 'all' }
  | { type: 'specific'; project_ids: string[] }

export interface CodeProject {
  id: string
  org_id: string
  name: string
  root_path: string
  repo_url: string | null
  file_count: number
  chunk_count: number
  last_indexed: string | null
  created_at: string
  reindex_interval_hours?: number | null
  last_indexed_at?: string | null
  last_index_error?: string | null
  indexed_files_count?: number | null
  index_status?: string | null
  archived_at?: string | null
  /** v34: glob-like patterns to exclude from indexing */
  exclude_patterns?: string[]
}

export interface RetentionPreview {
  would_delete: number
  retention_days: number | null
}

export interface CodeIndexResponse {
  project: string
  status: string
  file_count: number
  chunk_count: number
  last_indexed: string
}

export interface FacetCount {
  value: string
  count: number
}

export interface MemoryFacets {
  types: FacetCount[]
  scopes: FacetCount[]
  projects: FacetCount[]
}

export interface BulkDeleteResponse {
  deleted: number
}

export interface CodeSearchResult {
  file_path: string
  symbol: string | null
  start_line: number
  end_line: number
  content: string
  score: number
}

export interface SessionSummary {
  id: string
  org_id: string
  project: string
  directory: string
  started_at: string
  ended_at: string | null
  summary: string | null
  memory_count: number
}

export interface Webhook {
  id: string
  org_id: string
  name: string
  target_url: string
  secret: string | null
  events: string[]
  active: boolean
  created_at: string
}

export interface CreateWebhookRequest {
  name: string
  target_url: string
  secret?: string
  events?: string[]
}

export interface UpdateWebhookRequest {
  active?: boolean
  secret?: string
  events?: string[]
}

export interface UserSummary {
  id: string
  email: string
  name: string
  role: string
}

export interface GlobalSearchResult {
  memories: Memory[]
  users: UserSummary[]
  projects: Project[]
  policies: Policy[]
  conventions: Convention[]
}

export interface DailyCount {
  date: string  // "YYYY-MM-DD"
  count: number
}

export interface NameCount {
  name: string
  count: number
}

export interface MemoryTrends {
  daily_counts: DailyCount[]
  by_type: NameCount[]
  by_project: NameCount[]
  total: number
  this_week: number
  this_month: number
}

export interface ProjectEventOverrides {
  resolve_issues?: boolean
  review_prs?: boolean
  respond_comments?: boolean
  auto_index?: boolean
  scanner?: boolean
}

export interface UpdateProjectEventOverridesRequest {
  overrides: ProjectEventOverrides
}

export interface ApiKeyWithUser {
  id: string
  user_id: string
  user_name: string
  user_email: string
  label: string
  last_used: string | null
  created_at: string
  revoked: boolean
  expires_at: string | null
  // v32 usage tracking
  times_used?: number
  last_used_at?: string | null
}

export interface OnboardingItem {
  key: string
  label: string
  description: string
  done: boolean
}

export interface OnboardingStatus {
  items: OnboardingItem[]
}

export interface WebhookTestResult {
  success: boolean
  status_code: number | null
  error: string | null
}

export interface WebhookDelivery {
  id: string
  webhook_id: string
  org_id: string
  event_type: string
  payload: string
  status_code: number | null
  success: boolean
  error: string | null
  delivered_at: string
}

export interface InviteLinkResponse {
  token: string
  invite_url: string
  expires_at: string
  role: string
}

export interface ImportMemory {
  content: string
  project?: string
  scope?: string
  type?: string
  tags?: string[]
  session_id?: string
}

export interface ImportMemoriesResponse {
  imported: number
  skipped: number
  errors: string[]
}

export interface AgentActivity {
  tool: string
  total_memories: number
  memories_last_24h: number
  memories_last_7d: number
  last_seen: string
}

export interface HeatmapDay {
  day: string    // "YYYY-MM-DD"
  count: number
}

export interface ContributorStat {
  user_id: string
  memory_count: number
  last_activity: string
  user_name: string | null
  user_email: string | null
}

export interface MergeMemoriesRequest {
  keep_id: string
  merge_id: string
}

export interface BulkTagRequest {
  ids: string[]
  action: 'add' | 'remove'
  tag: string
}

export interface BulkTagResponse {
  updated: number
}

export interface NotificationItem {
  id: string
  message: string
  action: string
  resource_type: string | null
  created_at: string
  actor: string | null
}

export interface ProjectStats {
  total_memories: number
  memories_this_week: number
  last_memory_at: string | null
  top_tags: string[]
}

// ── Collections ───────────────────────────────────────────────────────────────

export interface Collection {
  id: string
  org_id: string
  name: string
  description?: string | null
  created_at: string
  memory_count?: number | null
}

export interface AssignCollectionRequest {
  collection_id: string | null
}

export interface RenameTagResponse {
  updated_count: number
}

export interface RetryDeliveryResponse {
  delivery_id: string
  status: string
}

export interface ImportConfigResponse {
  applied_fields: string[]
  skipped_fields: string[]
}

// ── Policies ──────────────────────────────────────────────────────────────────

export interface Policy {
  id: string
  org_id: string
  name: string
  rule_type: 'model_whitelist' | 'budget_limit' | 'pii_redact'
  config: Record<string, unknown>
  enabled: boolean
  created_at: string
  updated_at: string
}

export interface CreatePolicyRequest {
  name: string
  rule_type: 'model_whitelist' | 'budget_limit' | 'pii_redact'
  config: Record<string, unknown>
  enabled?: boolean
}

export interface UpdatePolicyRequest {
  name?: string
  enabled?: boolean
}

export interface Convention {
  id: number
  org_id: string
  project_id?: string | null
  title: string
  content: string
  category: string
  weight: number
  tags: string[]
  created_at: string
  updated_at: string
  archived_at?: string | null
}

export interface CreateConventionRequest {
  title: string
  content: string
  category?: string
  weight?: number
  tags?: string[]
  project_id?: string
}

export interface UpdateConventionRequest {
  title?: string
  content?: string
  category?: string
  weight?: number
  tags?: string[]
}

// ── Code Knowledge Graph (v41/v42) ────────────────────────────────────────────

export interface GraphNode {
  id: number
  type: string
  name: string
  qualified_name: string
  file_path: string | null
  start_line?: number | null
  end_line?: number | null
  language?: string | null
}

export interface GraphEdge {
  id: number
  from_id: number
  to_id: number
  type: string
}

export interface CodeGraph {
  project: string
  node_count: number
  edge_count: number
  nodes: GraphNode[]
  edges: GraphEdge[]
}

export interface CodeSnippet {
  file_path: string
  symbol: string | null
  language: string | null
  start_line: number
  end_line: number
  content: string
}
