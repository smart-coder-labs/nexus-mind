import type {
  Org,
  OrgSettings,
  User,
  AuthSession,
  Memory,
  AuditEntry,
  OrgStats,
  MemoryFacets,
  MemoryTrends,
  NameCount,
  MemoryFilters,
  AuditFilters,
  CustomRole,
  Project,
  ProjectMember,
  ProjectAccess,
  Client,
  ClientMember,
  ClientStatus,
  ProjectEventOverrides,
  UpdateProjectEventOverridesRequest,
  CodeProject,
  CodeIndexResponse,
  BulkDeleteResponse,
  BulkTagResponse,
  CodeSearchResult,
  LocateResult,
  CodeGraph,
  CodeSnippet,
  SessionSummary,
  Webhook,
  CreateWebhookRequest,
  UpdateWebhookRequest,
  WebhookDelivery,
  GlobalSearchResult,
  ApiKeyWithUser,
  OnboardingStatus,
  WebhookTestResult,
  UsageStats,
  UsageBucketSize,
  UsageLevel,
  UsageSummaryResponse,
  UsageTimeseriesResponse,
  InviteLinkResponse,
  ImportMemory,
  ImportMemoriesResponse,
  MigrationRun,
  MigrationCandidate,
  MigrationReviewAction,
  MigrationReviewResponse,
  MigrationCommitResponse,
  MigrationRunReport,
  AgentActivity,
  HeatmapDay,
  ContributorStat,
  DashboardData,
  MergeMemoriesRequest,
  NotificationItem,
  ProjectStats,
  Collection,
  AssignCollectionRequest,
  RenameTagResponse,
  RetryDeliveryResponse,
  RetentionPreview,
  ImportConfigResponse,
  Policy,
  CreatePolicyRequest,
  UpdatePolicyRequest,
  Convention,
  CreateConventionRequest,
  UpdateConventionRequest,
  MemoryGraphResponse,
  Backup,
  BackupDetail,
  BackupRestoreSummary,
  Harness,
  CreateHarnessRequest,
  PublishHarnessVersionRequest,
  HarnessVersion,
  HarnessApprovalRequest,
  HarnessApproval,
  HarnessInstallResultRequest,
  HarnessDownloadResponse,
  HarnessRecommendation,
  CreateHarnessConfigReviewRequest,
  CreateHarnessConfigReviewCommentRequest,
  HarnessConfigReview,
  HarnessConfigReviewComment,
  Task,
  TaskComment,
  Sprint,
  ListTasksParams,
  CreateTaskRequest,
  PatchTaskRequest,
  ListSprintsParams,
  CreateSprintRequest,
  SddChange,
  SddArtifact,
  SddArtifactDetail,
  SddRevision,
  SddRevisionMeta,
  SddSearchResult,
  SddSpec,
  SddSpecDetail,
  SddSpecMerge,
  SddSpecRevision,
  SddSpecRevisionMeta,
  ListSddChangesParams,
  ListSddSpecsParams,
  PatchSddChangeRequest,
  LinkSddChangeMemoryRequest,
  AutonomousAgentTemplate,
  AutonomousAgentDefinition,
  AutonomousAgentDetail,
  AutonomousAgentSchedule,
  AutonomousAgentRun,
  AutonomousRuntimeHealth,
  AutonomousAgentConnector,
  AutonomousAgentFinding,
  AutonomousAgentDelivery,
  AutonomousAgentEvent,
  AutonomousAgentTarget,
  AutonomousAgentOrgSettings,
  AutonomousAgentMetrics,
} from '../types'
import { isPublicRoute } from '../auth/public-routes'

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
      // A 401 means the session cookie is gone or expired, so eject to /login.
      // Except on the public routes: there an unauthenticated 401 is the normal
      // state (the login page boots with GET /auth/me to detect an existing
      // session), and redirecting would reload the document into the same 401 —
      // an infinite reload loop.
      if (res.status === 401 && !isPublicRoute(window.location?.pathname)) {
        window.location.replace('/login')
      }
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

  getDashboard(days: 7 | 30 | 90): Promise<DashboardData> {
    return this.request(`/v1/admin/dashboard?days=${days}`)
  }

  getMemoryFacets(): Promise<MemoryFacets> {
    return this.request('/v1/admin/stats/memory-facets')
  }

  getMemoryTrends(days?: number): Promise<MemoryTrends> {
    const qs = days != null ? `?days=${days}` : ''
    return this.request(`/v1/admin/stats/trends${qs}`)
  }

  getTagStats(): Promise<NameCount[]> {
    return this.request('/v1/admin/stats/tags')
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

  updateAnnouncement(announcement: string, announcement_type: string): Promise<OrgSettings> {
    return this.request('/v1/admin/org/announcement', {
      method: 'PATCH',
      body: JSON.stringify({ announcement, announcement_type }),
    })
  }

  updateOrgLogo(logo_url: string | null): Promise<void> {
    return this.request('/v1/admin/org/logo', {
      method: 'PATCH',
      body: JSON.stringify({ logo_url }),
    })
  }

  scheduleMemoryDelete(id: string, delete_after: string | null): Promise<void> {
    return this.request(`/v1/admin/memories/${id}/schedule-delete`, {
      method: 'PATCH',
      body: JSON.stringify({ delete_after }),
    })
  }

  getRetentionPreview(): Promise<RetentionPreview> {
    return this.request('/v1/admin/settings/retention-preview')
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

  resetUserKey(userId: string): Promise<{ new_key: string }> {
    return this.request(`/v1/admin/users/${userId}/reset-key`, { method: 'POST' })
  }

  updateUserRole(userId: string, role: string): Promise<void> {
    return this.request(`/v1/users/${userId}/role`, {
      method: 'PATCH',
      body: JSON.stringify({ role }),
    })
  }

  disableUser(userId: string): Promise<void> {
    return this.request(`/v1/admin/users/${userId}/disable`, { method: 'POST' })
  }

  enableUser(userId: string): Promise<void> {
    return this.request(`/v1/admin/users/${userId}/enable`, { method: 'POST' })
  }

  updateUserNote(userId: string, note: string | null): Promise<void> {
    return this.request(`/v1/admin/users/${userId}/note`, {
      method: 'PATCH',
      body: JSON.stringify({ note }),
    })
  }

  listRoles(): Promise<CustomRole[]> {
    return this.request('/v1/roles')
  }

  getUsersByRole(role: string): Promise<User[]> {
    return this.request(`/v1/admin/users?role=${encodeURIComponent(role)}`)
  }

  assignUserRole(userId: string, role: string): Promise<void> {
    return this.request(`/v1/admin/users/${userId}`, {
      method: 'PATCH',
      body: JSON.stringify({ role }),
    })
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

  updateRole(id: string, permissions: string[]): Promise<void> {
    return this.request(`/v1/roles/${id}`, {
      method: 'PATCH',
      body: JSON.stringify({ permissions }),
    })
  }

  listProjects(params: { include_archived?: boolean; client_id?: string } = {}): Promise<Project[]> {
    const qs = new URLSearchParams()
    if (params.include_archived) qs.set('include_archived', 'true')
    if (params.client_id) qs.set('client_id', params.client_id)
    const q = qs.toString()
    return this.request(`/v1/projects${q ? `?${q}` : ''}`)
  }

  archiveProject(id: string): Promise<void> {
    return this.request(`/v1/projects/${id}/archive`, { method: 'POST' })
  }

  restoreProject(id: string): Promise<void> {
    return this.request(`/v1/projects/${id}/restore`, { method: 'POST' })
  }

  createProject(data: { name: string; description?: string; parent_id?: string; client_id?: string }): Promise<Project> {
    return this.request('/v1/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  updateProject(id: string, data: Partial<{ parent_id: string | null; client_id: string | null; description: string; custom_instructions: string; retention_days: number }>): Promise<void> {
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

  getProjectSettings(projectId: string): Promise<ProjectEventOverrides> {
    return this.request(`/v1/projects/${projectId}/settings`)
  }

  updateProjectSettings(projectId: string, overrides: ProjectEventOverrides): Promise<ProjectEventOverrides> {
    const body: UpdateProjectEventOverridesRequest = { overrides }
    return this.request(`/v1/projects/${projectId}/settings`, {
      method: 'PATCH',
      body: JSON.stringify(body),
    })
  }

  // ── Clients (consultancy grouping) ──────────────────────────────────────────
  //
  // `createClientEntity` is named to avoid colliding with the `createClient()`
  // factory exported at the bottom of this module.

  listClients(includeArchived = false): Promise<Client[]> {
    const qs = includeArchived ? '?include_archived=true' : ''
    return this.request(`/v1/clients${qs}`)
  }

  createClientEntity(data: { name: string; slug: string; status: ClientStatus }): Promise<Client> {
    return this.request('/v1/clients', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  updateClient(id: string, data: { name?: string; status?: ClientStatus }): Promise<Client> {
    return this.request(`/v1/clients/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    })
  }

  archiveClient(id: string): Promise<void> {
    return this.request(`/v1/clients/${id}/archive`, { method: 'POST' })
  }

  deleteClient(id: string): Promise<void> {
    return this.request(`/v1/clients/${id}`, { method: 'DELETE' })
  }

  listClientMembers(id: string): Promise<ClientMember[]> {
    return this.request(`/v1/clients/${id}/members`)
  }

  addClientMember(id: string, userId: string, role: string): Promise<void> {
    return this.request(`/v1/clients/${id}/members`, {
      method: 'POST',
      body: JSON.stringify({ user_id: userId, role }),
    })
  }

  removeClientMember(id: string, userId: string): Promise<void> {
    return this.request(`/v1/clients/${id}/members/${userId}`, { method: 'DELETE' })
  }

  async listMemories(filters: MemoryFilters = {}): Promise<Memory[]> {
    const params = new URLSearchParams()
    Object.entries(filters).forEach(([k, v]) => v != null && params.set(k, String(v)))
    // The endpoint returns a paginated MemoryPage ({ memories, total, ... });
    // unwrap to the array (tolerating a bare array for backward compatibility).
    const res = await this.request<Memory[] | { memories?: Memory[] }>(`/v1/memory?${params}`)
    return Array.isArray(res) ? res : (res?.memories ?? [])
  }

  getMemory(id: string): Promise<Memory> {
    return this.request<Memory>(`/v1/memory/${encodeURIComponent(id)}`)
  }

  async searchMemory(params: { query: string; limit?: number }): Promise<Memory[]> {
    // /v1/memory/search returns a paginated MemoryPage — unwrap to the array.
    // Without this, the caller gets { memories: [...], total, ... } and any
    // .filter() / .map() on the result throws (see admin crash on memory open).
    const res = await this.request<Memory[] | { memories?: Memory[] }>('/v1/memory/search', {
      method: 'POST',
      body: JSON.stringify({ query: params.query, limit: params.limit ?? 5, mode: 'hybrid' }),
    })
    return Array.isArray(res) ? res : (res?.memories ?? [])
  }

  async searchMemories(query: string, limit = 20, mode: 'keyword' | 'hybrid' | 'semantic' = 'hybrid'): Promise<Memory[]> {
    // /v1/memory/search returns a paginated MemoryPage — unwrap to the array.
    const res = await this.request<Memory[] | { memories?: Memory[] }>('/v1/memory/search', {
      method: 'POST',
      body: JSON.stringify({ query, limit, mode }),
    })
    return Array.isArray(res) ? res : (res?.memories ?? [])
  }

  deleteMemory(id: string): Promise<void> {
    return this.request(`/v1/memory/${id}`, { method: 'DELETE' })
  }

  archiveMemory(id: string): Promise<void> {
    return this.request(`/v1/memory/${id}/archive`, { method: 'POST' })
  }

  restoreMemory(id: string): Promise<void> {
    return this.request(`/v1/memory/${id}/restore`, { method: 'POST' })
  }

  pinMemory(id: string): Promise<void> {
    return this.request(`/v1/memory/${id}/pin`, { method: 'POST' })
  }

  unpinMemory(id: string): Promise<void> {
    return this.request(`/v1/memory/${id}/unpin`, { method: 'POST' })
  }

  updateMemory(id: string, content: string): Promise<Memory> {
    return this.request(`/v1/memory/${id}`, {
      method: 'PATCH',
      body: JSON.stringify({ content }),
    })
  }

  updateMemoryNote(id: string, note: string): Promise<Memory> {
    return this.request(`/v1/admin/memories/${id}/note`, {
      method: 'PATCH',
      body: JSON.stringify({ note }),
    })
  }

  listSessions(params: { limit?: number } = {}): Promise<SessionSummary[]> {
    const qs = params.limit !== undefined ? `?limit=${params.limit}` : ''
    return this.request(`/v1/sessions${qs}`)
  }

  createSession(data: { summary?: string; description?: string } = {}): Promise<SessionSummary> {
    return this.request('/v1/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  getSessionMemories(sessionId: string, limit = 50): Promise<import('../types').Memory[]> {
    const qs = new URLSearchParams({ session_id: sessionId, limit: String(limit) })
    return this.request(`/v1/memory?${qs}`)
  }

  updateSession(id: string, data: { summary?: string; description?: string }): Promise<SessionSummary> {
    return this.request(`/v1/sessions/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    })
  }

  deleteSession(id: string): Promise<void> {
    return this.request(`/v1/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' })
  }

  bulkDeleteMemories(ids: string[]): Promise<BulkDeleteResponse> {
    return this.request('/v1/memory/bulk', {
      method: 'DELETE',
      body: JSON.stringify({ ids }),
    })
  }

  updateProfile(data: { name?: string }): Promise<void> {
    return this.request('/v1/users/me', { method: 'PATCH', body: JSON.stringify(data) })
  }

  changePassword(data: { current_password: string; new_password: string }): Promise<{ message: string }> {
    return this.request('/v1/admin/auth/change-password', { method: 'POST', body: JSON.stringify(data) })
  }

  getAuditLog(filters: AuditFilters = {}): Promise<AuditEntry[]> {
    const params = new URLSearchParams()
    Object.entries(filters).forEach(([k, v]) => v != null && params.set(k, String(v)))
    return this.request(`/v1/audit?${params}`)
  }

  listCodeProjects(params: { include_archived?: boolean } = {}): Promise<CodeProject[]> {
    const qs = params.include_archived ? '?include_archived=true' : ''
    return this.request(`/v1/code/projects${qs}`)
  }

  archiveCodeProject(id: string): Promise<void> {
    return this.request(`/v1/code/projects/${id}/archive`, { method: 'POST' })
  }

  restoreCodeProject(id: string): Promise<void> {
    return this.request(`/v1/code/projects/${id}/restore`, { method: 'POST' })
  }

  indexProject(data: { project: string; repo_url?: string; root_path?: string; github_token?: string; graph_only?: boolean }): Promise<CodeIndexResponse> {
    return this.request('/v1/code/index', { method: 'POST', body: JSON.stringify(data) })
  }

  deleteCodeProject(name: string): Promise<void> {
    return this.request(`/v1/code/projects/${encodeURIComponent(name)}`, { method: 'DELETE' })
  }

  updateCodeProjectSchedule(id: string, interval_hours: number | null): Promise<void> {
    return this.request(`/v1/code/projects/${id}/schedule`, {
      method: 'PATCH',
      body: JSON.stringify({ interval_hours }),
    })
  }

  reindexCodeProject(projectId: string): Promise<{ status: string; project_id: string }> {
    return this.request(`/v1/code/projects/${projectId}/reindex`, { method: 'POST' })
  }

  updateCodeProject(id: string, data: { exclude_patterns?: string[] }): Promise<void> {
    return this.request(`/v1/code/projects/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    })
  }

  getCodeProjectFiles(projectId: string): Promise<string[]> {
    return this.request(`/v1/code/projects/${encodeURIComponent(projectId)}/files`)
  }

  searchCode(project: string, query: string, topK = 10, extension?: string): Promise<CodeSearchResult[]> {
    return this.request('/v1/code/search', {
      method: 'POST',
      body: JSON.stringify({ project, query, top_k: topK, extension }),
    })
  }

  /**
   * Paths-only "Locate" — ranked DISTINCT file paths, deduped by file. The
   * token-cheap counterpart to `searchCode`: same auth, same body shape, but the
   * response is `{ results: [...] }` (unwrapped here to the bare array).
   */
  async locateCode(project: string, query: string, limit?: number): Promise<LocateResult[]> {
    const res = await this.request<{ results?: LocateResult[] } | LocateResult[]>('/v1/code/locate', {
      method: 'POST',
      body: JSON.stringify({ project, query, limit }),
    })
    return Array.isArray(res) ? res : (res?.results ?? [])
  }

  getCodeGraph(
    project: string,
    opts: { node_type?: string; edge_type?: string; limit?: number; offset?: number } = {},
  ): Promise<CodeGraph> {
    const qs = new URLSearchParams({ project })
    if (opts.node_type) qs.set('node_type', opts.node_type)
    if (opts.edge_type) qs.set('edge_type', opts.edge_type)
    if (opts.limit != null) qs.set('limit', String(opts.limit))
    if (opts.offset != null) qs.set('offset', String(opts.offset))
    return this.request(`/v1/code/graph?${qs}`)
  }

  getMemoryGraph(
    project: string,
    opts: { since?: string; limit?: number; offset?: number } = {},
  ): Promise<MemoryGraphResponse> {
    const qs = new URLSearchParams({ project })
    if (opts.since) qs.set('since', opts.since)
    if (opts.limit != null) qs.set('limit', String(opts.limit))
    if (opts.offset != null) qs.set('offset', String(opts.offset))
    return this.request<MemoryGraphResponse>(`/v1/memory/graph?${qs}`)
  }

  /**
   * Fetches the memory knowledge graph for a project family — the given
   * root project plus every descendant in `parent_id`. The backend resolves
   * the family server-side, merges per-project graphs, and returns the
   * `projects` array with stable per-project colors for the legend.
   */
  getMemoryGraphForFamily(
    projectId: string,
    opts: { since?: string; limit?: number } = {},
  ): Promise<MemoryGraphResponse> {
    const qs = new URLSearchParams({ project_id: projectId })
    if (opts.since) qs.set('since', opts.since)
    if (opts.limit != null) qs.set('limit', String(opts.limit))
    return this.request<MemoryGraphResponse>(`/v1/memory/graph?${qs}`)
  }

  getCodeSnippet(project: string, file: string, start?: number, end?: number): Promise<CodeSnippet> {
    const qs = new URLSearchParams({ project, file })
    if (start != null) qs.set('start', String(start))
    if (end != null) qs.set('end', String(end))
    return this.request(`/v1/code/snippet?${qs}`)
  }

  listWebhooks(): Promise<{ webhooks: Webhook[] }> {
    return this.request('/v1/webhooks')
  }

  createWebhook(data: CreateWebhookRequest): Promise<Webhook> {
    return this.request('/v1/webhooks', { method: 'POST', body: JSON.stringify(data) })
  }

  updateWebhook(id: string, data: UpdateWebhookRequest): Promise<Webhook> {
    return this.request(`/v1/webhooks/${id}`, { method: 'PATCH', body: JSON.stringify(data) })
  }

  deleteWebhook(id: string): Promise<void> {
    return this.request(`/v1/webhooks/${id}`, { method: 'DELETE' })
  }

  testWebhook(id: string): Promise<WebhookTestResult> {
    return this.request(`/v1/webhooks/${id}/test`, { method: 'POST' })
  }

  listWebhookDeliveries(webhookId: string, limit = 20): Promise<{ deliveries: WebhookDelivery[] }> {
    return this.request(`/v1/webhooks/${webhookId}/deliveries?limit=${limit}`)
  }

  listOrgKeys(): Promise<ApiKeyWithUser[]> {
    return this.request('/v1/admin/keys')
  }

  revokeOrgKey(keyId: string): Promise<void> {
    return this.request(`/v1/admin/keys/${keyId}`, { method: 'DELETE' })
  }

  createOrgKey(data: { name: string; expires_at?: string; role?: string; description?: string }): Promise<{ id: string; name: string; key: string; role?: string; expires_at?: string; created_at?: string }> {
    return this.request('/v1/admin/keys', { method: 'POST', body: JSON.stringify(data) })
  }

  getOnboarding(): Promise<OnboardingStatus> {
    return this.request('/v1/admin/onboarding')
  }

  getDuplicates(): Promise<Memory[][]> {
    return this.request('/v1/admin/stats/duplicates')
  }

  getUsageStats(): Promise<UsageStats> {
    return this.request('/v1/admin/stats/usage')
  }

  // ── Usage metrics (tokens + execution time) ─────────────────────────────────
  //
  // Privileged read (admin/super_user); the backend scopes rows to the caller's
  // visible projects. `runUsageBackfill` is super_user-only server-side.

  // ── Knowledge migration ────────────────────────────────────────────────────

  listMigrationRuns(params?: { client_id?: string; limit?: number }): Promise<MigrationRun[]> {
    const qs = new URLSearchParams()
    if (params?.client_id) qs.set('client_id', params.client_id)
    if (params?.limit) qs.set('limit', String(params.limit))
    const suffix = qs.toString() ? `?${qs}` : ''
    return this.request<MigrationRun[]>(`/v1/migrations${suffix}`)
  }

  getMigrationRun(id: string): Promise<MigrationRun> {
    return this.request<MigrationRun>(`/v1/migrations/${id}`)
  }

  listMigrationCandidates(
    id: string,
    params?: { status?: string; destination_kind?: string; limit?: number },
  ): Promise<MigrationCandidate[]> {
    const qs = new URLSearchParams()
    if (params?.status) qs.set('status', params.status)
    if (params?.destination_kind) qs.set('destination_kind', params.destination_kind)
    if (params?.limit) qs.set('limit', String(params.limit))
    const suffix = qs.toString() ? `?${qs}` : ''
    return this.request<MigrationCandidate[]>(`/v1/migrations/${id}/candidates${suffix}`)
  }

  reviewMigrationCandidates(
    id: string,
    actions: MigrationReviewAction[],
  ): Promise<MigrationReviewResponse> {
    return this.request<MigrationReviewResponse>(`/v1/migrations/${id}/review`, {
      method: 'POST',
      body: JSON.stringify({ actions }),
    })
  }

  commitMigrationRun(id: string): Promise<MigrationCommitResponse> {
    return this.request<MigrationCommitResponse>(`/v1/migrations/${id}/commit`, { method: 'POST' })
  }

  cancelMigrationRun(id: string): Promise<{ cancelled: number }> {
    return this.request<{ cancelled: number }>(`/v1/migrations/${id}/cancel`, { method: 'POST' })
  }

  getMigrationReport(id: string): Promise<MigrationRunReport> {
    return this.request<MigrationRunReport>(`/v1/migrations/${id}/report`)
  }

  getUsageSummary(params: {
    level: UsageLevel
    from?: string
    to?: string
    client_id?: string
    project_id?: string
  }): Promise<UsageSummaryResponse> {
    const qs = new URLSearchParams({ level: params.level })
    if (params.from) qs.set('from', params.from)
    if (params.to) qs.set('to', params.to)
    if (params.client_id) qs.set('client_id', params.client_id)
    if (params.project_id) qs.set('project_id', params.project_id)
    return this.request<UsageSummaryResponse>(`/v1/usage/summary?${qs}`)
  }

  getUsageTimeseries(params: {
    bucket: UsageBucketSize
    from?: string
    to?: string
    client_id?: string
    project_id?: string
  }): Promise<UsageTimeseriesResponse> {
    const qs = new URLSearchParams({ bucket: params.bucket })
    if (params.from) qs.set('from', params.from)
    if (params.to) qs.set('to', params.to)
    if (params.client_id) qs.set('client_id', params.client_id)
    if (params.project_id) qs.set('project_id', params.project_id)
    return this.request<UsageTimeseriesResponse>(`/v1/usage/timeseries?${qs}`)
  }

  runUsageBackfill(): Promise<{ inserted: number }> {
    return this.request('/v1/usage/backfill', { method: 'POST' })
  }

  createInviteLink(role = 'user'): Promise<InviteLinkResponse> {
    return this.request('/v1/admin/invites', {
      method: 'POST',
      body: JSON.stringify({ role }),
    })
  }

  validateInvite(token: string): Promise<{ valid: boolean; role?: string; org_id?: string; org_name?: string; reason?: string }> {
    return this.request(`/v1/invites/${token}`)
  }

  redeemInvite(token: string, name: string, password: string): Promise<{ api_key: string }> {
    return this.request(`/v1/invites/${token}/redeem`, {
      method: 'POST',
      body: JSON.stringify({ name, password }),
    })
  }

  globalSearch(q: string, limit = 10): Promise<GlobalSearchResult> {
    const params = new URLSearchParams({ q, limit: String(limit) })
    return this.request(`/v1/search?${params}`)
  }

  importMemories(memories: ImportMemory[]): Promise<ImportMemoriesResponse> {
    return this.request('/v1/admin/memories/import', {
      method: 'POST',
      body: JSON.stringify({ memories }),
    })
  }

  getAgentActivity(days?: number): Promise<AgentActivity[]> {
    const qs = days != null ? `?days=${days}` : ''
    return this.request(`/v1/admin/stats/agent-activity${qs}`)
  }

  getMemoryHeatmap(days?: number): Promise<HeatmapDay[]> {
    const qs = days != null ? `?days=${days}` : ''
    return this.request(`/v1/admin/stats/memory-heatmap${qs}`)
  }

  getTopContributors(days?: number): Promise<ContributorStat[]> {
    const qs = days != null ? `?days=${days}` : ''
    return this.request(`/v1/admin/stats/top-contributors${qs}`)
  }

  mergeMemories(keepId: string, mergeId: string): Promise<Memory> {
    const body: MergeMemoriesRequest = { keep_id: keepId, merge_id: mergeId }
    return this.request('/v1/admin/memories/merge', {
      method: 'POST',
      body: JSON.stringify(body),
    })
  }

  bulkTagMemories(ids: string[], action: 'add' | 'remove', tag: string): Promise<BulkTagResponse> {
    return this.request('/v1/admin/memories/bulk-tag', {
      method: 'POST',
      body: JSON.stringify({ ids, action, tag }),
    })
  }

  getNotifications(limit = 15): Promise<NotificationItem[]> {
    return this.request(`/v1/admin/notifications?limit=${limit}`)
  }

  getProjectStats(projectId: string): Promise<ProjectStats> {
    return this.request(`/v1/projects/${projectId}/stats`)
  }

  importOrgConfig(data: object): Promise<ImportConfigResponse> {
    return this.request('/v1/admin/import', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  async exportOrgConfig(): Promise<Blob> {
    const res = await fetch(`${this.baseUrl}/v1/admin/export`, {
      credentials: 'include',
    })
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText }))
      throw Object.assign(new Error(body.error ?? res.statusText), {
        code: body.code,
        status: res.status,
      })
    }
    return res.blob()
  }

  async exportMemories(params: {
    q?: string
    tags?: string
    collection_id?: string
  } = {}): Promise<Blob> {
    const qs = new URLSearchParams()
    if (params.q)             qs.set('q',             params.q)
    if (params.tags)          qs.set('tags',          params.tags)
    if (params.collection_id) qs.set('collection_id', params.collection_id)
    const res = await fetch(`${this.baseUrl}/v1/memory/export?${qs}`, {
      credentials: 'include',
    })
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText }))
      throw Object.assign(new Error(body.error ?? res.statusText), {
        code: body.code,
        status: res.status,
      })
    }
    return res.blob()
  }

  async exportAuditLog(params: {
    user_id?: string
    action?: string
    resource_type?: string
    from?: string
    to?: string
    search?: string
  } = {}): Promise<Blob> {
    const qs = new URLSearchParams()
    Object.entries(params).forEach(([k, v]) => v != null && v !== '' && qs.set(k, String(v)))
    const res = await fetch(`${this.baseUrl}/v1/audit/export?${qs}`, {
      credentials: 'include',
    })
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText }))
      throw Object.assign(new Error(body.error ?? res.statusText), {
        code: body.code,
        status: res.status,
      })
    }
    return res.blob()
  }

  // ── Collections ─────────────────────────────────────────────────────────────

  async listCollections(): Promise<Collection[]> {
    return this.request<Collection[]>('/v1/admin/collections')
  }

  async createCollection(data: { name: string; description?: string }): Promise<Collection> {
    return this.request<Collection>('/v1/admin/collections', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    })
  }

  async deleteCollection(id: string): Promise<void> {
    await this.request<void>(`/v1/admin/collections/${id}`, { method: 'DELETE' })
  }

  async assignMemoryToCollection(memoryId: string, req: AssignCollectionRequest): Promise<void> {
    await this.request<void>(`/v1/memories/${memoryId}/collection`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    })
  }

  renameTag(from: string, to: string): Promise<RenameTagResponse> {
    return this.request('/v1/admin/tags/rename', {
      method: 'POST',
      body: JSON.stringify({ from, to }),
    })
  }

  mergeTag(source: string, target: string): Promise<RenameTagResponse> {
    // No dedicated merge endpoint — rename absorbs all memories from source into target
    return this.request('/v1/admin/tags/rename', {
      method: 'POST',
      body: JSON.stringify({ from: source, to: target }),
    })
  }

  retryWebhookDelivery(deliveryId: string): Promise<RetryDeliveryResponse> {
    return this.request(`/v1/webhooks/deliveries/${deliveryId}/retry`, {
      method: 'POST',
    })
  }

  // ── Policies ─────────────────────────────────────────────────────────────────

  listPolicies(): Promise<{ policies: Policy[] }> {
    return this.request('/v1/policies')
  }

  createPolicy(data: CreatePolicyRequest): Promise<Policy> {
    return this.request('/v1/policies', { method: 'POST', body: JSON.stringify(data) })
  }

  updatePolicy(id: string, data: UpdatePolicyRequest): Promise<Policy> {
    return this.request(`/v1/policies/${id}`, { method: 'PATCH', body: JSON.stringify(data) })
  }

  deletePolicy(id: string): Promise<void> {
    return this.request(`/v1/policies/${id}`, { method: 'DELETE' })
  }

  checkPolicy(data: { model: string; prompt_tokens?: number }): Promise<{ allowed: boolean; violations: Array<{ rule_type: string; reason: string }> }> {
    return this.request('/v1/policy/check', { method: 'POST', body: JSON.stringify(data) })
  }

  // Conventions
  listConventions(category?: string, includeArchived?: boolean): Promise<Convention[]> {
    const params = new URLSearchParams()
    if (category) params.set('category', category)
    if (includeArchived) params.set('include_archived', 'true')
    const qs = params.toString() ? `?${params.toString()}` : ''
    return this.request(`/v1/conventions${qs}`)
  }

  getConvention(id: number): Promise<Convention> {
    return this.request(`/v1/conventions/${id}`)
  }

  createConvention(data: CreateConventionRequest): Promise<Convention> {
    return this.request('/v1/conventions', { method: 'POST', body: JSON.stringify(data) })
  }

  updateConvention(id: number, data: UpdateConventionRequest): Promise<Convention> {
    return this.request(`/v1/conventions/${id}`, { method: 'PATCH', body: JSON.stringify(data) })
  }

  deleteConvention(id: number): Promise<void> {
    return this.request(`/v1/conventions/${id}`, { method: 'DELETE' })
  }

  storeMemory(data: { content: string; tags?: string[]; project_id?: string; metadata?: object }): Promise<Memory> {
    return this.request('/v1/memory/store', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  getMemoryHealth(): Promise<{
    total_memories: number
    duplicate_count: number
    stale_count: number
    untagged_count: number
  }> {
    return this.request('/v1/admin/memories/health')
  }

  archiveConvention(id: number): Promise<void> {
    return this.request(`/v1/conventions/${id}/archive`, { method: 'POST' })
  }

  restoreConvention(id: number): Promise<void> {
    return this.request(`/v1/conventions/${id}/restore`, { method: 'POST' })
  }

  // ── Postgres backups (admin) ────────────────────────────────────────────────

  listBackups(): Promise<Backup[]> {
    return this.request<Backup[]>('/v1/backups')
  }

  getBackup(id: string): Promise<BackupDetail> {
    return this.request<BackupDetail>(`/v1/backups/${encodeURIComponent(id)}`)
  }

  createBackup(): Promise<Backup> {
    return this.request<Backup>('/v1/backups', { method: 'POST' })
  }

  restoreBackup(id: string): Promise<BackupRestoreSummary> {
    return this.request<BackupRestoreSummary>(
      `/v1/backups/${encodeURIComponent(id)}/restore?confirm=true`,
      { method: 'POST' },
    )
  }

  async downloadBackup(id: string): Promise<Blob> {
    const res = await fetch(`${this.baseUrl}/v1/backups/${encodeURIComponent(id)}/download`, {
      credentials: 'include',
    })
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText }))
      throw Object.assign(new Error(body.error ?? res.statusText), {
        code: body.code,
        status: res.status,
      })
    }
    return res.blob()
  }

  // ── Harness library (admin) ────────────────────────────────────────────────

  listHarnesses(params: { target?: string; owner_user_id?: string } = {}): Promise<Harness[]> {
    const qs = new URLSearchParams()
    if (params.target) qs.set('target', params.target)
    if (params.owner_user_id) qs.set('owner_user_id', params.owner_user_id)
    return this.request<Harness[]>(`/v1/harnesses${qs.toString() ? `?${qs}` : ''}`)
  }

  createHarness(data: CreateHarnessRequest): Promise<Harness> {
    return this.request<Harness>('/v1/harnesses', { method: 'POST', body: JSON.stringify(data) })
  }

  archiveHarness(harnessId: string): Promise<Harness> {
    return this.request<Harness>(`/v1/harnesses/${encodeURIComponent(harnessId)}/archive`, { method: 'POST' })
  }

  getHarnessVersion(harnessId: string, version: string): Promise<HarnessVersion> {
    return this.request<HarnessVersion>(`/v1/harnesses/${encodeURIComponent(harnessId)}/versions/${encodeURIComponent(version)}`)
  }

  publishHarnessVersion(harnessId: string, data: PublishHarnessVersionRequest): Promise<HarnessVersion> {
    return this.request<HarnessVersion>(`/v1/harnesses/${encodeURIComponent(harnessId)}/versions`, {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  approveHarnessInstall(harnessId: string, version: string, data: HarnessApprovalRequest): Promise<HarnessApproval> {
    return this.request<HarnessApproval>(
      `/v1/harnesses/${encodeURIComponent(harnessId)}/versions/${encodeURIComponent(version)}/approval`,
      { method: 'POST', body: JSON.stringify(data) },
    )
  }

  recordHarnessInstallResult(harnessId: string, version: string, data: HarnessInstallResultRequest): Promise<HarnessApproval> {
    return this.request<HarnessApproval>(
      `/v1/harnesses/${encodeURIComponent(harnessId)}/versions/${encodeURIComponent(version)}/install-result`,
      { method: 'POST', body: JSON.stringify(data) },
    )
  }

  downloadHarnessVersion(harnessId: string, version: string): Promise<HarnessDownloadResponse> {
    return this.request<HarnessDownloadResponse>(
      `/v1/harnesses/${encodeURIComponent(harnessId)}/versions/${encodeURIComponent(version)}/download`,
    )
  }

  listHarnessRecommendations(params: { target?: string } = {}): Promise<HarnessRecommendation[]> {
    const qs = new URLSearchParams()
    if (params.target) qs.set('target', params.target)
    return this.request<HarnessRecommendation[]>(`/v1/harness-recommendations${qs.toString() ? `?${qs}` : ''}`)
  }

  createHarnessConfigReview(data: CreateHarnessConfigReviewRequest): Promise<HarnessConfigReview> {
    return this.request<HarnessConfigReview>('/v1/harness-config-reviews', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  getHarnessConfigReview(id: string): Promise<HarnessConfigReview> {
    return this.request<HarnessConfigReview>(`/v1/harness-config-reviews/${encodeURIComponent(id)}`)
  }

  listHarnessConfigReviews(params: { status?: string } = {}): Promise<HarnessConfigReview[]> {
    const qs = new URLSearchParams()
    if (params.status) qs.set('status', params.status)
    return this.request<HarnessConfigReview[]>(`/v1/harness-config-reviews${qs.toString() ? `?${qs}` : ''}`)
  }

  listHarnessConfigReviewComments(reviewId: string): Promise<HarnessConfigReviewComment[]> {
    return this.request<HarnessConfigReviewComment[]>(`/v1/harness-config-reviews/${encodeURIComponent(reviewId)}/comments`)
  }

  createHarnessConfigReviewComment(reviewId: string, data: CreateHarnessConfigReviewCommentRequest): Promise<HarnessConfigReviewComment> {
    return this.request<HarnessConfigReviewComment>(`/v1/harness-config-reviews/${encodeURIComponent(reviewId)}/comments`, {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  // ── Tasks ────────────────────────────────────────────────────────────────────

  listTasks(params: ListTasksParams = {}): Promise<Task[]> {
    const qs = new URLSearchParams()
    Object.entries(params).forEach(([k, v]) => v != null && qs.set(k, String(v)))
    const q = qs.toString()
    return this.request<Task[]>(`/v1/tasks${q ? `?${q}` : ''}`)
  }

  getTask(id: string): Promise<Task> {
    return this.request<Task>(`/v1/tasks/${encodeURIComponent(id)}`)
  }

  createTask(data: CreateTaskRequest): Promise<Task> {
    return this.request<Task>('/v1/tasks', { method: 'POST', body: JSON.stringify(data) })
  }

  updateTask(id: string, data: PatchTaskRequest): Promise<Task> {
    return this.request<Task>(`/v1/tasks/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    })
  }

  deleteTask(id: string): Promise<void> {
    return this.request(`/v1/tasks/${encodeURIComponent(id)}`, { method: 'DELETE' })
  }

  assignTask(id: string, userIds: string[]): Promise<import('../types').TaskAssignee[]> {
    return this.request(`/v1/tasks/${encodeURIComponent(id)}/assignees`, {
      method: 'POST',
      body: JSON.stringify({ user_ids: userIds }),
    })
  }

  unassignTask(id: string, userId: string): Promise<void> {
    return this.request(`/v1/tasks/${encodeURIComponent(id)}/assignees/${encodeURIComponent(userId)}`, {
      method: 'DELETE',
    })
  }

  listTaskComments(id: string): Promise<TaskComment[]> {
    return this.request<TaskComment[]>(`/v1/tasks/${encodeURIComponent(id)}/comments`)
  }

  addTaskComment(id: string, body: string): Promise<TaskComment> {
    return this.request<TaskComment>(`/v1/tasks/${encodeURIComponent(id)}/comments`, {
      method: 'POST',
      body: JSON.stringify({ body }),
    })
  }

  listTaskSubtasks(id: string): Promise<Task[]> {
    return this.request<Task[]>(`/v1/tasks/${encodeURIComponent(id)}/subtasks`)
  }

  addTaskLabel(id: string, label: string): Promise<string[]> {
    return this.request<string[]>(`/v1/tasks/${encodeURIComponent(id)}/labels`, {
      method: 'POST',
      body: JSON.stringify({ label }),
    })
  }

  removeTaskLabel(id: string, label: string): Promise<void> {
    return this.request(`/v1/tasks/${encodeURIComponent(id)}/labels/${encodeURIComponent(label)}`, {
      method: 'DELETE',
    })
  }

  listTaskSpecLinks(id: string): Promise<string[]> {
    return this.request<string[]>(`/v1/tasks/${encodeURIComponent(id)}/spec-links`)
  }

  linkTaskSpec(id: string, specChangeName: string): Promise<void> {
    return this.request(`/v1/tasks/${encodeURIComponent(id)}/spec-links`, {
      method: 'POST',
      body: JSON.stringify({ spec_change_name: specChangeName }),
    })
  }

  unlinkTaskSpec(id: string, specChangeName: string): Promise<void> {
    return this.request(`/v1/tasks/${encodeURIComponent(id)}/spec-links/${encodeURIComponent(specChangeName)}`, {
      method: 'DELETE',
    })
  }

  listSprints(params: ListSprintsParams = {}): Promise<Sprint[]> {
    const qs = new URLSearchParams()
    Object.entries(params).forEach(([k, v]) => v != null && qs.set(k, String(v)))
    const q = qs.toString()
    return this.request<Sprint[]>(`/v1/sprints${q ? `?${q}` : ''}`)
  }

  createSprint(data: CreateSprintRequest): Promise<Sprint> {
    return this.request<Sprint>('/v1/sprints', { method: 'POST', body: JSON.stringify(data) })
  }

  // ── SDD Artifacts ────────────────────────────────────────────────────────────
  //
  // Read-only over artifact CONTENT (design.md A7). There is deliberately no
  // artifact-save method on this client at all — artifacts are authored by the
  // harness and by git, never by the admin, so the capability must not exist
  // here for any code path to reach for. The three writes below touch change
  // metadata and memory links only — curation, not authorship.

  listSddChanges(params: ListSddChangesParams = {}): Promise<SddChange[]> {
    const qs = new URLSearchParams()
    Object.entries(params).forEach(([k, v]) => v != null && qs.set(k, String(v)))
    const q = qs.toString()
    return this.request<SddChange[]>(`/v1/sdd/changes${q ? `?${q}` : ''}`)
  }

  /** Hydrated read: `artifacts[]` + `task_links[]` + `memory_links[]`. */
  getSddChange(id: string): Promise<SddChange> {
    return this.request<SddChange>(`/v1/sdd/changes/${encodeURIComponent(id)}`)
  }

  getSddChangeArtifacts(id: string): Promise<SddArtifact[]> {
    return this.request<SddArtifact[]>(`/v1/sdd/changes/${encodeURIComponent(id)}/artifacts`)
  }

  getSddChangeTasks(id: string): Promise<Task[]> {
    return this.request<Task[]>(`/v1/sdd/changes/${encodeURIComponent(id)}/tasks`)
  }

  /** Returns the artifact's fields INLINE plus `content` — the response is
   *  serde-flattened, so there is no `.artifact` wrapper to reach through. */
  getSddArtifact(id: string): Promise<SddArtifactDetail> {
    return this.request<SddArtifactDetail>(`/v1/sdd/artifacts/${encodeURIComponent(id)}`)
  }

  /** Metadata only — the revision list never carries content. */
  listSddArtifactRevisions(id: string): Promise<SddRevisionMeta[]> {
    return this.request<SddRevisionMeta[]>(`/v1/sdd/artifacts/${encodeURIComponent(id)}/revisions`)
  }

  getSddArtifactRevision(id: string, rev: number): Promise<SddRevision> {
    return this.request<SddRevision>(
      `/v1/sdd/artifacts/${encodeURIComponent(id)}/revisions/${rev}`,
    )
  }

  /** Spans BOTH openspec trees. Each hit's `hit_type` says whether it came from the
   *  living specification or from a draft inside a change. */
  searchSdd(q: string, limit = 20): Promise<SddSearchResult[]> {
    const qs = new URLSearchParams({ q, limit: String(limit) })
    return this.request<SddSearchResult[]>(`/v1/sdd/search?${qs.toString()}`)
  }

  // ── SDD Specs — the living specification ──────────────────────────────────
  //
  // `openspec/specs/{capability}/spec.md`. Read-only over CONTENT here, exactly like
  // artifacts (A7): the contract is authored by the harness and by git, and there is
  // deliberately no spec-save method on this client for any code path to reach for.

  /** Metadata only — the list never carries a contract's text. */
  listSddSpecs(params: ListSddSpecsParams = {}): Promise<SddSpec[]> {
    const qs = new URLSearchParams()
    Object.entries(params).forEach(([k, v]) => v != null && qs.set(k, String(v)))
    const q = qs.toString()
    return this.request<SddSpec[]>(`/v1/sdd/specs${q ? `?${q}` : ''}`)
  }

  /** The spec's fields INLINE plus `content` — flattened, so there is no wrapper. */
  getSddSpec(id: string): Promise<SddSpecDetail> {
    return this.request<SddSpecDetail>(`/v1/sdd/specs/${encodeURIComponent(id)}`)
  }

  /** Metadata only — the revision list never carries content. */
  listSddSpecRevisions(id: string): Promise<SddSpecRevisionMeta[]> {
    return this.request<SddSpecRevisionMeta[]>(`/v1/sdd/specs/${encodeURIComponent(id)}/revisions`)
  }

  getSddSpecRevision(id: string, rev: number): Promise<SddSpecRevision> {
    return this.request<SddSpecRevision>(`/v1/sdd/specs/${encodeURIComponent(id)}/revisions/${rev}`)
  }

  /** Which living specifications this change has merged its deltas into. */
  getSddChangeSpecs(id: string): Promise<SddSpecMerge[]> {
    return this.request<SddSpecMerge[]>(`/v1/sdd/changes/${encodeURIComponent(id)}/specs`)
  }

  /** Curation (A7). Accepts ONLY title/status/phase/sprint_id — the backend
   *  declares `deny_unknown_fields`, so a stray `project` or `name` is a 422. */
  patchSddChange(id: string, data: PatchSddChangeRequest): Promise<SddChange> {
    return this.request<SddChange>(`/v1/sdd/changes/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    })
  }

  /** Curation (A7). Returns the change's full memory link list. */
  linkSddChangeMemory(id: string, data: LinkSddChangeMemoryRequest): Promise<Memory[]> {
    return this.request<Memory[]>(`/v1/sdd/changes/${encodeURIComponent(id)}/memories`, {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  /** Curation (A7). */
  unlinkSddChangeMemory(id: string, memoryId: string): Promise<void> {
    return this.request(
      `/v1/sdd/changes/${encodeURIComponent(id)}/memories/${encodeURIComponent(memoryId)}`,
      { method: 'DELETE' },
    )
  }

  listAutonomousAgentTemplates(): Promise<AutonomousAgentTemplate[]> { return this.request('/v1/autonomous-agents/templates') }
  listAutonomousAgents(): Promise<AutonomousAgentDefinition[]> { return this.request('/v1/autonomous-agents') }
  getAutonomousAgent(id:string): Promise<AutonomousAgentDetail> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}`) }
  createAutonomousAgent(data:{name:string;description?:string;template_key:string;config:Record<string,unknown>;budgets:Record<string,unknown>}): Promise<AutonomousAgentDetail> { return this.request('/v1/autonomous-agents',{method:'POST',body:JSON.stringify(data)}) }
  updateAutonomousAgent(id:string,data:{name?:string;description?:string;config?:Record<string,unknown>;budgets?:Record<string,unknown>}): Promise<AutonomousAgentDetail> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}`,{method:'PATCH',body:JSON.stringify(data)}) }
  validateAutonomousAgent(id:string): Promise<AutonomousAgentDetail> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/validate`,{method:'POST'}) }
  enableAutonomousAgent(id:string): Promise<AutonomousAgentDetail> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/enable`,{method:'POST'}) }
  disableAutonomousAgent(id:string): Promise<AutonomousAgentDetail> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/disable`,{method:'POST'}) }
  archiveAutonomousAgent(id:string): Promise<AutonomousAgentDetail> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/archive`,{method:'POST'}) }
  runAutonomousAgent(id:string, body?:{targets:Array<{repository:string;type:'pr'|'issue';number:number}>}): Promise<AutonomousAgentRun> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/run`,{method:'POST', ...(body ? {body:JSON.stringify(body)} : {})}) }
  getAutonomousAgentSchedule(id:string): Promise<AutonomousAgentSchedule> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/schedule`) }
  putAutonomousAgentSchedule(id:string,data:{kind:string;expression?:string;timezone:string;misfire_policy:string;enabled:boolean}): Promise<AutonomousAgentSchedule> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/schedule`,{method:'PUT',body:JSON.stringify(data)}) }
  listAutonomousAgentRuns(definitionId?:string): Promise<AutonomousAgentRun[]> { return this.request(`/v1/autonomous-agent-runs${definitionId?`?definition_id=${encodeURIComponent(definitionId)}`:''}`) }
  getAutonomousRuntimeHealth(): Promise<AutonomousRuntimeHealth> { return this.request('/v1/autonomous-agents/runtime') }
  checkAutonomousRuntimeHealth(): Promise<AutonomousRuntimeHealth> { return this.request('/v1/autonomous-agents/runtime',{method:'POST'}) }
  listAutonomousAgentConnectors(): Promise<AutonomousAgentConnector[]> { return this.request('/v1/autonomous-agent-connectors') }
  putAutonomousAgentConnector(data:{kind:string;name:string;secret?:string;metadata:Record<string,unknown>;scopes:string[]}): Promise<AutonomousAgentConnector> { return this.request('/v1/autonomous-agent-connectors',{method:'PUT',body:JSON.stringify(data)}) }
  revokeAutonomousAgentConnector(id:string): Promise<void> { return this.request(`/v1/autonomous-agent-connectors/${encodeURIComponent(id)}`,{method:'DELETE'}) }
  cancelAutonomousAgentRun(id:string): Promise<AutonomousAgentRun> { return this.request(`/v1/autonomous-agent-runs/${encodeURIComponent(id)}/cancel`,{method:'POST'}) }
  listAutonomousAgentRunEvents(id:string): Promise<AutonomousAgentEvent[]> { return this.request(`/v1/autonomous-agent-runs/${encodeURIComponent(id)}/events`) }
  // Turn-by-turn transcript from `after` (exclusive), paged by sequence. Callers
  // poll incrementally with the last sequence they hold so each poll only pulls
  // new turns instead of re-downloading the whole conversation.
  async listAutonomousAgentRunTranscript(id:string, after=0): Promise<AutonomousAgentEvent[]> {
    const all: AutonomousAgentEvent[] = []
    let cursor = after
    for (let page = 0; page < 100; page++) {
      const batch: AutonomousAgentEvent[] = await this.request(`/v1/autonomous-agent-runs/${encodeURIComponent(id)}/transcript?after=${cursor}&limit=5000`)
      all.push(...batch)
      if (batch.length < 5000) break
      cursor = batch[batch.length - 1].sequence
    }
    return all
  }
  listAutonomousAgentFindings(): Promise<AutonomousAgentFinding[]> { return this.request('/v1/autonomous-agent-findings') }
  patchAutonomousAgentFinding(id:string,status:'open'|'resolved'|'ignored'): Promise<AutonomousAgentFinding> { return this.request(`/v1/autonomous-agent-findings/${encodeURIComponent(id)}`,{method:'PATCH',body:JSON.stringify({status})}) }
  listAutonomousAgentDeliveries(): Promise<AutonomousAgentDelivery[]> { return this.request('/v1/autonomous-agent-deliveries') }
  retryAutonomousAgentDelivery(id:string): Promise<AutonomousAgentDelivery> { return this.request(`/v1/autonomous-agent-deliveries/${encodeURIComponent(id)}/retry`,{method:'POST'}) }
  listAutonomousAgentTargets(id:string): Promise<AutonomousAgentTarget[]> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/targets`) }
  putAutonomousAgentTarget(id:string,data:{kind:string;name:string;config:Record<string,unknown>;credential_connector_id?:string;enabled:boolean}): Promise<AutonomousAgentTarget> { return this.request(`/v1/autonomous-agents/${encodeURIComponent(id)}/targets`,{method:'POST',body:JSON.stringify(data)}) }
  getAutonomousAgentSettings(): Promise<AutonomousAgentOrgSettings> { return this.request('/v1/autonomous-agents/settings') }
  patchAutonomousAgentSettings(data:{enabled?:boolean;retention_days?:number}): Promise<AutonomousAgentOrgSettings> { return this.request('/v1/autonomous-agents/settings',{method:'PATCH',body:JSON.stringify(data)}) }
  getAutonomousAgentMetrics(): Promise<AutonomousAgentMetrics> { return this.request('/v1/autonomous-agents/metrics') }
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
