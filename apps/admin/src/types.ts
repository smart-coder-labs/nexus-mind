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
  // permissions derived from the user's role (returned by /me)
  permissions?: string[]
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
  /** Additive facet. Empty — never a 403 — for a caller without `sdd:read`
   *  (design.md A4). An older backend omits the key entirely, so every read site
   *  must default it to `[]`. */
  sdd_changes: SddChangeSummary[]
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

// ── Memory Knowledge Graph (v45+) ─────────────────────────────────────────────

export interface MemGraphNode {
  id: string        // namespaced: "memory:uuid", "project:uuid", "tag:name", etc.
  type: string      // "Memory" | "Project" | "Session" | "User" | "Collection" | "Tag" | "AuditEvent"
  label: string     // display label (truncated content, name, etc.)
}

export interface MemGraphEdge {
  id: string
  from_id: string
  to_id: string
  type: string      // "belongs_to" | "in_session" | "created_by" | "in_collection" | "tagged" | "performed_by" | "targets" | "child_of"
}

/**
 * One project that contributed to a memory-graph response. The backend
 * picks a stable color from its 8-color palette (FNV-1a hash of the id,
 * mod 8) and ships it here so the frontend never has to know the palette.
 */
export interface ProjectGraphInfo {
  id: string
  name: string
  color: string          // CSS hex like "#2997ff"
  parent_id: string | null
}

export interface MemoryGraphResponse {
  project: string
  node_count: number
  edge_count: number
  nodes: MemGraphNode[]
  edges: MemGraphEdge[]
  /**
   * Projects that contributed to this response. For a family-scoped fetch
   * (via `?project_id=...`), this is the resolved family (root + descendants).
   * For a legacy single-project fetch (`?project=name`), it's a one-element
   * array. The frontend uses this to color the legend and to set per-node
   * border colors by the memory's owning project.
   */
  projects: ProjectGraphInfo[]
}

// ── Postgres backups (admin) ──────────────────────────────────────────────────

export type BackupKind = 'manual' | 'scheduled' | string
export type BackupStatus = 'pending' | 'running' | 'completed' | 'failed' | string

export interface Backup {
  id: string
  org_id: string
  created_at: string
  kind: BackupKind
  status: BackupStatus
  size_bytes: number
  metadata?: Record<string, unknown> | null
}

export interface BackupTableInfo {
  table_name: string
  row_count: number
}

export interface BackupDetail extends Backup {
  table_list: BackupTableInfo[]
}

export interface BackupRestoreSummary {
  backup_id: string
  restored_at: string
  tables_restored: number
  rows_restored: number
}

// ── Harness Library (admin) ───────────────────────────────────────────────────

export interface HarnessVersionSummary {
  id: string
  version: string
  manifest_hash: string
  targets: string[]
  format?: HarnessFormat
  warning_metadata?: Record<string, unknown> | null
  status: string
  published_at: string
}

export type HarnessFormat = 'agent' | 'skill' | 'command' | 'hook' | 'output_style' | 'claude_code_plugin' | 'theme'
export type HarnessComponentKind = 'file' | 'folder' | 'plugin_marketplace' | 'theme_json'

export interface HarnessOwner {
  id: string
  name: string
  email: string
}

export interface HarnessManifestEntry {
  kind: 'file'
  path: string
  media_type: string
  size_bytes: number
  sha256: string
  content?: string
}

export interface HarnessManifestComponent {
  kind: HarnessComponentKind
  path: string
  media_type?: string
  size_bytes?: number
  sha256?: string
  content?: string
  entries?: HarnessManifestEntry[]
}

export interface HarnessManifest {
  schema_version: '1.1'
  targets: Array<'claude' | 'codex' | 'cursor'>
  format: HarnessFormat
  components: HarnessManifestComponent[]
  provenance: { source: string }
  security: { requires_approval: true; executable?: boolean; secret_scan_status?: 'passed' }
}

export interface Harness {
  id: string
  org_id: string
  project_id?: string | null
  slug: string
  name: string
  description?: string | null
  visibility: string
  status: string
  created_by: string
  owner_user_id: string
  owner?: HarnessOwner | null
  created_at: string
  updated_at: string
  latest_version?: HarnessVersionSummary | null
}

export interface CreateHarnessRequest {
  slug: string
  name: string
  description?: string
  project_id?: string
  visibility?: string
  owner_user_id?: string
}

export interface PublishHarnessVersionRequest {
  version: string
  manifest: HarnessManifest | Record<string, unknown>
  manifest_hash?: string
}

export interface HarnessVersion {
  id: string
  harness_id: string
  version: string
  manifest: Record<string, unknown>
  manifest_hash: string
  targets: string[]
  format?: HarnessFormat
  provenance: Record<string, unknown>
  status: string
  published_by: string
  published_at: string
  revoked_at?: string | null
}

export interface HarnessApprovalRequest {
  target_tool: string
  target_scope: string
  manifest_hash: string
  metadata?: Record<string, unknown>
}

export interface HarnessInstallResultRequest {
  approval_id: string
  manifest_hash: string
  status: string
  metadata?: Record<string, unknown>
}

export interface HarnessApproval {
  id: string
  org_id: string
  user_id: string
  harness_version_id: string
  target_tool: string
  target_scope: string
  manifest_hash: string
  status: string
  metadata: Record<string, unknown>
  approved_at: string
}

export interface HarnessDownloadResponse {
  harness_id: string
  version: string
  manifest: Record<string, unknown>
  manifest_hash: string
  approval_required: boolean
}

export interface HarnessRecommendation {
  harness_id: string
  version: string
  name: string
  description?: string | null
  targets: string[]
  owner?: HarnessOwner | null
  format?: HarnessFormat
  warning_metadata?: Record<string, unknown> | null
  manifest_hash: string
  approval_required: boolean
  download_url: string
  required_permissions: string[]
}

export interface CreateHarnessConfigReviewRequest {
  source_tool: string
  redacted_config: Record<string, unknown>
  redaction_report: Record<string, unknown>
  content_hash: string
  status?: string
}

export interface HarnessConfigReviewAuthor {
  id: string
  name: string
  email: string
}

export interface HarnessConfigReview {
  id: string
  org_id: string
  user_id: string
  source_tool: string
  redacted_config: Record<string, unknown>
  redaction_report: Record<string, unknown>
  content_hash: string
  status: string
  created_at: string
  shared_at?: string | null
  author?: HarnessConfigReviewAuthor | null
}

export interface HarnessConfigReviewComment {
  id: string
  org_id: string
  review_id: string
  user_id: string
  body: string
  created_at: string
  author?: HarnessConfigReviewAuthor | null
}

export interface CreateHarnessConfigReviewCommentRequest {
  body: string
}

// ── Team Tasks (admin UI) ─────────────────────────────────────────────────────

export type TaskStatus = 'backlog' | 'todo' | 'in_progress' | 'in_review' | 'done' | 'cancelled'
export type TaskPriority = 'low' | 'medium' | 'high' | 'urgent'

export interface TaskAssignee {
  id: string
  name: string
  email: string
}

export interface Task {
  id: string
  org_id: string
  project: string
  title: string
  description?: string | null
  status: TaskStatus
  priority: TaskPriority
  due_date?: string | null
  parent_id?: string | null
  sprint_id?: string | null
  created_by: string
  created_at: string
  updated_at: string
  archived_at?: string | null
  assignees: TaskAssignee[]
  labels: string[]
  comment_count: number
  spec_links: string[]
  subtask_count: number
}

export interface TaskComment {
  id: string
  task_id: string
  user_id: string
  author_name: string
  body: string
  created_at: string
}

export type SprintStatus = 'planned' | 'active' | 'completed'

export interface Sprint {
  id: string
  org_id: string
  project: string
  name: string
  goal?: string | null
  starts_at?: string | null
  ends_at?: string | null
  status: SprintStatus
  created_by: string
  created_at: string
  archived_at?: string | null
  task_count: number
}

export interface ListTasksParams {
  project?: string
  assignee?: string
  status?: TaskStatus
  sprint?: string
  label?: string
  parent_id?: string
  include_archived?: boolean
  limit?: number
  offset?: number
}

export interface CreateTaskRequest {
  project: string
  title: string
  description?: string
  status?: TaskStatus
  priority?: TaskPriority
  due_date?: string
  parent_id?: string
  sprint_id?: string
}

export interface PatchTaskRequest {
  title?: string
  description?: string
  status?: TaskStatus
  priority?: TaskPriority
  due_date?: string
  sprint_id?: string
}

export interface AssignTaskRequest {
  user_ids: string[]
}

export interface AddLabelRequest {
  label: string
}

export interface LinkSpecRequest {
  spec_change_name: string
}

export interface ListSprintsParams {
  project?: string
  status?: SprintStatus
  include_archived?: boolean
  limit?: number
  offset?: number
}

export interface CreateSprintRequest {
  project: string
  name: string
  goal?: string
  starts_at?: string
  ends_at?: string
}

// ── SDD Artifacts ─────────────────────────────────────────────────────────────
//
// Hand-written mirrors of `apps/backend/src/models/types.rs`. Three shapes here
// are load-bearing and easy to get wrong:
//
//  1. `SddArtifactDetail` is `#[serde(flatten)]`-ed on the wire — the artifact's
//     own fields are INLINE alongside `change_name`/`project`/`content`. Modelling
//     it as `{ artifact: SddArtifact, ... }` compiles and silently yields
//     `undefined` content at runtime. Hence `extends SddArtifact`.
//  2. `SddRevisionMeta` has NO `content` field, on purpose — the revision *list*
//     endpoint physically cannot leak a 36 KB document.
//  3. `SddArtifact` itself carries no content either; content is fetched by id.

/** Advisory pipeline marker on a change. The artifact inventory — not this — is
 *  the ground truth for what a change actually has. */
export type SddPhase =
  | 'explore' | 'propose' | 'spec' | 'design' | 'tasks' | 'apply' | 'verify' | 'archive'

export type SddStatus = 'active' | 'archived' | 'abandoned'

export type SddArtifactKind =
  | 'exploration' | 'proposal' | 'spec' | 'design' | 'tasks'
  | 'apply-progress' | 'verify-report' | 'archive-report' | 'state'

/** One artifact file within a change. Carries NO content. */
export interface SddArtifact {
  id: string
  change_id: string
  kind: SddArtifactKind
  /** Empty string for every kind except `spec`. Never null. */
  capability: string
  path?: string | null
  latest_revision: number
  created_at: string
  updated_at: string
}

/** An artifact plus the content of its latest revision. Serde-FLATTENED on the
 *  wire: the `SddArtifact` fields arrive inline, not nested under `artifact`. */
export interface SddArtifactDetail extends SddArtifact {
  change_name: string
  project: string
  content?: string | null
  content_hash?: string | null
}

/** A full, immutable revision — with content. */
export interface SddRevision {
  id: string
  artifact_id: string
  revision: number
  content: string
  content_hash: string
  byte_size: number
  git_commit?: string | null
  git_path?: string | null
  source: string
  created_by: string
  created_at: string
}

/** Revision metadata. No `content` field, deliberately. */
export interface SddRevisionMeta {
  id: string
  artifact_id: string
  revision: number
  content_hash: string
  byte_size: number
  git_commit?: string | null
  git_path?: string | null
  source: string
  created_by: string
  created_at: string
}

/** An SDD change — one `openspec/changes/{name}/` folder. */
export interface SddChange {
  id: string
  org_id: string
  project: string
  name: string
  title?: string | null
  status: SddStatus
  phase: SddPhase
  repo_url?: string | null
  repo_ref?: string | null
  sprint_id?: string | null
  created_by: string
  created_at: string
  updated_at: string
  archived_at?: string | null
  /** Metadata-only inventory. Hydrated on list AND detail reads. */
  artifacts: SddArtifact[]
  /** Hydrated on detail reads only. */
  task_links: Task[]
  /** Hydrated on detail reads only. */
  memory_links: Memory[]
}

/** Thin projection carried by `GlobalSearchResult` — no content, no inventory. */
export interface SddChangeSummary {
  id: string
  project: string
  name: string
  title?: string | null
  phase: SddPhase
  status: SddStatus
}

/** An FTS5 hit — a snippet, never the whole document. */
export interface SddSearchHit {
  artifact_id: string
  change_id: string
  change_name: string
  project: string
  kind: SddArtifactKind
  capability: string
  snippet: string
}

export interface ListSddChangesParams {
  project?: string
  status?: SddStatus
  phase?: SddPhase
  sprint_id?: string
  include_archived?: boolean
}

/** Body for `PATCH /v1/sdd/changes/:id`.
 *
 *  The identity tuple `(project, name)` is deliberately absent: the backend
 *  declares `#[serde(deny_unknown_fields)]`, so sending `project` or `name` is a
 *  422, not a silent no-op. A rename is a delete-and-recreate. */
export interface PatchSddChangeRequest {
  title?: string
  status?: SddStatus
  phase?: SddPhase
  sprint_id?: string | null
}

/** Body for `POST /v1/sdd/changes/:id/memories`. */
export interface LinkSddChangeMemoryRequest {
  memory_id: string
  /** `produced` (default) or `informed`. */
  relation?: string
}
