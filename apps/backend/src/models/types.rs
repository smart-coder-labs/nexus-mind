use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;

// ── Role enum ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
    Viewer,
}

impl Role {
    pub fn as_u8(self) -> u8 {
        match self {
            Role::Admin => 2,
            Role::Member => 1,
            Role::Viewer => 0,
        }
    }
}

impl FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(Role::Admin),
            "member" => Ok(Role::Member),
            "viewer" => Ok(Role::Viewer),
            other => Err(format!("unknown role: {other}")),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Viewer => "viewer",
        };
        write!(f, "{s}")
    }
}

// ── UserRole enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserRole {
    Standard(Role),
    Custom(String),
}

impl UserRole {
    /// Only super_user has organization-wide resource visibility. `admin` remains
    /// privileged for permission checks but is membership-scoped for data reads.
    pub fn is_super_user(&self) -> bool {
        matches!(self, UserRole::Custom(role) if role == "super_user")
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, UserRole::Standard(Role::Admin))
    }

    pub fn is_privileged(&self) -> bool {
        self.is_admin() || self.is_super_user()
    }

    pub fn as_str(&self) -> &str {
        match self {
            UserRole::Standard(r) => match r {
                Role::Admin => "admin",
                Role::Member => "member",
                Role::Viewer => "viewer",
            },
            UserRole::Custom(s) => s.as_str(),
        }
    }
}

impl FromStr for UserRole {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(UserRole::Standard(Role::Admin)),
            "member" => Ok(UserRole::Standard(Role::Member)),
            "viewer" => Ok(UserRole::Standard(Role::Viewer)),
            custom => Ok(UserRole::Custom(custom.to_string())),
        }
    }
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

fn default_scope() -> String {
    "project".to_string()
}

fn default_active_status() -> String {
    "active".to_string()
}

// ── Agent event settings ──────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentOrgSettings {
    pub enabled: bool,
    pub retention_days: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentMetrics {
    pub queued: i64,
    pub running: i64,
    pub blocked: i64,
    pub open_findings: i64,
    pub failed_deliveries: i64,
    pub dead_letters: i64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchAutonomousAgentOrgSettingsRequest {
    pub enabled: Option<bool>,
    pub retention_days: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentEventSettings {
    #[serde(default = "default_true")]
    pub resolve_issues: bool,
    #[serde(default = "default_true")]
    pub review_prs: bool,
    #[serde(default = "default_true")]
    pub respond_comments: bool,
    #[serde(default = "default_true")]
    pub auto_index: bool,
    #[serde(default = "default_true")]
    pub scanner: bool,
}

impl Default for AgentEventSettings {
    fn default() -> Self {
        Self {
            resolve_issues: true,
            review_prs: true,
            respond_comments: true,
            auto_index: true,
            scanner: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct OrgSettings {
    #[serde(default)]
    pub events: AgentEventSettings,
    /// Auto-delete memories older than this many days. NULL = keep forever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<i64>,
    /// System prompt injected into every agent's context for this org. NULL = no custom instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
    /// Minimum password length enforced for this org. NULL = use default (8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_password_length: Option<i64>,
    /// Announcement banner text. Empty string or NULL = no banner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub announcement: Option<String>,
    /// Announcement type: "info" | "warning" | "error". Defaults to "info".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub announcement_type: Option<String>,
    /// URL to the org's logo image. NULL = no logo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
}

/// Request body for `PATCH /v1/admin/org/logo`.
/// None = clear the logo (sets logo_url = NULL).
#[derive(Debug, Deserialize)]
pub struct UpdateOrgLogoRequest {
    pub logo_url: Option<String>,
}

/// Response for `POST /v1/admin/import`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImportConfigResponse {
    pub applied_fields: Vec<String>,
    pub skipped_fields: Vec<String>,
}

/// Response for `GET /v1/admin/settings/retention-preview`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RetentionPreview {
    /// Number of memories that would be deleted given current retention settings.
    pub would_delete: i64,
    /// The current retention_days setting. None means no policy is configured.
    pub retention_days: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Org {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct User {
    pub id: String,
    pub org_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active: Option<String>,
    /// Non-null when the account has been disabled. NULL = active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_at: Option<String>,
    /// Private admin note for this user account. Never returned to non-admin callers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_note: Option<String>,
    /// ISO datetime of the last successful API key authentication for this user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
}

/// Request body for `PATCH /v1/admin/users/:id/note`.
/// None = clear the note (sets admin_note = NULL).
#[derive(Debug, Deserialize)]
pub struct UpdateUserNoteRequest {
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CustomRole {
    pub id: String,
    pub org_id: Option<String>,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub extends: Vec<String>,
    pub permissions: Vec<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub version: i64,
    pub enabled: bool,
    pub is_template: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Injected by auth middleware into every authenticated request.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuthContext {
    pub org_id: String,
    pub user_id: String,
    pub role: UserRole,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Memory {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub project: String,
    pub tool: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: String,
    // v2 fields
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub memory_type: Option<String>,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub topic_key: Option<String>,
    pub session_id: Option<String>,
    #[serde(default = "default_revision_count")]
    pub revision_count: i64,
    pub normalized_hash: Option<String>,
    pub project_id: Option<String>,
    /// Non-null when the memory has been soft-archived. NULL = active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// Pinned memories float to the top of the list. false = not pinned.
    #[serde(default)]
    pub pinned: bool,
    /// Collection this memory belongs to. None = no collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Private admin note. Never returned to agents or non-admin callers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_note: Option<String>,
    /// ISO datetime when this memory is scheduled for deletion. NULL = no scheduled deletion.
    #[serde(rename = "delete_at", default)]
    pub delete_after: Option<String>,
    /// Derived status: "active" or "archived" (based on archived_at).
    #[serde(default = "default_active_status")]
    pub status: String,
}

fn default_revision_count() -> i64 {
    1
}

/// Pagination envelope returned by `GET /v1/memory` and `POST /v1/memory/search`.
///
/// Generic over the item type so the same envelope shape can carry either full
/// `Memory` rows (default) or `MemoryPreview` rows when `compact=true` is
/// requested — see `MemoryPreview` and `api::memory::MemoryPageResponse`.
#[derive(Serialize)]
pub struct MemoryPage<T = Memory> {
    pub memories: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    /// Set to `"keyword-fallback"` when a `semantic`/`hybrid` search request was
    /// silently downgraded to keyword search because no embed service is
    /// configured. Absent (not serialized) when no degradation occurred.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded: Option<String>,
}

/// Compact representation of a `Memory`, returned instead of the full row when
/// `compact=true` is passed to `GET /v1/memory` or `POST /v1/memory/search`
/// (and to the memory lists embedded in `GET /v1/context*`). `preview` is the
/// first 200 characters of `content` (UTF-8 char-boundary safe).
#[derive(Serialize, Clone, Debug)]
pub struct MemoryPreview {
    pub id: String,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub memory_type: Option<String>,
    pub project: String,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub created_at: String,
    pub preview: String,
}

/// Truncates `s` to at most `max_chars` characters without panicking on
/// multi-byte UTF-8 boundaries (a naive byte-index slice can land mid-codepoint).
pub fn preview_chars(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => s[..byte_idx].to_string(),
        None => s.to_string(),
    }
}

impl From<&Memory> for MemoryPreview {
    fn from(m: &Memory) -> Self {
        MemoryPreview {
            id: m.id.clone(),
            title: m.title.clone(),
            memory_type: m.memory_type.clone(),
            project: m.project.clone(),
            tags: m.tags.clone(),
            pinned: m.pinned,
            created_at: m.created_at.clone(),
            preview: preview_chars(&m.content, 200),
        }
    }
}

/// Request body for `POST /v1/memory/store`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoreMemoryRequest {
    pub project: Option<String>,
    pub tool: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    // v2 optional fields
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub memory_type: Option<String>,
    pub scope: Option<String>,
    pub topic_key: Option<String>,
    pub session_id: Option<String>,
}

/// A session groups a set of memories under a logical work unit.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Session {
    pub id: String,
    pub org_id: String,
    pub name: Option<String>,
    pub project: String,
    pub directory: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub summary: Option<String>,
}

/// Session with memory count — returned by `GET /v1/sessions`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionWithCount {
    pub id: String,
    pub org_id: String,
    pub name: Option<String>,
    pub project: String,
    pub directory: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub summary: Option<String>,
    pub memory_count: i64,
}

/// Request body for `POST /v1/sessions`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateSessionRequest {
    pub project: String,
    pub name: Option<String>,
    pub directory: Option<String>,
    pub summary: Option<String>,
}

/// Request body for `PATCH /v1/sessions/:id`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PatchSessionRequest {
    pub name: Option<String>,
    pub ended_at: Option<String>,
    pub summary: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuditEntry {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub timestamp: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub previous_hash: Option<String>,
    #[serde(default)]
    pub current_hash: Option<String>,
}

/// Aggregated view of recent activity for a single project (tenant-scoped).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectContext {
    pub project: String,
    pub recent_memories: Vec<Memory>,
    pub tools: Vec<String>,
    pub last_activity: Option<String>,
}

// ── Convention ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Convention {
    pub id: i64,
    pub org_id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub content: String,
    pub category: String,
    pub weight: i64,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateConventionRequest {
    pub title: String,
    pub content: String,
    pub category: Option<String>,
    pub weight: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConventionRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub category: Option<String>,
    pub weight: Option<i64>,
    pub tags: Option<Vec<String>>,
}

/// Request body for `POST /v1/audit/log` — external audit ingest.
///
/// `action` and `resource_type` are semantically required but declared as `Option`
/// so that a missing-field JSON body deserializes successfully. The handler validates
/// and returns 400 (not Axum's default 422) when they are absent or empty.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExternalAuditRequest {
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    /// Optional ISO 8601 timestamp override. Server stamps current time if absent.
    pub timestamp: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiError {
    pub error: String,
    pub code: String,
}

/// A single facet value with its memory count.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FacetCount {
    pub value: String,
    pub count: i64,
}

/// Aggregated facet counts for the memory corpus of an org.
/// Returned by `GET /v1/admin/stats/memory-facets`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MemoryFacets {
    pub types: Vec<FacetCount>,
    pub scopes: Vec<FacetCount>,
    pub projects: Vec<FacetCount>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ToolUsage {
    pub tool: String,
    pub count: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OrgStats {
    pub total_memories: i64,
    pub active_users_24h: i64,
    pub searches_today: i64,
    pub top_tools: Vec<ToolUsage>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GlobalMetrics {
    pub total_orgs: i64,
    pub total_users: i64,
    pub total_memories: i64,
    pub active_users_24h: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrgWithStats {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
    pub user_count: i64,
    pub memory_count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UsageStats {
    pub memories: i64,
    pub sessions: i64,
    pub users: i64,
    pub projects: i64,
    pub code_repos: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct DashboardData {
    pub stats: OrgStats,
    pub usage: Option<UsageStats>,
    pub trends: MemoryTrends,
    pub activity: Vec<AuditEntry>,
    pub agent_activity: Option<Vec<AgentActivity>>,
    pub heatmap: Option<Vec<HeatmapDay>>,
    pub contributors: Option<Vec<ContributorStat>>,
    pub health: Option<MemoryHealth>,
    pub users: Option<Vec<User>>,
    pub onboarding: Option<OnboardingStatus>,
    pub conventions: Option<Vec<Convention>>,
    pub availability: DashboardAvailability,
}

#[derive(Serialize, Clone, Debug)]
pub struct DashboardAvailability {
    pub usage: bool,
    pub users: bool,
    pub onboarding: bool,
    pub agent_activity: bool,
    pub health: bool,
    pub contributors: bool,
    pub heatmap: bool,
    pub conventions: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub parent_id: Option<String>,
    /// Non-null when the project has been soft-archived. NULL = active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// Owning client. NULL = an internal u2s project (not unassigned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectMember {
    pub id: String,
    pub project_id: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: String,
}

// ── Client (consultancy grouping) ─────────────────────────────────────────────

/// A client of the consultancy. Sits between `Organization` and `Project`.
///
/// A project whose `client_id` is NULL is *internal* work, not an unassigned
/// project — see `Project::client_id`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Client {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub slug: String,
    #[serde(default = "default_active_status")]
    pub status: String,
    /// Non-null when the client has been soft-archived. NULL = active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClientMember {
    pub id: String,
    pub client_id: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateClientRequest {
    pub name: String,
    pub slug: String,
    #[serde(default = "default_active_status")]
    pub status: String,
}

/// `slug` is deliberately absent: it is the stable external identifier and is
/// immutable after create. Renaming a client changes `name` only.
#[derive(Debug, Deserialize)]
pub struct UpdateClientRequest {
    pub name: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddClientMemberRequest {
    pub user_id: String,
    #[serde(default = "default_member_role")]
    pub role: String,
}

fn default_member_role() -> String {
    "member".to_string()
}

/// The statuses a client may hold. `offboarded` is terminal for new work but
/// keeps history readable — it is not a delete.
pub const CLIENT_STATUSES: [&str; 3] = ["active", "paused", "offboarded"];

/// The scopes a memory may carry, widened from `project | personal` by the
/// client model.
pub const MEMORY_SCOPES: [&str; 4] = ["org", "client", "project", "personal"];

// ── Usage metrics (tokens + execution time) ───────────────────────────────────

/// One recorded unit of agent work: token counts and wall-clock time, resolved
/// at ingest into the task → project → client → org hierarchy. Every id below
/// `org_id` is nullable — an unresolvable reference is stored as NULL, never
/// rejected, so telemetry never 500s the caller.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageEvent {
    pub id: String,
    pub org_id: String,
    pub user_id: Option<String>,
    pub client_id: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub duration_ms: i64,
    /// `ingest` (reported by an agent/harness) or `backfill` (derived from a session).
    pub source: String,
    pub event_ts: String,
    pub created_at: String,
}

/// Body of `POST /v1/usage`. `project` is a project **name** (resolved
/// server-side, never auto-created); if it is absent but `task_id` is present
/// the project is derived from that task.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UsageIngestRequest {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tokens_in: i64,
    #[serde(default)]
    pub tokens_out: i64,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub event_ts: Option<String>,
}

/// One aggregated bucket at the requested rollup level. `key_id` is NULL for
/// events whose id at that level was unresolved (e.g. usage with no project).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageSummaryRow {
    pub key_id: Option<String>,
    pub key_name: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub tokens_total: i64,
    pub duration_ms: i64,
    pub event_count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageSummaryResponse {
    pub rows: Vec<UsageSummaryRow>,
    pub totals: UsageSummaryRow,
}

/// One time bucket of `GET /v1/usage/timeseries`.
///
/// `bucket_ts` is the bucket's leading edge as a date-only (`YYYY-MM-DD`, for
/// `day`/`week`) or hour-precision (`YYYY-MM-DD HH`, for `hour`) string — the
/// same lexicographic shape `usage_events.event_ts` is stored in, so the client
/// can sort and gap-fill without parsing a locale-dependent format.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageBucket {
    pub bucket_ts: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub tokens_total: i64,
    pub duration_ms: i64,
    pub event_count: i64,
}

/// Response of `GET /v1/usage/timeseries`. Only non-empty buckets are returned
/// — the client gap-fills against the requested range, which it already knows.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageTimeseriesResponse {
    pub bucket: String,
    pub buckets: Vec<UsageBucket>,
}

/// A client slug is lowercase alphanumeric with internal dashes, 1–64 chars.
/// Used in URLs and as the stable identifier, so it is validated at the edge
/// rather than trusted from the caller.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() || slug.len() > 64 {
        return Err("slug must be 1–64 characters".to_string());
    }
    let mut chars = slug.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err("slug must start with a lowercase letter or digit".to_string());
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("slug may contain only lowercase letters, digits and dashes".to_string());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct PromoteMemoryRequest {
    /// Optional note recorded on the audit row explaining why this was promoted.
    #[serde(default)]
    pub note: Option<String>,
}

/// Read-only report of how legacy `memories.project` strings map onto real
/// projects. Deliberately carries no mutation — assigning `project_id` to
/// legacy rows is a separate operator action.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectResolutionReport {
    pub resolved: i64,
    pub unresolved: i64,
    pub unresolved_values: Vec<UnresolvedProject>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UnresolvedProject {
    pub project: String,
    pub memory_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectStats {
    pub total_memories: i64,
    pub memories_this_week: i64,
    pub last_memory_at: Option<String>,
    pub top_tags: Vec<String>,
}

/// A single entry in the over-enrolled projects diagnostic report.
/// Returned by `GET /v1/admin/org/projects/over-enrolled`.
#[derive(Debug, Serialize, Deserialize)]
pub struct OverEnrolledProject {
    pub project_name: String,
    pub member_count: i64,
    pub active_user_count: i64,
}

// ── Policy types ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Policy {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub rule_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    /// Project this policy is scoped to. NULL = org-wide (applies to every project).
    #[serde(default)]
    pub project_id: Option<String>,
}

/// Typed enum that represents the config payload for each rule type.
/// Used by `CreatePolicyRequest` (flat body serialization via `#[serde(flatten)]`).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "rule_type", content = "config", rename_all = "snake_case")]
pub enum PolicyConfig {
    ModelWhitelist {
        allowed_models: Vec<String>,
    },
    BudgetLimit {
        #[serde(default)]
        max_tokens_per_day: Option<i64>,
        #[serde(default)]
        max_requests_per_day: Option<i64>,
    },
    PiiRedact {
        patterns: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreatePolicyRequest {
    pub name: String,
    #[serde(flatten)]
    pub config: PolicyConfig,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Project to scope this policy to. None = org-wide (applies to every project).
    #[serde(default)]
    pub project_id: Option<String>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UpdatePolicyRequest {
    pub name: Option<String>,
    /// If present, handler rejects with 400 immutable_rule_type — rule_type cannot change.
    pub rule_type: Option<String>,
    /// Raw JSON config value — validated against the existing rule_type by the handler.
    pub config: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PolicyCheckRequest {
    pub model: String,
    #[serde(default)]
    pub prompt_tokens: Option<i64>,
    /// Also accepted as `content` for callers that use the memory-store field name.
    #[serde(default, alias = "content")]
    pub prompt_preview: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PolicyViolation {
    pub policy_id: String,
    pub policy_name: String,
    pub rule_type: String,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PolicyCheckResponse {
    pub allowed: bool,
    pub violations: Vec<PolicyViolation>,
}

// ── Code index types ──────────────────────────────────────────────────────────

/// Represents a logical code project being indexed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CodeProject {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub root_path: String,
    pub repo_url: Option<String>,
    pub file_count: i64,
    pub chunk_count: i64,
    pub last_indexed: Option<String>,
    pub created_at: String,
    /// Auto re-index interval in hours. NULL = no auto re-index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reindex_interval_hours: Option<i64>,
    /// Timestamp of the last completed index run (success or error).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_indexed_at: Option<String>,
    /// Last indexing error message, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_index_error: Option<String>,
    /// Number of files indexed in the last successful run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed_files_count: Option<i64>,
    /// Current index status: "pending" | "indexing" | "success" | "error".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_status: Option<String>,
    /// Soft-archive timestamp. NULL = active; non-NULL = archived at that datetime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// Glob-like patterns to exclude from indexing (e.g. "*.lock", "node_modules/*").
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

/// Request body for `PATCH /v1/code/projects/:id`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UpdateCodeProjectRequest {
    pub exclude_patterns: Option<Vec<String>>,
}

/// Request body for `PATCH /v1/code/projects/:id/schedule`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateReindexScheduleRequest {
    pub interval_hours: Option<i64>,
}

/// Response body for `POST /v1/code/projects/:id/reindex`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReindexProjectResponse {
    pub status: String,
    pub project_id: String,
}

/// A single chunk of source code with its embedding metadata.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CodeChunk {
    pub id: i64,
    pub code_project_id: i64,
    pub file_path: String,
    pub file_hash: String,
    pub language: Option<String>,
    pub symbol: Option<String>,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
    pub created_at: String,
}

/// Request body for `POST /v1/code/index`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IndexProjectRequest {
    pub project: String,
    pub root_path: Option<String>,
    pub repo_url: Option<String>,
    /// GitHub Personal Access Token for private repositories. Requires the `repo`
    /// (or `contents:read`) scope. Never logged or returned in API responses.
    /// When provided, takes priority over the org-level GitHub OAuth connection.
    /// Stored encrypted at rest (AES-256-GCM) for future reindex operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_token: Option<String>,
    /// When true, build the structural + symbol graph only and skip the slow
    /// embedding pass (no semantic search). Fast, codebase-memory-style indexing.
    #[serde(default)]
    pub graph_only: Option<bool>,
}

/// Response body for `POST /v1/code/index`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IndexProjectResponse {
    pub project: String,
    pub status: String,
    pub file_count: i64,
    pub chunk_count: i64,
    pub last_indexed: String,
}

/// Request body for `POST /v1/code/search`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchCodeRequest {
    pub project: String,
    pub query: String,
    pub top_k: Option<i64>,
    /// Optional file extension filter (e.g. "ts", "rs"). Only results whose
    /// `file_path` ends with `.{extension}` are returned.
    pub extension: Option<String>,
}

/// API key with joined user info — returned by `GET /v1/admin/keys`.
#[derive(Debug, Serialize, Clone)]
pub struct ApiKeyWithUser {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub user_email: String,
    pub label: String,
    pub last_used: Option<String>,
    pub created_at: String,
    pub revoked: bool,
    pub expires_at: Option<String>,
    /// Total number of times this key has been used for authentication.
    #[serde(default)]
    pub times_used: i64,
    /// ISO datetime of the last successful authentication (may differ from last_used by a few ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
}

/// A single result from a code semantic search.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchCodeResult {
    pub file_path: String,
    pub symbol: Option<String>,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
    pub score: f32,
}

/// Request body for `POST /v1/code/locate`.
///
/// Same query embedding + cosine ranking as `/v1/code/search`, but the response is
/// a lean, deduped-by-file list of ranked file paths — the token-saving output an
/// agent uses to jump straight to the right file.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocateCodeRequest {
    pub project: String,
    pub query: String,
    /// Max distinct files to return. Defaults to 5, capped at [`MAX_TOP_K`].
    pub limit: Option<i64>,
}

/// One ranked distinct file in a `POST /v1/code/locate` response. A file's score is
/// its single best-scoring chunk; `top_symbol` is that chunk's symbol.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocateCodeHit {
    pub file_path: String,
    pub top_symbol: Option<String>,
    pub score: f32,
}

/// Response body for `POST /v1/code/locate`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocateCodeResponse {
    pub results: Vec<LocateCodeHit>,
}

/// Response body for `GET /v1/code/status/:project`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CodeStatusResponse {
    pub project: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_indexed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_count: Option<i64>,
}

// ── Code graph API types ──────────────────────────────────────────────────────

/// A single node in the code knowledge graph returned by `GET /v1/code/graph`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphNodeDto {
    pub id: i64,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    pub qualified_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<i64>,
    pub language: String,
}

/// A directed edge in the code knowledge graph.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphEdgeDto {
    pub id: i64,
    pub from_id: i64,
    pub to_id: i64,
    #[serde(rename = "type")]
    pub edge_type: String,
}

/// Response envelope for `GET /v1/code/graph`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphResponse {
    pub project: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
}

// ── Memory graph API types ─────────────────────────────────────────────────

/// A single node in the memory knowledge graph returned by `GET /v1/memory/graph`.
/// Ids are namespaced strings (e.g. `memory:{uuid}`, `project:{id}`, `tag:{name}`)
/// because memory entities span multiple tables with heterogeneous TEXT/UUID keys.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemGraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub label: String,
}

/// A directed edge in the memory knowledge graph.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemGraphEdge {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    #[serde(rename = "type")]
    pub edge_type: String,
}

/// One project that participated in a memory-graph response — used to color
/// the legend and to disambiguate nodes that belong to different projects in
/// the same family. `color` is a stable CSS color string the frontend can use
/// directly (the backend picks from a fixed palette so the frontend never has
/// to know the palette itself).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectGraphInfo {
    pub id: String,
    pub name: String,
    pub color: String,
    pub parent_id: Option<String>,
}

/// Response envelope for `GET /v1/memory/graph`. Mirrors `GraphResponse`'s field
/// names so the frontend force-graph seam can be reused, even though node/edge
/// ids are strings here instead of integers.
///
/// `projects` lists the project(s) that contributed to this response. When the
/// request is scoped to a project family (via `project_id`), `projects` is
/// the resolved family (root + descendants). For the legacy single-project
/// lookup (`?project=name`), it contains a single entry for that project.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryGraphResponse {
    pub project: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes: Vec<MemGraphNode>,
    pub edges: Vec<MemGraphEdge>,
    pub projects: Vec<ProjectGraphInfo>,
}

/// Response body for `GET /v1/code/snippet` — the source of the chunk covering a symbol.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SnippetResponse {
    pub file_path: String,
    pub symbol: Option<String>,
    pub language: Option<String>,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
}

// ── Project event override types ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProjectEventOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_issues: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_prs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respond_comments: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_index: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanner: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectEventOverridesRequest {
    pub overrides: ProjectEventOverrides,
}

// ── Webhook types ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Webhook {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub target_url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
    pub active: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub target_url: String,
    pub secret: Option<String>,
    pub events: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateWebhookRequest {
    pub active: Option<bool>,
    pub secret: Option<String>,
    pub events: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UserSummary {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
}

// ── Webhook delivery log ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: String,
    pub webhook_id: String,
    pub org_id: String,
    pub event_type: String,
    pub payload: String,
    pub status_code: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
    pub delivered_at: String,
}

// ── Webhook test result ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct WebhookTestResult {
    pub success: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GlobalSearchResult {
    pub memories: Vec<Memory>,
    pub users: Vec<UserSummary>,
    pub projects: Vec<Project>,
    pub policies: Vec<Policy>,
    pub conventions: Vec<Convention>,
    /// Additive facet. Empty (never a 403) for a caller without `sdd:read` —
    /// gating the whole search on a brand-new permission would break global
    /// search for every existing user (design.md A4).
    #[serde(default)]
    pub sdd_changes: Vec<SddChangeSummary>,
    /// The living specifications — gated exactly like `sdd_changes`: empty for a
    /// caller without `sdd:read`, never a 403.
    #[serde(default)]
    pub sdd_specs: Vec<SddSpecSummary>,
}

/// Result type for the internal (backoffice) search endpoint.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InternalSearchResult {
    pub orgs: Vec<OrgWithStats>,
    pub users: Vec<User>,
}

/// Daily memory count — one entry per calendar day.
#[derive(Debug, Serialize, Clone)]
pub struct DailyCount {
    pub date: String, // "YYYY-MM-DD"
    pub count: i64,
}

/// Name + count pair used for type/project breakdowns.
#[derive(Debug, Serialize, Clone)]
pub struct NameCount {
    pub name: String,
    pub count: i64,
}

/// Memory trend data for the last 30 days — returned by `GET /v1/admin/stats/trends`.
#[derive(Debug, Serialize, Clone)]
pub struct MemoryTrends {
    pub daily_counts: Vec<DailyCount>, // last 30 days
    pub by_type: Vec<NameCount>,       // top 5 types
    pub by_project: Vec<NameCount>,    // top 5 projects
    pub total: i64,
    pub this_week: i64,  // last 7 days
    pub this_month: i64, // last 30 days
}

// ── Onboarding types ──────────────────────────────────────────────────────────

/// A single onboarding checklist item.
#[derive(Debug, Serialize, Clone)]
pub struct OnboardingItem {
    pub key: String,
    pub label: String,
    pub description: String,
    pub done: bool,
}

/// Onboarding status returned by `GET /v1/admin/onboarding`.
#[derive(Debug, Serialize, Clone)]
pub struct OnboardingStatus {
    pub items: Vec<OnboardingItem>,
}

/// Request body for `PATCH /v1/memory/:id`.
/// All fields are optional — at least one must be provided.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UpdateMemoryRequest {
    pub content: Option<String>,
    pub title: Option<String>,
}

/// Returned by `POST /v1/admin/users/:user_id/reset-key`.
/// The new key is only visible once — callers must show it immediately.
#[derive(Debug, Serialize, Clone)]
pub struct ResetKeyResponse {
    pub new_key: String,
}

/// Returned by `POST /v1/admin/keys` (create) and `POST /v1/admin/keys/:id/rotate`.
/// `raw_key` is only visible once — callers must display it immediately.
#[derive(Debug, Serialize, Clone)]
pub struct ApiKeyCreatedResponse {
    pub key: ApiKeyWithUser,
    pub raw_key: String,
}

/// Input for `POST /v1/admin/keys` — creates a new API key for a user.
#[derive(Debug, Deserialize, Clone)]
pub struct CreateApiKeyRequest {
    pub user_id: String,
    pub label: Option<String>,
    pub expires_at: Option<String>,
}

/// Input for `PATCH /v1/admin/keys/:id` — updates mutable fields of a key.
#[derive(Debug, Deserialize, Clone)]
pub struct UpdateApiKeyRequest {
    pub label: Option<String>,
    /// Pass `null` explicitly to clear the expiry.
    pub expires_at: Option<serde_json::Value>,
}

// ── Invite link types ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InviteLink {
    pub token: String,
    pub org_id: String,
    pub role: String,
    pub created_by: String,
    pub used_at: Option<String>,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateInviteLinkRequest {
    /// Role to assign to the user who accepts the invite. Defaults to "user".
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InviteLinkResponse {
    pub token: String,
    pub invite_url: String,
    pub expires_at: String,
    pub role: String,
}

// ── Memory import types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ImportMemory {
    pub content: String,
    pub project: Option<String>,
    pub scope: Option<String>,
    #[serde(rename = "type")]
    pub memory_type: Option<String>,
    pub tags: Option<Vec<String>>,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ImportMemoriesRequestInner {
    Array(Vec<ImportMemory>),
    Object { memories: Vec<ImportMemory> },
}

/// Accepts either a raw JSON array or `{ "memories": [...] }`.
#[derive(Debug)]
pub struct ImportMemoriesRequest {
    pub memories: Vec<ImportMemory>,
}

impl<'de> serde::Deserialize<'de> for ImportMemoriesRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = ImportMemoriesRequestInner::deserialize(deserializer)?;
        Ok(match inner {
            ImportMemoriesRequestInner::Array(memories) => ImportMemoriesRequest { memories },
            ImportMemoriesRequestInner::Object { memories } => ImportMemoriesRequest { memories },
        })
    }
}

#[derive(Debug, Serialize)]
pub struct ImportMemoriesResponse {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// Request body for `PATCH /v1/admin/memories/:id/note`.
/// Empty string clears the note (sets admin_note = NULL).
#[derive(Debug, Deserialize)]
pub struct UpdateNoteRequest {
    pub note: String,
}

/// Request body for `PATCH /v1/admin/org/announcement`.
/// Empty string = clear the announcement.
#[derive(Debug, Deserialize)]
pub struct UpdateAnnouncementRequest {
    pub announcement: String,
    pub announcement_type: Option<String>,
}

/// Request body for `PATCH /v1/admin/memories/:id/schedule-delete`.
/// None = clear the scheduled deletion date.
#[derive(Debug, Deserialize)]
pub struct ScheduleDeleteRequest {
    #[serde(alias = "delete_after")]
    pub delete_at: Option<String>,
}

/// Request body for `POST /v1/admin/memories/merge`.
///
/// Accepts two calling conventions:
///   - `source_id` + `target_id` (preferred): `target_id` is kept, `source_id` is merged in.
///   - `keep_id` + `merge_id` (legacy): explicit field names.
///
/// When both conventions are provided the explicit `keep_id`/`merge_id` take precedence.
#[derive(Debug, Deserialize)]
pub struct MergeMemoriesRequest {
    /// ID of the memory to keep (preferred field name).
    pub target_id: Option<String>,
    /// ID of the memory to merge into the kept one (preferred field name).
    pub source_id: Option<String>,
    /// Alias for `target_id` (legacy).
    pub keep_id: Option<String>,
    /// Alias for `source_id` (legacy).
    pub merge_id: Option<String>,
}

impl MergeMemoriesRequest {
    /// Returns `(keep_id, merge_id)` or an error string if the fields cannot be resolved.
    pub fn resolve(&self) -> Result<(String, String), &'static str> {
        let keep = self
            .keep_id
            .clone()
            .or_else(|| self.target_id.clone())
            .ok_or("missing field `keep_id` or `target_id`")?;
        let merge = self
            .merge_id
            .clone()
            .or_else(|| self.source_id.clone())
            .ok_or("missing field `merge_id` or `source_id`")?;
        Ok((keep, merge))
    }
}

fn default_bulk_action() -> String {
    "add".to_string()
}

fn deserialize_tag_field<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TagVisitor;

    impl<'de> serde::de::Visitor<'de> for TagVisitor {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a string or an array of strings")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            Ok(vec![v.to_owned()])
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Vec<String>, E> {
            Ok(vec![v])
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Vec<String>, A::Error> {
            let mut out = Vec::new();
            while let Some(s) = seq.next_element::<String>()? {
                out.push(s);
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(TagVisitor)
}

/// Request body for `POST /v1/admin/memories/bulk-tag`.
///
/// Accepts both the canonical field names and common aliases:
/// - `ids` or `memory_ids`
/// - `tag` (string) or `tags` (string or array of strings)
/// - `action` defaults to `"add"` when omitted
#[derive(Debug, Deserialize)]
pub struct BulkTagRequest {
    #[serde(alias = "memory_ids")]
    pub ids: Vec<String>,
    #[serde(default = "default_bulk_action")]
    pub action: String,
    #[serde(alias = "tag", deserialize_with = "deserialize_tag_field")]
    pub tags: Vec<String>,
}

/// Response body for `POST /v1/admin/memories/bulk-tag`.
#[derive(Debug, Serialize)]
pub struct BulkTagResponse {
    pub updated: usize,
}

/// Request body for `POST /v1/admin/tags/rename`.
#[derive(Debug, Deserialize)]
pub struct RenameTagRequest {
    pub from: String,
    pub to: String,
}

/// Response body for `POST /v1/admin/tags/rename`.
#[derive(Debug, Serialize)]
pub struct RenameTagResponse {
    pub updated_count: i64,
}

/// A single contributor entry — returned by `GET /v1/admin/stats/top-contributors`.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ContributorStat {
    pub user_id: String,
    pub memory_count: i64,
    pub last_activity: String,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
}

/// A single day entry in the memory creation heatmap.
/// Returned by `GET /v1/admin/stats/memory-heatmap`.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct HeatmapDay {
    pub day: String, // "YYYY-MM-DD"
    pub count: i64,
}

/// Memory health summary — returned by `GET /v1/admin/memories/health`.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct MemoryHealth {
    pub total_memories: i64,
    pub duplicate_count: i64,
    pub stale_count: i64,
    pub untagged_count: i64,
}

/// Agent/tool activity summary — returned by `GET /v1/admin/stats/agent-activity`.
#[derive(Debug, Serialize, Clone)]
pub struct AgentActivity {
    pub tool: String,
    pub total_memories: i64,
    pub memories_last_24h: i64,
    pub memories_last_7d: i64,
    pub last_seen: String,
}

// ── Notification types ────────────────────────────────────────────────────────

/// A single derived notification item — computed from recent audit log events.
/// Returned by `GET /v1/admin/notifications`.
#[derive(Debug, Serialize, Clone)]
pub struct NotificationItem {
    pub id: String,
    pub message: String,
    pub action: String,
    pub resource_type: Option<String>,
    pub created_at: String,
    pub actor: Option<String>,
}

// ── Collections ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub memory_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssignCollectionRequest {
    pub collection_id: Option<String>, // null to unassign
}

// ── GitHub OAuth types ────────────────────────────────────────────────────────

/// Response for `GET /v1/github/auth` — contains the GitHub OAuth redirect URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubAuthUrlResponse {
    pub url: String,
}

/// Request body for `POST /v1/github/callback`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCallbackRequest {
    pub code: String,
    pub state: Option<String>,
}

/// Response for `GET /v1/github/status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConnectionStatus {
    pub connected: bool,
    pub github_login: Option<String>,
    pub scopes: Option<String>,
}

// ── Agent types ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Agent {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub model: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub model: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AgentAssignment {
    pub id: String,
    pub agent_id: String,
    pub org_id: String,
    pub repo_url: String,
    pub created_at: String,
}

// ── Autonomous agent control plane ──────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentDefinition {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub description: Option<String>,
    pub template_key: String,
    pub template_version: i64,
    pub status: String,
    pub current_revision: i64,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    /// Validation status of the current revision ('pending' | 'valid' | 'invalid').
    /// Lets the admin gate the Enable action without fetching each agent's detail.
    #[serde(default)]
    pub validation_status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentRevision {
    pub id: String,
    pub definition_id: String,
    pub revision: i64,
    pub config: serde_json::Value,
    pub config_hash: String,
    pub capabilities: Vec<String>,
    pub budgets: serde_json::Value,
    pub policy_generation: i64,
    pub validation_status: String,
    pub validation: Option<serde_json::Value>,
    pub validated_at: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentDetail {
    #[serde(flatten)]
    pub definition: AutonomousAgentDefinition,
    pub revision: AutonomousAgentRevision,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAutonomousAgentRequest {
    pub name: String,
    pub description: Option<String>,
    pub template_key: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub budgets: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAutonomousAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
    pub budgets: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentTemplate {
    pub key: String,
    pub version: i64,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub default_budgets: serde_json::Value,
    pub config_schema: serde_json::Value,
    pub workflow: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentSchedule {
    pub id: String,
    pub definition_id: String,
    pub kind: String,
    pub expression: Option<String>,
    pub timezone: String,
    pub misfire_policy: String,
    pub next_run_at: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutAutonomousAgentScheduleRequest {
    pub kind: String,
    pub expression: Option<String>,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_misfire_policy")]
    pub misfire_policy: String,
    #[serde(default)]
    pub enabled: bool,
}

fn default_timezone() -> String {
    "UTC".into()
}
fn default_misfire_policy() -> String {
    "run_once".into()
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentRun {
    pub id: String,
    pub definition_id: String,
    pub revision_id: String,
    pub trigger_kind: String,
    pub occurrence_key: String,
    pub scheduled_for: Option<String>,
    pub snapshot_sha: Option<String>,
    pub status: String,
    pub budget: serde_json::Value,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentConnector {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub metadata: serde_json::Value,
    pub scopes: Vec<String>,
    pub health: String,
    pub revocation_generation: i64,
    pub secret_configured: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutAutonomousAgentConnectorRequest {
    pub kind: String,
    pub name: String,
    pub secret: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentFinding {
    pub id: String,
    pub definition_id: String,
    pub run_id: String,
    pub fingerprint: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub summary: String,
    pub evidence: serde_json::Value,
    pub occurrence_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchAutonomousAgentFindingRequest {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentDelivery {
    pub id: String,
    pub run_id: String,
    pub finding_id: Option<String>,
    pub channel: String,
    pub status: String,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
    pub attempts: i64,
    pub last_error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentEvent {
    pub sequence: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AutonomousAgentTarget {
    pub id: String,
    pub definition_id: String,
    pub kind: String,
    pub name: String,
    pub config: serde_json::Value,
    pub credential_connector_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutAutonomousAgentTargetRequest {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub config: serde_json::Value,
    pub credential_connector_id: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

// ── Harness sharing types ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Harness {
    pub id: String,
    pub org_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub slug: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub visibility: String,
    pub status: String,
    pub created_by: String,
    pub owner_user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<HarnessOwner>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<HarnessVersionSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessOwner {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessVersionSummary {
    pub id: String,
    pub version: String,
    pub manifest_hash: String,
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_metadata: Option<serde_json::Value>,
    pub status: String,
    pub published_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessVersion {
    pub id: String,
    pub harness_id: String,
    pub version: String,
    pub manifest: serde_json::Value,
    pub manifest_hash: String,
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    pub provenance: serde_json::Value,
    pub status: String,
    pub published_by: String,
    pub published_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CreateHarnessRequest {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub owner_user_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PublishHarnessVersionRequest {
    pub version: String,
    pub manifest: serde_json::Value,
    pub manifest_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessDownloadResponse {
    pub harness_id: String,
    pub version: String,
    pub manifest: serde_json::Value,
    pub manifest_hash: String,
    pub approval_required: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HarnessApprovalRequest {
    pub target_tool: String,
    pub target_scope: String,
    pub manifest_hash: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HarnessInstallResultRequest {
    pub approval_id: String,
    pub manifest_hash: String,
    pub status: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessApproval {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub harness_version_id: String,
    pub target_tool: String,
    pub target_scope: String,
    pub manifest_hash: String,
    pub status: String,
    pub metadata: serde_json::Value,
    pub approved_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessRecommendation {
    pub harness_id: String,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<HarnessOwner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_metadata: Option<serde_json::Value>,
    pub manifest_hash: String,
    pub approval_required: bool,
    pub download_url: String,
    pub required_permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HarnessFormat {
    Agent,
    Skill,
    Command,
    Hook,
    OutputStyle,
    ClaudeCodePlugin,
    Theme,
}

impl HarnessFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            HarnessFormat::Agent => "agent",
            HarnessFormat::Skill => "skill",
            HarnessFormat::Command => "command",
            HarnessFormat::Hook => "hook",
            HarnessFormat::OutputStyle => "output_style",
            HarnessFormat::ClaudeCodePlugin => "claude_code_plugin",
            HarnessFormat::Theme => "theme",
        }
    }
}

impl FromStr for HarnessFormat {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "agent" => Ok(HarnessFormat::Agent),
            "skill" => Ok(HarnessFormat::Skill),
            "command" => Ok(HarnessFormat::Command),
            "hook" => Ok(HarnessFormat::Hook),
            "output_style" => Ok(HarnessFormat::OutputStyle),
            "claude_code_plugin" => Ok(HarnessFormat::ClaudeCodePlugin),
            "theme" => Ok(HarnessFormat::Theme),
            _ => Err("unsupported_format"),
        }
    }
}

pub fn validate_typed_harness_manifest(manifest: &serde_json::Value) -> Result<(), &'static str> {
    if manifest.get("schema_version").and_then(|v| v.as_str()) != Some("1.1") {
        return Err("unsupported_schema_version");
    }
    let targets = manifest
        .get("targets")
        .and_then(|v| v.as_array())
        .ok_or("missing_targets")?;
    // Valid targets: claude, codex, cursor. `opencode` was removed in favor of `cursor`
    // (SDD change harness-agent-tools, Phase 0). Any already-published `harness_versions`
    // rows with `opencode` in `targets` need an operational data UPDATE to `cursor`
    // (or archival) before/with rollout — see openspec/changes/harness-agent-tools/tasks.md
    // task 0.8 and MIGRATION_NOTE.md in that change dir.
    if targets.is_empty()
        || targets
            .iter()
            .any(|v| !matches!(v.as_str(), Some("claude" | "codex" | "cursor")))
    {
        return Err("missing_targets");
    }
    let format = manifest
        .get("format")
        .and_then(|v| v.as_str())
        .ok_or("missing_format")?
        .parse::<HarnessFormat>()?;
    let provenance = manifest
        .get("provenance")
        .and_then(|v| v.as_object())
        .ok_or("missing_provenance")?;
    if provenance
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err("missing_provenance");
    }
    let security = manifest
        .get("security")
        .and_then(|v| v.as_object())
        .ok_or("missing_security")?;
    if security.get("requires_approval").and_then(|v| v.as_bool()) != Some(true) {
        return Err("approval_required");
    }
    if security.get("secret_scan_status").and_then(|v| v.as_str()) == Some("failed") {
        return Err("secret_scan_failed");
    }
    if matches!(
        format,
        HarnessFormat::Hook | HarnessFormat::ClaudeCodePlugin
    ) && security.get("executable").and_then(|v| v.as_bool()) != Some(true)
    {
        return Err("executable_warning_required");
    }
    let components = manifest
        .get("components")
        .and_then(|v| v.as_array())
        .ok_or("missing_components")?;
    if components.is_empty() {
        return Err("missing_components");
    }
    for component in components {
        validate_manifest_component(&format, component)?;
    }
    Ok(())
}

fn validate_manifest_component(
    format: &HarnessFormat,
    component: &serde_json::Value,
) -> Result<(), &'static str> {
    let kind = component
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or("missing_component_kind")?;
    let path = component
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing_component_path")?;
    validate_safe_manifest_path(path)?;
    if let Some(content) = component.get("content").and_then(|v| v.as_str()) {
        validate_safe_manifest_content(content)?;
    }
    if kind == "folder" {
        let entries = component
            .get("entries")
            .and_then(|v| v.as_array())
            .ok_or("missing_folder_entries")?;
        if entries.is_empty() {
            return Err("missing_folder_entries");
        }
        for entry in entries {
            if entry.get("kind").and_then(|v| v.as_str()) != Some("file") {
                return Err("format_component_mismatch");
            }
            let entry_path = entry
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing_component_path")?;
            validate_safe_manifest_path(entry_path)?;
            require_file_metadata(entry)?;
            if let Some(content) = entry.get("content").and_then(|v| v.as_str()) {
                validate_safe_manifest_content(content)?;
            }
        }
    } else {
        require_file_metadata(component)?;
    }
    let valid = match format {
        HarnessFormat::Agent => kind == "file" && path.ends_with(".md"),
        HarnessFormat::Skill => (kind == "file" && path.ends_with(".md")) || kind == "folder",
        HarnessFormat::Command => kind == "file" && path.ends_with(".md"),
        HarnessFormat::Hook => kind == "file" && path.ends_with(".sh"),
        HarnessFormat::OutputStyle => kind == "file" && path.ends_with(".md"),
        HarnessFormat::ClaudeCodePlugin => {
            kind == "plugin_marketplace"
                && path.ends_with(".json")
                && component
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(json_content_is_object)
                    .unwrap_or(false)
        }
        HarnessFormat::Theme => {
            kind == "theme_json"
                && path.ends_with(".json")
                && component
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(json_content_is_object)
                    .unwrap_or(false)
        }
    };
    if !valid {
        return Err("format_component_mismatch");
    }
    Ok(())
}

fn require_file_metadata(value: &serde_json::Value) -> Result<(), &'static str> {
    let media_type = value
        .get("media_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let sha256 = value
        .get("sha256")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let Some(size_bytes) = value.get("size_bytes").and_then(|v| v.as_u64()) else {
        return Err("missing_component_metadata");
    };
    if media_type.is_empty() || sha256.is_empty() {
        return Err("missing_component_metadata");
    }
    if let Some(content) = value.get("content").and_then(|v| v.as_str()) {
        if size_bytes != content_size_bytes(content) || sha256 != expected_content_sha256(content) {
            return Err("component_integrity_mismatch");
        }
    }
    Ok(())
}

fn content_size_bytes(content: &str) -> u64 {
    content.len() as u64
}

fn expected_content_sha256(content: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(content.as_bytes())))
}

fn validate_safe_manifest_path(path: &str) -> Result<(), &'static str> {
    let lower = path.to_lowercase();
    if path.trim().is_empty()
        || path.starts_with('/')
        || path.starts_with('~')
        || looks_like_windows_absolute_path(path)
        || path.contains("..")
        || lower.contains("/users/")
        || lower.contains("\\users\\")
        || lower.contains(".ssh")
        || lower.contains(".env")
    {
        return Err("unsafe_component_path");
    }
    Ok(())
}

fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn validate_safe_manifest_content(content: &str) -> Result<(), &'static str> {
    if content.len() > 64 * 1024 {
        return Err("component_content_too_large");
    }
    let lower = content.to_lowercase();
    if lower.contains("raw-secret")
        || lower.contains("bearer ")
        || lower.contains("nm_live")
        || lower.contains("ghp_")
        || lower.contains("/users/")
        || contains_openai_key(&lower)
    {
        return Err("secret_scan_failed");
    }
    Ok(())
}

/// An OpenAI key is `sk-` followed by a long opaque token. A bare `contains("sk-")`
/// is NOT that check: `sk-` is a substring of ordinary English words, and every one
/// of these is real text from harnesses this scanner refused to publish:
///
///   task-specific   ask-on-risk   risk-based   disk-backed   mask-off
///
/// The scanner was rejecting documents for containing the word "task-specific".
/// Refusing a publish is a serious act; it has to be for a secret, not for prose.
///
/// So: require the prefix at a token boundary, followed by enough opaque characters
/// that it cannot be a word. A real key is `sk-` + 20+ of [a-z0-9_-]; "task-specific"
/// gives 8 before hitting nothing, and every word above dies on the boundary check.
fn contains_openai_key(lower: &str) -> bool {
    const PREFIX: &str = "sk-";
    const MIN_TOKEN_LEN: usize = 20;

    let bytes = lower.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(PREFIX) {
        let start = from + rel;

        // Token boundary: the prefix must not be glued to a preceding word character,
        // which is exactly what makes "task-", "ask-" and "risk-" false positives.
        let boundary = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();

        if boundary {
            let token: usize = lower[start + PREFIX.len()..]
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
                .count();
            if token >= MIN_TOKEN_LEN {
                return true;
            }
        }
        from = start + PREFIX.len();
    }
    false
}

fn json_content_is_object(content: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| v.as_object().map(|_| ()))
        .is_some()
}

#[derive(Debug, Deserialize, Clone)]
pub struct CreateHarnessConfigReviewRequest {
    pub source_tool: String,
    pub redacted_config: serde_json::Value,
    pub redaction_report: serde_json::Value,
    pub content_hash: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessConfigReview {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub source_tool: String,
    pub redacted_config: serde_json::Value,
    pub redaction_report: serde_json::Value,
    pub content_hash: String,
    pub status: String,
    pub created_at: String,
    pub shared_at: Option<String>,
    /// Author identity, populated on read (joined from users). None on create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<HarnessConfigReviewAuthor>,
}

/// Lightweight author identity attached to a config review or comment on read.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessConfigReviewAuthor {
    pub id: String,
    pub name: String,
    pub email: String,
}

/// A comment left by a user on a shared config review (DB row + author on read).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HarnessConfigReviewComment {
    pub id: String,
    pub org_id: String,
    pub review_id: String,
    pub user_id: String,
    pub body: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<HarnessConfigReviewAuthor>,
}

#[derive(Debug, Deserialize)]
pub struct CreateHarnessConfigReviewCommentRequest {
    pub body: String,
}

/// Stored GitHub OAuth connection for an org (DB row).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConnection {
    pub org_id: String,
    pub access_token: String,
    pub token_type: String,
    pub scopes: String,
    pub github_login: String,
    pub github_user_id: i64,
    pub created_at: String,
    pub updated_at: String,
}

// ── Knowledge migration types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationRunStatus {
    Staging,
    InReview,
    Committing,
    Completed,
    Cancelled,
}

impl fmt::Display for MigrationRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Staging => "staging",
            Self::InReview => "in_review",
            Self::Committing => "committing",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        };
        f.write_str(value)
    }
}

impl FromStr for MigrationRunStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "staging" => Ok(Self::Staging),
            "in_review" => Ok(Self::InReview),
            "committing" => Ok(Self::Committing),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown migration run status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationCandidateStatus {
    Staged,
    Approved,
    Rejected,
    Committing,
    Committed,
    Skipped,
    Failed,
    Cancelled,
}

impl fmt::Display for MigrationCandidateStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Staged => "staged",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Committing => "committing",
            Self::Committed => "committed",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        };
        f.write_str(value)
    }
}

impl FromStr for MigrationCandidateStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "staged" => Ok(Self::Staged),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "committing" => Ok(Self::Committing),
            "committed" => Ok(Self::Committed),
            "skipped" => Ok(Self::Skipped),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown migration candidate status: {value}")),
        }
    }
}

pub fn can_transition_migration_run(from: MigrationRunStatus, to: MigrationRunStatus) -> bool {
    use MigrationRunStatus::*;
    matches!(
        (from, to),
        (Staging, InReview | Cancelled)
            | (InReview, Staging | Committing | Cancelled)
            | (Committing, InReview | Completed)
    )
}

pub fn can_transition_migration_candidate(
    from: MigrationCandidateStatus,
    to: MigrationCandidateStatus,
) -> bool {
    use MigrationCandidateStatus::*;
    matches!(
        (from, to),
        (Staged, Approved | Rejected | Cancelled)
            | (Approved, Staged | Committing | Cancelled)
            | (Committing, Committed | Skipped | Failed | Approved)
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReviewItem {
    pub candidate_id: String,
    pub expected_version: i64,
    pub decision: MigrationReviewDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationReviewDecision {
    Approve,
    Reject,
    Cancel,
}

#[derive(Debug, Deserialize)]
pub struct BulkMigrationReviewRequest {
    pub items: Vec<MigrationReviewItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReviewResult {
    pub candidate_id: String,
    pub expected_version: i64,
    pub candidate_status: MigrationCandidateStatus,
    pub error: Option<String>,
}

// ── Team Tasks types ─────────────────────────────────────────────────────────

/// Fixed task status set (team-tasks-core / "Fixed Task Status Set"). Custom or
/// per-organization statuses are not supported in v1. Hand-rolled `FromStr`/
/// `Display` mirrors `Role`'s pattern — stored as TEXT, parsed on write
/// (invalid -> 4xx), serialized as the snake_case string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Backlog,
    Todo,
    InProgress,
    InReview,
    Done,
    Cancelled,
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "backlog" => Ok(TaskStatus::Backlog),
            "todo" => Ok(TaskStatus::Todo),
            "in_progress" => Ok(TaskStatus::InProgress),
            "in_review" => Ok(TaskStatus::InReview),
            "done" => Ok(TaskStatus::Done),
            "cancelled" => Ok(TaskStatus::Cancelled),
            other => Err(format!("unknown task status: {other}")),
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TaskStatus::Backlog => "backlog",
            TaskStatus::Todo => "todo",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::InReview => "in_review",
            TaskStatus::Done => "done",
            TaskStatus::Cancelled => "cancelled",
        };
        write!(f, "{s}")
    }
}

/// Status transition matrix (design.md §2.2). Any active state may transition
/// to `cancelled`; `done` is reached only from `in_progress` or `in_review`;
/// `done`/`cancelled` may be reopened (to `in_progress` / `backlog`
/// respectively); same-state transitions are a no-op allowed (idempotent
/// PATCH). Auto-resolve (resolve-by-spec) bypasses this matrix entirely — it
/// is a system transition, not a user edit — and must call the DB write
/// directly rather than going through this function.
pub fn can_transition(from: TaskStatus, to: TaskStatus) -> bool {
    use TaskStatus::*;
    if from == to {
        return true;
    }
    matches!(
        (from, to),
        (Backlog, Todo)
            | (Backlog, InProgress)
            | (Backlog, Cancelled)
            | (Todo, Backlog)
            | (Todo, InProgress)
            | (Todo, Cancelled)
            | (InProgress, Backlog)
            | (InProgress, Todo)
            | (InProgress, InReview)
            | (InProgress, Done)
            | (InProgress, Cancelled)
            | (InReview, InProgress)
            | (InReview, Done)
            | (InReview, Cancelled)
            | (Done, InProgress)
            | (Cancelled, Backlog)
    )
}

// ── SDD artifacts ───────────────────────────────────────────────────────────

/// The nine SDD artifact kinds, one per file the harness writes
/// (openspec-convention.md). The string form is the **on-disk filename stem**,
/// so `apply-progress` is hyphenated, not snake_case — the DB and the
/// filesystem must agree about the identity of the same artifact.
///
/// `Spec` is the only kind that repeats within a change (once per capability,
/// from `specs/{capability}/spec.md`); every other kind appears at most once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SddArtifactKind {
    Exploration,
    Proposal,
    Spec,
    Design,
    Tasks,
    ApplyProgress,
    VerifyReport,
    ArchiveReport,
    State,
}

impl FromStr for SddArtifactKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exploration" => Ok(SddArtifactKind::Exploration),
            "proposal" => Ok(SddArtifactKind::Proposal),
            "spec" => Ok(SddArtifactKind::Spec),
            "design" => Ok(SddArtifactKind::Design),
            "tasks" => Ok(SddArtifactKind::Tasks),
            "apply-progress" => Ok(SddArtifactKind::ApplyProgress),
            "verify-report" => Ok(SddArtifactKind::VerifyReport),
            "archive-report" => Ok(SddArtifactKind::ArchiveReport),
            "state" => Ok(SddArtifactKind::State),
            other => Err(format!("unknown sdd artifact kind: {other}")),
        }
    }
}

impl fmt::Display for SddArtifactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SddArtifactKind::Exploration => "exploration",
            SddArtifactKind::Proposal => "proposal",
            SddArtifactKind::Spec => "spec",
            SddArtifactKind::Design => "design",
            SddArtifactKind::Tasks => "tasks",
            SddArtifactKind::ApplyProgress => "apply-progress",
            SddArtifactKind::VerifyReport => "verify-report",
            SddArtifactKind::ArchiveReport => "archive-report",
            SddArtifactKind::State => "state",
        };
        write!(f, "{s}")
    }
}

/// Position in the SDD DAG. Advisory metadata — the artifact inventory is the
/// ground truth for what exists. `phase` exists so the admin can render a
/// pipeline and `/sdd-continue` can resume without reading every artifact; a
/// save MUST NOT be rejected because it arrives "out of phase".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SddPhase {
    Explore,
    Propose,
    Spec,
    Design,
    Tasks,
    Apply,
    Verify,
    Archive,
}

impl SddPhase {
    /// DAG order. The importer infers a change's phase as the max rank over the
    /// artifact kinds actually present on disk.
    pub fn rank(&self) -> u8 {
        match self {
            SddPhase::Explore => 0,
            SddPhase::Propose => 1,
            SddPhase::Spec => 2,
            SddPhase::Design => 3,
            SddPhase::Tasks => 4,
            SddPhase::Apply => 5,
            SddPhase::Verify => 6,
            SddPhase::Archive => 7,
        }
    }
}

impl FromStr for SddPhase {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "explore" => Ok(SddPhase::Explore),
            "propose" => Ok(SddPhase::Propose),
            "spec" => Ok(SddPhase::Spec),
            "design" => Ok(SddPhase::Design),
            "tasks" => Ok(SddPhase::Tasks),
            "apply" => Ok(SddPhase::Apply),
            "verify" => Ok(SddPhase::Verify),
            "archive" => Ok(SddPhase::Archive),
            other => Err(format!("unknown sdd phase: {other}")),
        }
    }
}

impl fmt::Display for SddPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SddPhase::Explore => "explore",
            SddPhase::Propose => "propose",
            SddPhase::Spec => "spec",
            SddPhase::Design => "design",
            SddPhase::Tasks => "tasks",
            SddPhase::Apply => "apply",
            SddPhase::Verify => "verify",
            SddPhase::Archive => "archive",
        };
        write!(f, "{s}")
    }
}

/// Lifecycle of a change folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SddStatus {
    Active,
    Archived,
    Abandoned,
}

impl FromStr for SddStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(SddStatus::Active),
            "archived" => Ok(SddStatus::Archived),
            "abandoned" => Ok(SddStatus::Abandoned),
            other => Err(format!("unknown sdd status: {other}")),
        }
    }
}

impl fmt::Display for SddStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SddStatus::Active => "active",
            SddStatus::Archived => "archived",
            SddStatus::Abandoned => "abandoned",
        };
        write!(f, "{s}")
    }
}

/// An SDD change — one `openspec/changes/{name}/` folder. Root entity;
/// everything (tasks, memories, sprints) links to this.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddChange {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub status: String,
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprint_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// Hydrated on detail reads only. Never carries content — see `SddArtifact`.
    #[serde(default)]
    pub artifacts: Vec<SddArtifact>,
    #[serde(default)]
    pub task_links: Vec<Task>,
    #[serde(default)]
    pub memory_links: Vec<Memory>,
}

/// Thin projection for `GlobalSearchResult` — additive facet, no content.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddChangeSummary {
    pub id: String,
    pub project: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub phase: String,
    pub status: String,
}

/// One artifact file within a change. Carries NO content — content lives in
/// revisions and is fetched explicitly.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddArtifact {
    pub id: String,
    pub change_id: String,
    pub kind: String,
    /// Empty string for every kind except `spec`. Never NULL — see migrations v53.
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub latest_revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// An artifact plus the content of its latest revision.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddArtifactDetail {
    #[serde(flatten)]
    pub artifact: SddArtifact,
    pub change_name: String,
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// A full, immutable revision — with content.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddRevision {
    pub id: String,
    pub artifact_id: String,
    pub revision: i64,
    pub content: String,
    pub content_hash: String,
    pub byte_size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_path: Option<String>,
    pub source: String,
    pub created_by: String,
    pub created_at: String,
}

/// Revision metadata. **Has no `content` field on purpose** — the list endpoint
/// physically cannot leak a 36 KB document because the type cannot hold one.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddRevisionMeta {
    pub id: String,
    pub artifact_id: String,
    pub revision: i64,
    pub content_hash: String,
    pub byte_size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_path: Option<String>,
    pub source: String,
    pub created_by: String,
    pub created_at: String,
}

/// An FTS5 hit — a snippet, never the whole document.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSearchHit {
    pub artifact_id: String,
    pub change_id: String,
    pub change_name: String,
    pub project: String,
    pub kind: String,
    pub capability: String,
    pub snippet: String,
}

// ── The living specification ────────────────────────────────────────────────
//
// `openspec/specs/{capability}/spec.md` — the SOURCE OF TRUTH, as opposed to the
// in-flight drafts under `openspec/changes/{name}/`.
//
// A main spec is NOT an artifact of a change. It belongs to the PROJECT and it
// outlives the changes that modify it, so it is a root entity (`SddSpec`), not an
// `SddArtifact` hanging off a synthetic change — which would invert the
// relationship. `sdd-archive` merges a closing change's delta specs into it, and
// `SddSpecRevision::merged_from_change_id` records which change did so.

/// One living specification — one `openspec/specs/{capability}/spec.md`.
/// Carries NO content: content lives in revisions and is fetched explicitly.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSpec {
    pub id: String,
    pub org_id: String,
    pub project: String,
    /// The `{capability}` directory name. Unique per (org, project) — one contract
    /// per capability is what makes it the contract.
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub latest_revision: i64,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// The change whose deltas produced the LATEST revision. Hydrated from that
    /// revision — it is what makes a spec's history traceable back to the changes
    /// that shaped it, and it is metadata, so list reads carry it too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_merged_from_change_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_merged_from_change_name: Option<String>,
}

/// A spec plus the content of its latest revision.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSpecDetail {
    #[serde(flatten)]
    pub spec: SddSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// A spec a given change has merged into, and the revision that merge produced.
/// Backs `GET /v1/sdd/changes/:id/specs`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSpecMerge {
    #[serde(flatten)]
    pub spec: SddSpec,
    /// The revision OF THIS SPEC that the change produced (its most recent, if the
    /// change merged into the spec more than once).
    pub merged_revision: i64,
}

/// A full, immutable spec revision — with content.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSpecRevision {
    pub id: String,
    pub spec_id: String,
    pub revision: i64,
    pub content: String,
    pub content_hash: String,
    pub byte_size: i64,
    /// WHICH change merged its deltas to produce this revision. `None` for a
    /// revision written outside the change pipeline (an import, an admin edit) —
    /// and for one whose change was later purged (`ON DELETE SET NULL`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_from_change_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_from_change_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_path: Option<String>,
    pub source: String,
    pub created_by: String,
    pub created_at: String,
}

/// Spec revision metadata. **Has no `content` field on purpose** — same contract as
/// `SddRevisionMeta`: the list endpoint physically cannot leak a 117-line document.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSpecRevisionMeta {
    pub id: String,
    pub spec_id: String,
    pub revision: i64,
    pub content_hash: String,
    pub byte_size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_from_change_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_from_change_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_path: Option<String>,
    pub source: String,
    pub created_by: String,
    pub created_at: String,
}

/// Thin projection for `GlobalSearchResult` — additive facet, no content.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSpecSummary {
    pub id: String,
    pub project: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub latest_revision: i64,
}

/// An FTS5 hit over the specs tree — a snippet, never the whole contract.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSpecSearchHit {
    pub spec_id: String,
    pub project: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub snippet: String,
}

/// One hit from `GET /v1/sdd/search`, which spans BOTH trees.
///
/// `hit_type` is the discriminator and is never absent: a caller asking "which spec
/// covers rate limiting?" must be able to tell the CONTRACT
/// (`openspec/specs/{capability}/spec.md`) from a draft inside some change, because
/// those two answers mean very different things. The tree-specific ids are
/// `Option` for exactly that reason — a spec hit has no `change_id`, and pretending
/// otherwise would be a lie with a plausible shape.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SddSearchResult {
    /// `"spec"` or `"artifact"`.
    pub hit_type: String,
    pub project: String,
    pub capability: String,
    pub snippet: String,
    /// Artifact hits only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Spec hits only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl SddSearchResult {
    pub fn from_artifact(hit: SddSearchHit) -> Self {
        Self {
            hit_type: "artifact".to_string(),
            project: hit.project,
            capability: hit.capability,
            snippet: hit.snippet,
            artifact_id: Some(hit.artifact_id),
            change_id: Some(hit.change_id),
            change_name: Some(hit.change_name),
            kind: Some(hit.kind),
            spec_id: None,
            title: None,
        }
    }

    pub fn from_spec(hit: SddSpecSearchHit) -> Self {
        Self {
            hit_type: "spec".to_string(),
            project: hit.project,
            capability: hit.capability,
            snippet: hit.snippet,
            artifact_id: None,
            change_id: None,
            change_name: None,
            kind: None,
            spec_id: Some(hit.spec_id),
            title: hit.title,
        }
    }
}

/// Body for `PUT /v1/sdd/specs` — the workhorse. Idempotent by content hash:
/// re-saving identical content creates no revision.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SaveSpecRequest {
    pub project: String,
    pub capability: String,
    pub content: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    /// The change whose deltas this revision merges. Resolved to a `change_id`
    /// against `(org, project, name)`. A name that resolves to nothing is a 404,
    /// not a silently-NULL provenance: the traceability IS the feature.
    #[serde(default)]
    pub merged_from_change_name: Option<String>,
    #[serde(default)]
    pub git_commit: Option<String>,
    /// `agent` (default), `admin`, or `import`.
    #[serde(default)]
    pub source: Option<String>,
}

/// Query filters for `GET /v1/sdd/specs`.
#[derive(Debug, Clone, Default)]
pub struct SddSpecFilters {
    pub project: Option<String>,
    pub include_archived: bool,
}

/// Body for `POST /v1/sdd/changes`. Upserts by `(org_id, project, name)`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpsertChangeRequest {
    pub project: String,
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub repo_ref: Option<String>,
    #[serde(default)]
    pub sprint_id: Option<String>,
}

/// Body for `PATCH /v1/sdd/changes/:id`.
///
/// **The identity tuple `(project, name)` is deliberately absent.** A change's
/// identity is not patchable: renaming it would silently orphan every
/// `task_spec_links.spec_change_name` row that points at the old name, since
/// tasks join by name, not by FK. Move/rename is a delete-and-recreate.
///
/// `deny_unknown_fields` makes that refusal *loud*: a caller who sends
/// `{"project": "other"}` gets a 422, not a silent no-op that leaves them
/// believing the rename landed.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct PatchChangeRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub sprint_id: Option<String>,
}

/// Body for `PUT /v1/sdd/artifacts` — the workhorse. Idempotent by content hash:
/// re-saving identical content creates no revision.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SaveArtifactRequest {
    pub project: String,
    pub change_name: String,
    pub kind: String,
    /// Omitted for every kind but `spec`; normalized to `""` on write.
    #[serde(default)]
    pub capability: Option<String>,
    pub content: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub git_commit: Option<String>,
    #[serde(default)]
    pub git_ref: Option<String>,
    /// `agent` (default), `admin`, or `import`.
    #[serde(default)]
    pub source: Option<String>,
}

/// Body for `POST /v1/sdd/changes/:id/memories`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LinkChangeMemoryRequest {
    pub memory_id: String,
    /// `produced` (default) or `informed`.
    #[serde(default)]
    pub relation: Option<String>,
}

/// Query filters for `GET /v1/sdd/changes`.
#[derive(Debug, Clone, Default)]
pub struct SddChangeFilters {
    pub project: Option<String>,
    pub status: Option<String>,
    pub phase: Option<String>,
    pub sprint_id: Option<String>,
    pub include_archived: bool,
}

/// Denormalized assignee display — mirrors `HarnessOwner` exactly (joined from `users`).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TaskAssignee {
    pub id: String,
    pub name: String,
    pub email: String,
}

/// Core task entity (DB row + hydrated relations on detail reads).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Task {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprint_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub assignees: Vec<TaskAssignee>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub comment_count: i64,
    #[serde(default)]
    pub spec_links: Vec<String>,
    #[serde(default)]
    pub subtask_count: i64,
}

/// Threaded comment on a task (flat, chronologically-ordered per task).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TaskComment {
    pub id: String,
    pub task_id: String,
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    pub body: String,
    pub created_at: String,
}

/// A sprint groups tasks (grouping only, no burndown, one sprint per task).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Sprint {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    pub status: String,
    pub created_by: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub task_count: i64,
}

/// A retrospective note for a sprint.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SprintRetrospective {
    pub id: String,
    pub sprint_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub went_well: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub went_wrong: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_items: Option<String>,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    pub created_at: String,
}

/// Request body for `POST /v1/tasks`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CreateTaskRequest {
    pub project: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub sprint_id: Option<String>,
}

/// Request body for `PATCH /v1/tasks/:id`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PatchTaskRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub sprint_id: Option<String>,
}

/// Request body for `POST /v1/tasks/:id/assignees`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AssignTaskRequest {
    pub user_ids: Vec<String>,
}

/// Request body for `POST /v1/tasks/:id/labels`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddLabelRequest {
    pub label: String,
}

/// Request body for `POST /v1/tasks/:id/comments`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddCommentRequest {
    pub body: String,
}

/// Request body for `POST /v1/tasks/:id/spec-links`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LinkSpecRequest {
    pub spec_change_name: String,
}

/// Request body for `POST /v1/tasks/resolve-by-spec`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResolveBySpecRequest {
    pub spec_change_name: String,
}

/// Request body for `POST /v1/sprints`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CreateSprintRequest {
    pub project: String,
    pub name: String,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
}

/// Request body for `PATCH /v1/sprints/:id`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PatchSprintRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Request body for `POST /v1/sprints/:id/retrospectives`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CreateRetrospectiveRequest {
    #[serde(default)]
    pub went_well: Option<String>,
    #[serde(default)]
    pub went_wrong: Option<String>,
    #[serde(default)]
    pub action_items: Option<String>,
}

// ── Knowledge migration (v60) ────────────────────────────────────────────────

/// Where a staged candidate is headed once a human approves it.
///
/// The set is closed on purpose and mirrors the CHECK on
/// `migration_candidates.destination_kind`. Adding a destination means a
/// migration, not just a new enum arm — the database is the authority.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DestinationKind {
    Memory,
    Convention,
    Task,
    SddArtifact,
    Harness,
    HarnessConfigReview,
}

impl DestinationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DestinationKind::Memory => "memory",
            DestinationKind::Convention => "convention",
            DestinationKind::Task => "task",
            DestinationKind::SddArtifact => "sdd_artifact",
            DestinationKind::Harness => "harness",
            DestinationKind::HarnessConfigReview => "harness_config_review",
        }
    }
}

impl FromStr for DestinationKind {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "memory" => Ok(DestinationKind::Memory),
            "convention" => Ok(DestinationKind::Convention),
            "task" => Ok(DestinationKind::Task),
            "sdd_artifact" => Ok(DestinationKind::SddArtifact),
            "harness" => Ok(DestinationKind::Harness),
            "harness_config_review" => Ok(DestinationKind::HarnessConfigReview),
            _ => Err("unsupported_destination_kind"),
        }
    }
}

/// Which connector produced a run. `Noop` exists so the pipeline is testable in
/// CI without a filesystem, a database, or a model CLI on the runner.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    RepoDocs,
    GitHistory,
    ClaudeMemories,
    DbSchema,
    SourceCode,
    Noop,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::RepoDocs => "repo-docs",
            SourceKind::GitHistory => "git-history",
            SourceKind::ClaudeMemories => "claude-memories",
            SourceKind::DbSchema => "db-schema",
            SourceKind::SourceCode => "source-code",
            SourceKind::Noop => "noop",
        }
    }
}

impl FromStr for SourceKind {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "repo-docs" => Ok(SourceKind::RepoDocs),
            "git-history" => Ok(SourceKind::GitHistory),
            "claude-memories" => Ok(SourceKind::ClaudeMemories),
            "db-schema" => Ok(SourceKind::DbSchema),
            "source-code" => Ok(SourceKind::SourceCode),
            "noop" => Ok(SourceKind::Noop),
            _ => Err("unsupported_source_kind"),
        }
    }
}

/// One migration run. `client_id`, `project_id` and `source_kind` are immutable
/// after creation — enforced by a trigger, not by convention.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MigrationRun {
    pub id: String,
    pub org_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub source_kind: SourceKind,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_version: Option<String>,
    pub attestation: serde_json::Value,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

/// One staged candidate awaiting a human decision.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MigrationCandidate {
    pub id: String,
    pub run_id: String,
    pub source_identity: String,
    pub destination_kind: DestinationKind,
    pub destination_hint: serde_json::Value,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub normalized_metadata: serde_json::Value,
    pub attestation: serde_json::Value,
    pub provenance_kind: String,
    pub status: String,
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Request body for `POST /v1/migrations`.
#[derive(Debug, Deserialize, Clone)]
pub struct CreateMigrationRunRequest {
    pub source_kind: SourceKind,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub runner_version: Option<String>,
    #[serde(default)]
    pub attestation: Option<serde_json::Value>,
}

/// One candidate as submitted by a connector.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CandidateInput {
    pub source_identity: String,
    pub destination_kind: DestinationKind,
    pub content: String,
    #[serde(default)]
    pub destination_hint: serde_json::Value,
    #[serde(default)]
    pub source_excerpt: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub normalized_metadata: serde_json::Value,
    #[serde(default)]
    pub provenance_kind: Option<String>,
}

/// Request body for `POST /v1/migrations/:id/candidates`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StageCandidatesRequest {
    pub candidates: Vec<CandidateInput>,
}

/// Per-candidate outcome of a staging call. A malformed candidate is reported,
/// never fatal: one bad row must not discard the other four hundred.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "result")]
pub enum StageResult {
    Staged { id: String },
    Skipped { reason: String },
    Rejected { reason: String },
}

/// What a reviewer decided.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approved,
    Rejected,
    Restaged,
}

/// Request body for `POST /v1/migrations/:id/review`.
///
/// `expected_version` is deliberately NOT optional. Making it optional would
/// invite callers to omit it, and optimistic concurrency would silently stop
/// working exactly when it matters: two reviewers on the same queue.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReviewActionRequest {
    pub candidate_id: String,
    pub action: ReviewVerdict,
    pub expected_version: i64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub request_correlation_id: Option<String>,
}

/// Counts plus a reason for every candidate that did not reach its destination.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RunReport {
    pub run_id: String,
    pub staged: usize,
    pub approved: usize,
    pub rejected: usize,
    pub committed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub pending_index: usize,
    pub outcomes: Vec<RunReportEntry>,
}

/// One non-committed candidate and why it is not committed.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunReportEntry {
    pub candidate_id: String,
    pub source_identity: String,
    pub destination_kind: DestinationKind,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest_file_component(
        kind: &str,
        path: &str,
        media_type: &str,
        content: &str,
    ) -> serde_json::Value {
        json!({
            "kind": kind,
            "path": path,
            "media_type": media_type,
            "size_bytes": content_size_bytes(content),
            "sha256": expected_content_sha256(content),
            "content": content,
        })
    }

    #[test]
    fn org_roundtrip() {
        let org = Org {
            id: "org1".into(),
            name: "Acme Corp".into(),
            slug: "acme".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&org).unwrap();
        let back: Org = serde_json::from_str(&json).unwrap();
        assert_eq!(org, back);
    }

    #[test]
    fn auth_context_has_required_fields() {
        let ctx = AuthContext {
            org_id: "org1".into(),
            user_id: "u1".into(),
            role: UserRole::Standard(Role::Admin),
        };
        assert_eq!(ctx.org_id, "org1");
        assert_eq!(ctx.role, UserRole::Standard(Role::Admin));
    }

    // ── Role tests ────────────────────────────────────────────────────────────

    #[test]
    fn role_from_str_valid_values() {
        assert_eq!("admin".parse::<Role>().unwrap(), Role::Admin);
        assert_eq!("member".parse::<Role>().unwrap(), Role::Member);
        assert_eq!("viewer".parse::<Role>().unwrap(), Role::Viewer);
    }

    #[test]
    fn role_from_str_unknown_returns_err() {
        assert!("superuser".parse::<Role>().is_err());
        assert!("".parse::<Role>().is_err());
        assert!(
            "Admin".parse::<Role>().is_err(),
            "case-sensitive: uppercase must fail"
        );
    }

    #[test]
    fn role_display() {
        assert_eq!(Role::Admin.to_string(), "admin");
        assert_eq!(Role::Member.to_string(), "member");
        assert_eq!(Role::Viewer.to_string(), "viewer");
    }

    #[test]
    fn role_as_u8_ordering() {
        assert!(Role::Admin.as_u8() > Role::Member.as_u8());
        assert!(Role::Member.as_u8() > Role::Viewer.as_u8());
        assert_eq!(Role::Admin.as_u8(), 2);
        assert_eq!(Role::Member.as_u8(), 1);
        assert_eq!(Role::Viewer.as_u8(), 0);
    }

    #[test]
    fn memory_tags_default_empty() {
        let m = Memory {
            id: "m1".into(),
            org_id: "org1".into(),
            user_id: "u1".into(),
            project: "default".into(),
            tool: "claude".into(),
            content: "use snake_case".into(),
            tags: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
            title: None,
            memory_type: None,
            scope: "project".into(),
            topic_key: None,
            session_id: None,
            revision_count: 1,
            normalized_hash: None,
            project_id: None,
            archived_at: None,
            pinned: false,
            collection_id: None,
            admin_note: None,
            delete_after: None,
            status: "active".to_string(),
        };
        assert!(m.tags.is_empty());
        assert_eq!(m.scope, "project");
        assert_eq!(m.revision_count, 1);
    }

    #[test]
    fn audit_entry_optional_resource_id() {
        let entry = AuditEntry {
            id: "a1".into(),
            org_id: "org1".into(),
            user_id: "u1".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            action: "search".into(),
            resource_type: "memory".into(),
            resource_id: None,
            metadata: json!({}),
            previous_hash: None,
            current_hash: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert!(back.resource_id.is_none());
    }

    /// The scanner refused to publish real harnesses because their prose contained the
    /// words `task-specific` and `ask-on-risk`. Both hold the substring `sk-`, which the
    /// check looked for unanchored, aiming at OpenAI keys.
    ///
    /// Refusing a publish is a serious act. It has to be for a secret, not for English.
    #[test]
    fn secret_scan_does_not_reject_ordinary_words_containing_sk() {
        for prose in [
            "Read those files before task-specific work.",
            "delivery strategy: ask-on-risk (default)",
            "a risk-based forecast of disk-backed storage",
            "sk- on its own is not a key",
            "sk-short",
        ] {
            assert!(
                validate_safe_manifest_content(prose).is_ok(),
                "the scanner must not reject prose: {prose:?}"
            );
        }
    }

    /// …but a real key must still be caught, boundary and all.
    #[test]
    fn secret_scan_still_catches_a_real_openai_key() {
        for secret in [
            "OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz0123456789",
            "use sk-proj-Aa1Bb2Cc3Dd4Ee5Ff6Gg7Hh8Ii9Jj0 to authenticate",
            "sk-0123456789012345678901234567890123456789",
        ] {
            assert_eq!(
                validate_safe_manifest_content(secret),
                Err("secret_scan_failed"),
                "a real key must still be refused: {secret:?}"
            );
        }

        // The other patterns are unchanged.
        assert_eq!(
            validate_safe_manifest_content("ghp_0123456789abcdef"),
            Err("secret_scan_failed")
        );
        assert_eq!(
            validate_safe_manifest_content("Authorization: Bearer x"),
            Err("secret_scan_failed")
        );
        assert_eq!(
            validate_safe_manifest_content("/Users/cesar/.ssh/id_rsa"),
            Err("secret_scan_failed")
        );
    }

    #[test]
    fn typed_harness_manifest_accepts_all_supported_formats() {
        for manifest in [
            json!({ "schema_version": "1.1", "format": "agent", "targets": ["claude"], "components": [manifest_file_component("file", "agents/reviewer.md", "text/markdown", "# Agent")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } }),
            json!({ "schema_version": "1.1", "format": "skill", "targets": ["claude"], "components": [{ "kind": "folder", "path": "skills/reviewer", "entries": [manifest_file_component("file", "skills/reviewer/SKILL.md", "text/markdown", "---\nname: reviewer\n---")] }], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } }),
            json!({ "schema_version": "1.1", "format": "command", "targets": ["claude"], "components": [manifest_file_component("file", "commands/review.md", "text/markdown", "Review this")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } }),
            json!({ "schema_version": "1.1", "format": "hook", "targets": ["claude"], "components": [manifest_file_component("file", "hooks/pre-commit.sh", "text/x-shellscript", "#!/bin/sh\nexit 0")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "executable": true, "secret_scan_status": "passed" } }),
            json!({ "schema_version": "1.1", "format": "output_style", "targets": ["claude"], "components": [manifest_file_component("file", "output-styles/direct.md", "text/markdown", "Be direct")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } }),
            json!({ "schema_version": "1.1", "format": "claude_code_plugin", "targets": ["claude"], "components": [manifest_file_component("plugin_marketplace", "plugins/reviewer.json", "application/json", "{\"name\":\"reviewer\"}")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "executable": true, "secret_scan_status": "passed" } }),
            json!({ "schema_version": "1.1", "format": "theme", "targets": ["claude"], "components": [manifest_file_component("theme_json", "themes/dark.json", "application/json", "{\"name\":\"Dark\"}")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } }),
        ] {
            validate_typed_harness_manifest(&manifest).expect("supported manifest should validate");
        }
    }

    #[test]
    fn typed_harness_manifest_rejects_mismatched_or_unsafe_structures() {
        let mismatched = json!({ "schema_version": "1.1", "format": "theme", "targets": ["claude"], "components": [manifest_file_component("file", "hooks/run.sh", "text/x-shellscript", "#!/bin/sh")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } });
        let unsafe_path = json!({ "schema_version": "1.1", "format": "agent", "targets": ["claude"], "components": [manifest_file_component("file", "/Users/me/.claude/agent.md", "text/markdown", "# Agent")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } });
        let windows_absolute_path = json!({ "schema_version": "1.1", "format": "agent", "targets": ["claude"], "components": [manifest_file_component("file", "C:\\Users\\me\\.claude\\agent.md", "text/markdown", "# Agent")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } });
        let unsafe_content = json!({ "schema_version": "1.1", "format": "agent", "targets": ["claude"], "components": [manifest_file_component("file", "agents/reviewer.md", "text/markdown", "token nm_live_secret")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } });

        assert_eq!(
            validate_typed_harness_manifest(&mismatched),
            Err("format_component_mismatch")
        );
        assert_eq!(
            validate_typed_harness_manifest(&unsafe_path),
            Err("unsafe_component_path")
        );
        assert_eq!(
            validate_typed_harness_manifest(&windows_absolute_path),
            Err("unsafe_component_path")
        );
        assert_eq!(
            validate_typed_harness_manifest(&unsafe_content),
            Err("secret_scan_failed")
        );
    }

    #[test]
    fn typed_harness_manifest_accepts_cursor_and_rejects_opencode_target() {
        let cursor_manifest = json!({ "schema_version": "1.1", "format": "agent", "targets": ["cursor"], "components": [manifest_file_component("file", "agents/reviewer.md", "text/markdown", "# Agent")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } });
        let opencode_manifest = json!({ "schema_version": "1.1", "format": "agent", "targets": ["opencode"], "components": [manifest_file_component("file", "agents/reviewer.md", "text/markdown", "# Agent")], "provenance": { "source": "admin-ui" }, "security": { "requires_approval": true, "secret_scan_status": "passed" } });

        validate_typed_harness_manifest(&cursor_manifest).expect("cursor should be a valid target");
        assert_eq!(
            validate_typed_harness_manifest(&opencode_manifest),
            Err("missing_targets")
        );
    }

    #[test]
    fn typed_harness_manifest_rejects_fake_or_mismatched_integrity_metadata() {
        let fake_hash = json!({
            "schema_version": "1.1",
            "format": "agent",
            "targets": ["claude"],
            "components": [{
                "kind": "file",
                "path": "agents/reviewer.md",
                "media_type": "text/markdown",
                "size_bytes": content_size_bytes("# Agent"),
                "sha256": "sha256:template",
                "content": "# Agent"
            }],
            "provenance": { "source": "admin-ui" },
            "security": { "requires_approval": true, "secret_scan_status": "passed" }
        });
        let wrong_size = json!({
            "schema_version": "1.1",
            "format": "theme",
            "targets": ["claude"],
            "components": [{
                "kind": "theme_json",
                "path": "themes/utf8.json",
                "media_type": "application/json",
                "size_bytes": content_size_bytes("{\"name\":\"Café ☕\"}") - 1,
                "sha256": expected_content_sha256("{\"name\":\"Café ☕\"}"),
                "content": "{\"name\":\"Café ☕\"}"
            }],
            "provenance": { "source": "admin-ui" },
            "security": { "requires_approval": true, "secret_scan_status": "passed" }
        });
        let wrong_folder_entry = json!({
            "schema_version": "1.1",
            "format": "skill",
            "targets": ["claude"],
            "components": [{
                "kind": "folder",
                "path": "skills/reviewer",
                "entries": [{
                    "kind": "file",
                    "path": "skills/reviewer/SKILL.md",
                    "media_type": "text/markdown",
                    "size_bytes": content_size_bytes("---\nname: reviewer\n---"),
                    "sha256": "sha256:template",
                    "content": "---\nname: reviewer\n---"
                }]
            }],
            "provenance": { "source": "admin-ui" },
            "security": { "requires_approval": true, "secret_scan_status": "passed" }
        });

        assert_eq!(
            validate_typed_harness_manifest(&fake_hash),
            Err("component_integrity_mismatch")
        );
        assert_eq!(
            validate_typed_harness_manifest(&wrong_size),
            Err("component_integrity_mismatch")
        );
        assert_eq!(
            validate_typed_harness_manifest(&wrong_folder_entry),
            Err("component_integrity_mismatch")
        );
    }

    // ── T-04 tests ────────────────────────────────────────────────────────────

    #[test]
    fn audit_entry_serializes_hash_fields_as_null_when_missing() {
        // Simulates deserializing a pre-v9 row where both columns are NULL/missing.
        let json_str = r#"{
            "id": "a1",
            "org_id": "org1",
            "user_id": "u1",
            "timestamp": "2026-01-01T00:00:00Z",
            "action": "store",
            "resource_type": "memory",
            "resource_id": null,
            "metadata": {}
        }"#;
        let entry: AuditEntry = serde_json::from_str(json_str).unwrap();
        assert!(
            entry.previous_hash.is_none(),
            "previous_hash must default to None"
        );
        assert!(
            entry.current_hash.is_none(),
            "current_hash must default to None"
        );

        // And the serialized form must include both fields as null (not omit them).
        let out: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert_eq!(out["previous_hash"], serde_json::Value::Null);
        assert_eq!(out["current_hash"], serde_json::Value::Null);
    }

    #[test]
    fn audit_entry_hash_fields_round_trip() {
        let entry = AuditEntry {
            id: "a2".into(),
            org_id: "org1".into(),
            user_id: "u1".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            action: "store".into(),
            resource_type: "memory".into(),
            resource_id: Some("m1".into()),
            metadata: json!({}),
            previous_hash: Some("abc123".into()),
            current_hash: Some("def456".into()),
        };
        let json_str = serde_json::to_string(&entry).unwrap();
        let back: AuditEntry = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.previous_hash.as_deref(), Some("abc123"));
        assert_eq!(back.current_hash.as_deref(), Some("def456"));
    }

    // ── v2 struct tests ───────────────────────────────────────────────────────

    #[test]
    fn store_memory_request_deserializes_v2_optional_fields() {
        // Full v2 request with all new fields
        let json_str = r#"{
            "tool": "claude",
            "content": "use snake_case",
            "title": "Convention: naming",
            "type": "decision",
            "scope": "personal",
            "topic_key": "arch/naming",
            "session_id": "s1"
        }"#;
        let req: StoreMemoryRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.title.as_deref(), Some("Convention: naming"));
        assert_eq!(req.memory_type.as_deref(), Some("decision"));
        assert_eq!(req.scope.as_deref(), Some("personal"));
        assert_eq!(req.topic_key.as_deref(), Some("arch/naming"));
        assert_eq!(req.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn store_memory_request_legacy_fields_still_work() {
        // Legacy request — no v2 fields
        let json_str = r#"{"tool": "claude", "content": "use anyhow"}"#;
        let req: StoreMemoryRequest = serde_json::from_str(json_str).unwrap();
        assert!(req.title.is_none());
        assert!(req.memory_type.is_none());
        assert!(req.topic_key.is_none());
        assert!(req.session_id.is_none());
        // scope should default to None (handler applies the "project" default)
        assert!(req.scope.is_none());
    }

    #[test]
    fn memory_v2_fields_serialize_correctly() {
        let m = Memory {
            id: "m1".into(),
            org_id: "org1".into(),
            user_id: "u1".into(),
            project: "p".into(),
            tool: "claude".into(),
            content: "content".into(),
            tags: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
            title: Some("My title".into()),
            memory_type: Some("bugfix".into()),
            scope: "project".into(),
            topic_key: Some("k1".into()),
            session_id: None,
            revision_count: 2,
            normalized_hash: Some("abc123".into()),
            project_id: Some("proj_1".into()),
            archived_at: None,
            pinned: false,
            collection_id: None,
            admin_note: None,
            delete_after: None,
            status: "active".to_string(),
        };
        let json_val: serde_json::Value = serde_json::to_value(&m).unwrap();
        assert_eq!(json_val["title"], "My title");
        assert_eq!(json_val["type"], "bugfix");
        assert_eq!(json_val["scope"], "project");
        assert_eq!(json_val["revision_count"], 2);
    }

    #[test]
    fn session_struct_roundtrip() {
        let s = Session {
            id: "s1".into(),
            org_id: "org1".into(),
            name: None,
            project: "nexusmind".into(),
            directory: "/home/user".into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            ended_at: Some("2026-01-01T01:00:00Z".into()),
            summary: Some("Done".into()),
        };
        let json_str = serde_json::to_string(&s).unwrap();
        let back: Session = serde_json::from_str(&json_str).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn create_session_request_project_required() {
        let with_project: Result<CreateSessionRequest, _> =
            serde_json::from_str(r#"{"project": "nexusmind"}"#);
        assert!(with_project.is_ok());

        let without_project: Result<CreateSessionRequest, _> =
            serde_json::from_str(r#"{"directory": "/tmp"}"#);
        assert!(without_project.is_err(), "project is required");
    }

    #[test]
    fn patch_session_request_optional_fields() {
        let req: PatchSessionRequest =
            serde_json::from_str(r#"{"ended_at": "2026-01-01T01:00:00Z", "summary": "Done"}"#)
                .unwrap();
        assert_eq!(req.ended_at.as_deref(), Some("2026-01-01T01:00:00Z"));
        assert_eq!(req.summary.as_deref(), Some("Done"));

        let empty: PatchSessionRequest = serde_json::from_str("{}").unwrap();
        assert!(empty.ended_at.is_none());
        assert!(empty.summary.is_none());
    }

    // ── Policy type tests ─────────────────────────────────────────────────────

    #[test]
    fn policy_roundtrip() {
        let p = Policy {
            id: "p_abc".into(),
            org_id: "org1".into(),
            name: "No GPT".into(),
            rule_type: "model_whitelist".into(),
            config: json!({"allowed_models": ["claude-3-5-sonnet"]}),
            enabled: true,
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-01T00:00:00.000Z".into(),
            project_id: None,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: Policy = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
        assert_eq!(back.rule_type, "model_whitelist");
        assert!(back.enabled);
    }

    #[test]
    fn policy_with_project_id_roundtrip() {
        let p = Policy {
            id: "p_abc".into(),
            org_id: "org1".into(),
            name: "Project Scoped".into(),
            rule_type: "model_whitelist".into(),
            config: json!({"allowed_models": ["claude-3-5-sonnet"]}),
            enabled: true,
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-01T00:00:00.000Z".into(),
            project_id: Some("proj_1".into()),
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: Policy = serde_json::from_str(&s).unwrap();
        assert_eq!(back.project_id.as_deref(), Some("proj_1"));
    }

    #[test]
    fn create_policy_request_project_id_defaults_to_none() {
        let json_str = r#"{
            "name": "Whitelist only claude",
            "rule_type": "model_whitelist",
            "config": {"allowed_models": ["claude-3-5-sonnet"]}
        }"#;
        let req: CreatePolicyRequest = serde_json::from_str(json_str).unwrap();
        assert!(req.project_id.is_none());
    }

    #[test]
    fn create_policy_request_project_id_roundtrip() {
        let json_str = r#"{
            "name": "Scoped policy",
            "rule_type": "model_whitelist",
            "config": {"allowed_models": ["claude-3-5-sonnet"]},
            "project_id": "proj_1"
        }"#;
        let req: CreatePolicyRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.project_id.as_deref(), Some("proj_1"));
    }

    #[test]
    fn create_policy_request_model_whitelist_roundtrip() {
        let json_str = r#"{
            "name": "Whitelist only claude",
            "rule_type": "model_whitelist",
            "config": {"allowed_models": ["claude-3-5-sonnet", "claude-3-haiku"]},
            "enabled": true
        }"#;
        let req: CreatePolicyRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.name, "Whitelist only claude");
        assert!(req.enabled);
        match &req.config {
            PolicyConfig::ModelWhitelist { allowed_models } => {
                assert_eq!(allowed_models.len(), 2);
                assert_eq!(allowed_models[0], "claude-3-5-sonnet");
            }
            other => panic!("expected ModelWhitelist, got {:?}", other),
        }
    }

    #[test]
    fn create_policy_request_budget_limit_roundtrip() {
        let json_str = r#"{
            "name": "Daily cap",
            "rule_type": "budget_limit",
            "config": {"max_tokens_per_day": 100000, "max_requests_per_day": 500}
        }"#;
        let req: CreatePolicyRequest = serde_json::from_str(json_str).unwrap();
        assert!(req.enabled, "default_enabled should be true");
        match &req.config {
            PolicyConfig::BudgetLimit {
                max_tokens_per_day,
                max_requests_per_day,
            } => {
                assert_eq!(*max_tokens_per_day, Some(100000));
                assert_eq!(*max_requests_per_day, Some(500));
            }
            other => panic!("expected BudgetLimit, got {:?}", other),
        }
    }

    #[test]
    fn create_policy_request_pii_redact_roundtrip() {
        let json_str = r#"{
            "name": "PII guard",
            "rule_type": "pii_redact",
            "config": {"patterns": ["\\d{3}-\\d{2}-\\d{4}"]}
        }"#;
        let req: CreatePolicyRequest = serde_json::from_str(json_str).unwrap();
        match &req.config {
            PolicyConfig::PiiRedact { patterns } => {
                assert_eq!(patterns.len(), 1);
            }
            other => panic!("expected PiiRedact, got {:?}", other),
        }
    }

    #[test]
    fn update_policy_request_all_optional() {
        let empty: UpdatePolicyRequest = serde_json::from_str("{}").unwrap();
        assert!(empty.name.is_none());
        assert!(empty.config.is_none());
        assert!(empty.enabled.is_none());

        let partial: UpdatePolicyRequest = serde_json::from_str(r#"{"enabled": false}"#).unwrap();
        assert!(partial.name.is_none());
        assert_eq!(partial.enabled, Some(false));
    }

    #[test]
    fn policy_check_request_roundtrip() {
        let json_str = r#"{
            "model": "gpt-4o",
            "prompt_tokens": 512,
            "prompt_preview": "What is the capital?",
            "user_id": "u1",
            "project": "my-project"
        }"#;
        let req: PolicyCheckRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.model, "gpt-4o");
        assert_eq!(req.prompt_tokens, Some(512));
        assert_eq!(req.prompt_preview.as_deref(), Some("What is the capital?"));
        assert_eq!(req.user_id.as_deref(), Some("u1"));

        // Re-serialize and deserialize
        let s = serde_json::to_string(&req).unwrap();
        let back: PolicyCheckRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.model, "gpt-4o");
    }

    #[test]
    fn policy_check_response_allowed_no_violations() {
        let resp = PolicyCheckResponse {
            allowed: true,
            violations: vec![],
        };
        let s = serde_json::to_string(&resp).unwrap();
        let back: PolicyCheckResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(resp, back);
        assert!(back.violations.is_empty());
    }

    #[test]
    fn policy_check_response_denied_with_violation() {
        let resp = PolicyCheckResponse {
            allowed: false,
            violations: vec![PolicyViolation {
                policy_id: "p_abc".into(),
                policy_name: "No GPT".into(),
                rule_type: "model_whitelist".into(),
                reason: "Model 'gpt-4o' is not in the allowed list".into(),
            }],
        };
        let s = serde_json::to_string(&resp).unwrap();
        let back: PolicyCheckResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(back.violations.len(), 1);
        assert_eq!(back.violations[0].policy_id, "p_abc");
        assert!(!back.allowed);
    }

    // ── Code index type tests ─────────────────────────────────────────────────

    #[test]
    fn code_project_roundtrip() {
        let p = CodeProject {
            id: "cp1".into(),
            org_id: "org1".into(),
            name: "myapp".into(),
            root_path: "/workspace/myapp".into(),
            repo_url: Some("https://github.com/owner/myapp".into()),
            file_count: 10,
            chunk_count: 42,
            last_indexed: Some("2026-06-19T12:00:00Z".into()),
            created_at: "2026-06-19T12:00:00Z".into(),
            reindex_interval_hours: None,
            last_indexed_at: None,
            last_index_error: None,
            indexed_files_count: None,
            index_status: None,
            archived_at: None,
            exclude_patterns: vec![],
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: CodeProject = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
        assert_eq!(back.file_count, 10);
    }

    #[test]
    fn code_status_response_not_indexed_omits_optional_fields() {
        let resp = CodeStatusResponse {
            project: "ghost".into(),
            status: "not_indexed".into(),
            last_indexed: None,
            file_count: None,
            chunk_count: None,
        };
        let v: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["project"], "ghost");
        assert_eq!(v["status"], "not_indexed");
        // None fields must be absent (skip_serializing_if)
        assert!(
            v.get("last_indexed").is_none(),
            "last_indexed must be omitted when None"
        );
        assert!(
            v.get("file_count").is_none(),
            "file_count must be omitted when None"
        );
    }

    #[test]
    fn index_project_request_roundtrip() {
        let req = IndexProjectRequest {
            project: "myapp".into(),
            root_path: Some("/workspace/myapp".into()),
            repo_url: None,
            github_token: None,
            graph_only: None,
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: IndexProjectRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(req.project, back.project);
        assert_eq!(req.root_path, back.root_path);
    }

    #[test]
    fn search_code_request_top_k_optional() {
        let with_top_k: SearchCodeRequest =
            serde_json::from_str(r#"{"project":"p","query":"q","top_k":10}"#).unwrap();
        assert_eq!(with_top_k.top_k, Some(10));

        let without_top_k: SearchCodeRequest =
            serde_json::from_str(r#"{"project":"p","query":"q"}"#).unwrap();
        assert!(without_top_k.top_k.is_none());
    }

    // ── TaskStatus ────────────────────────────────────────────────────────────

    #[test]
    fn task_status_from_str_parses_all_valid_values() {
        assert_eq!(
            TaskStatus::from_str("backlog").unwrap(),
            TaskStatus::Backlog
        );
        assert_eq!(TaskStatus::from_str("todo").unwrap(), TaskStatus::Todo);
        assert_eq!(
            TaskStatus::from_str("in_progress").unwrap(),
            TaskStatus::InProgress
        );
        assert_eq!(
            TaskStatus::from_str("in_review").unwrap(),
            TaskStatus::InReview
        );
        assert_eq!(TaskStatus::from_str("done").unwrap(), TaskStatus::Done);
        assert_eq!(
            TaskStatus::from_str("cancelled").unwrap(),
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn task_status_from_str_rejects_unrecognized_value() {
        assert!(TaskStatus::from_str("bogus").is_err());
        assert!(TaskStatus::from_str("").is_err());
        assert!(
            TaskStatus::from_str("Backlog").is_err(),
            "must be case-sensitive snake_case"
        );
    }

    #[test]
    fn task_status_display_roundtrips_to_snake_case() {
        let cases = [
            (TaskStatus::Backlog, "backlog"),
            (TaskStatus::Todo, "todo"),
            (TaskStatus::InProgress, "in_progress"),
            (TaskStatus::InReview, "in_review"),
            (TaskStatus::Done, "done"),
            (TaskStatus::Cancelled, "cancelled"),
        ];
        for (variant, expected) in cases {
            assert_eq!(variant.to_string(), expected);
            assert_eq!(TaskStatus::from_str(expected).unwrap(), variant);
        }
    }

    #[test]
    fn can_transition_matches_design_matrix() {
        use TaskStatus::*;
        let allowed: &[(TaskStatus, TaskStatus)] = &[
            (Backlog, Todo),
            (Backlog, InProgress),
            (Backlog, Cancelled),
            (Todo, Backlog),
            (Todo, InProgress),
            (Todo, Cancelled),
            (InProgress, Backlog),
            (InProgress, Todo),
            (InProgress, InReview),
            (InProgress, Done),
            (InProgress, Cancelled),
            (InReview, InProgress),
            (InReview, Done),
            (InReview, Cancelled),
            (Done, InProgress),
            (Cancelled, Backlog),
        ];
        for (from, to) in allowed {
            assert!(can_transition(*from, *to), "{from} -> {to} must be allowed");
        }

        let all = [Backlog, Todo, InProgress, InReview, Done, Cancelled];
        for from in all {
            for to in all {
                if from == to {
                    assert!(
                        can_transition(from, to),
                        "{from} -> {to} same-state no-op must be allowed"
                    );
                    continue;
                }
                let expected_allowed = allowed.contains(&(from, to));
                assert_eq!(
                    can_transition(from, to),
                    expected_allowed,
                    "{from} -> {to} must be {}",
                    if expected_allowed {
                        "allowed"
                    } else {
                        "rejected"
                    }
                );
            }
        }
    }

    #[test]
    fn can_transition_rejects_illegal_edges() {
        use TaskStatus::*;
        assert!(
            !can_transition(Done, Todo),
            "done cannot revert directly to todo"
        );
        assert!(
            !can_transition(Done, Backlog),
            "done cannot revert directly to backlog"
        );
        assert!(
            !can_transition(Cancelled, Todo),
            "cancelled can only reopen to backlog"
        );
        assert!(
            !can_transition(Cancelled, InProgress),
            "cancelled can only reopen to backlog"
        );
        assert!(
            !can_transition(Backlog, InReview),
            "backlog cannot jump straight to in_review"
        );
        assert!(
            !can_transition(Backlog, Done),
            "backlog cannot jump straight to done"
        );
    }

    // ── Knowledge migration review state ───────────────────────────────────────

    #[test]
    fn migration_statuses_round_trip_and_reject_unknown_values() {
        let runs = [
            ("staging", MigrationRunStatus::Staging),
            ("in_review", MigrationRunStatus::InReview),
            ("committing", MigrationRunStatus::Committing),
            ("completed", MigrationRunStatus::Completed),
            ("cancelled", MigrationRunStatus::Cancelled),
        ];
        for (wire, status) in runs {
            assert_eq!(MigrationRunStatus::from_str(wire).unwrap(), status);
            assert_eq!(status.to_string(), wire);
        }
        assert!(MigrationRunStatus::from_str("draft").is_err());

        let candidates = [
            ("staged", MigrationCandidateStatus::Staged),
            ("approved", MigrationCandidateStatus::Approved),
            ("rejected", MigrationCandidateStatus::Rejected),
            ("committing", MigrationCandidateStatus::Committing),
            ("committed", MigrationCandidateStatus::Committed),
            ("skipped", MigrationCandidateStatus::Skipped),
            ("failed", MigrationCandidateStatus::Failed),
            ("cancelled", MigrationCandidateStatus::Cancelled),
        ];
        for (wire, status) in candidates {
            assert_eq!(MigrationCandidateStatus::from_str(wire).unwrap(), status);
            assert_eq!(status.to_string(), wire);
        }
        assert!(MigrationCandidateStatus::from_str("pending").is_err());
    }

    #[test]
    fn migration_transitions_preserve_terminal_states_and_restage_approval() {
        assert!(can_transition_migration_run(
            MigrationRunStatus::Staging,
            MigrationRunStatus::InReview
        ));
        assert!(can_transition_migration_run(
            MigrationRunStatus::InReview,
            MigrationRunStatus::Committing
        ));
        assert!(can_transition_migration_run(
            MigrationRunStatus::Committing,
            MigrationRunStatus::InReview
        ));
        assert!(!can_transition_migration_run(
            MigrationRunStatus::Completed,
            MigrationRunStatus::InReview
        ));
        assert!(!can_transition_migration_run(
            MigrationRunStatus::Cancelled,
            MigrationRunStatus::Staging
        ));

        assert!(can_transition_migration_candidate(
            MigrationCandidateStatus::Staged,
            MigrationCandidateStatus::Approved
        ));
        assert!(can_transition_migration_candidate(
            MigrationCandidateStatus::Approved,
            MigrationCandidateStatus::Staged
        ));
        assert!(can_transition_migration_candidate(
            MigrationCandidateStatus::Committing,
            MigrationCandidateStatus::Approved
        ));
        assert!(!can_transition_migration_candidate(
            MigrationCandidateStatus::Rejected,
            MigrationCandidateStatus::Approved
        ));
        assert!(!can_transition_migration_candidate(
            MigrationCandidateStatus::Committed,
            MigrationCandidateStatus::Staged
        ));
    }

    // ── SDD artifacts ───────────────────────────────────────────────────────

    /// 1.27 — every kind round-trips to its exact on-disk string.
    #[test]
    fn sdd_artifact_kind_round_trips() {
        let all = [
            ("exploration", SddArtifactKind::Exploration),
            ("proposal", SddArtifactKind::Proposal),
            ("spec", SddArtifactKind::Spec),
            ("design", SddArtifactKind::Design),
            ("tasks", SddArtifactKind::Tasks),
            ("apply-progress", SddArtifactKind::ApplyProgress),
            ("verify-report", SddArtifactKind::VerifyReport),
            ("archive-report", SddArtifactKind::ArchiveReport),
            ("state", SddArtifactKind::State),
        ];
        assert_eq!(all.len(), 9, "there are exactly 9 artifact kinds");

        for (s, kind) in all {
            assert_eq!(
                SddArtifactKind::from_str(s).unwrap(),
                kind,
                "{s} must parse"
            );
            assert_eq!(
                kind.to_string(),
                s,
                "{kind:?} must Display back to its on-disk string"
            );
        }

        assert!(
            SddArtifactKind::from_str("blueprint").is_err(),
            "an unrecognized kind must be rejected"
        );
        // The hyphenated kinds are the on-disk filenames — snake_case must NOT be accepted,
        // or the DB and the filesystem would disagree about the same artifact.
        assert!(
            SddArtifactKind::from_str("apply_progress").is_err(),
            "snake_case must not be accepted"
        );
    }

    /// 1.29 — phases round-trip, and rank() orders the DAG (used by the importer to
    /// infer the furthest phase present from an artifact inventory).
    #[test]
    fn sdd_phase_round_trips_and_ranks() {
        let ordered = [
            ("explore", SddPhase::Explore),
            ("propose", SddPhase::Propose),
            ("spec", SddPhase::Spec),
            ("design", SddPhase::Design),
            ("tasks", SddPhase::Tasks),
            ("apply", SddPhase::Apply),
            ("verify", SddPhase::Verify),
            ("archive", SddPhase::Archive),
        ];
        assert_eq!(ordered.len(), 8, "there are exactly 8 phases");

        for (s, phase) in ordered {
            assert_eq!(SddPhase::from_str(s).unwrap(), phase, "{s} must parse");
            assert_eq!(phase.to_string(), s, "{phase:?} must Display back");
        }
        assert!(
            SddPhase::from_str("refactor").is_err(),
            "an unknown phase must be rejected"
        );

        // rank() must be strictly increasing along the DAG.
        for pair in ordered.windows(2) {
            let (earlier, later) = (pair[0].1, pair[1].1);
            assert!(
                earlier.rank() < later.rank(),
                "{earlier:?} must rank before {later:?}"
            );
        }
    }

    /// 1.29 — the three change statuses.
    #[test]
    fn sdd_status_round_trips() {
        for (s, status) in [
            ("active", SddStatus::Active),
            ("archived", SddStatus::Archived),
            ("abandoned", SddStatus::Abandoned),
        ] {
            assert_eq!(SddStatus::from_str(s).unwrap(), status, "{s} must parse");
            assert_eq!(status.to_string(), s, "{status:?} must Display back");
        }
        assert!(
            SddStatus::from_str("draft").is_err(),
            "an unknown status must be rejected"
        );
    }

    /// 1.32 / 3.23 — the identity tuple is not patchable, and the refusal is LOUD.
    ///
    /// `deny_unknown_fields` turns `{"project": …}` into a deserialization error, which
    /// `AppJson` surfaces as a 422. Without it the field would be silently dropped and
    /// the caller would walk away believing a rename had landed when nothing moved.
    #[test]
    fn patch_change_request_cannot_touch_identity_fields() {
        let valid = serde_json::json!({ "title": "New title", "phase": "design" });
        let patch: PatchChangeRequest = serde_json::from_value(valid).unwrap();
        assert_eq!(patch.title.as_deref(), Some("New title"));
        assert_eq!(patch.phase.as_deref(), Some("design"));

        for identity_field in ["project", "name"] {
            let json = serde_json::json!({ "title": "x", identity_field: "hijacked" });
            let result: Result<PatchChangeRequest, _> = serde_json::from_value(json);
            assert!(
                result.is_err(),
                "a PATCH body carrying `{identity_field}` must be REJECTED, not silently ignored"
            );
        }

        let round_tripped = serde_json::to_value(&patch).unwrap();
        assert!(
            round_tripped.get("project").is_none(),
            "PatchChangeRequest must not carry `project`"
        );
        assert!(
            round_tripped.get("name").is_none(),
            "PatchChangeRequest must not carry `name`"
        );
    }

    /// 1.31 — `SddRevisionMeta` enforces the metadata-only contract in the type system:
    /// it has no `content` field at all, so a list endpoint physically cannot leak content.
    #[test]
    fn revision_meta_carries_no_content() {
        let meta = SddRevisionMeta {
            id: "r1".into(),
            artifact_id: "a1".into(),
            revision: 1,
            content_hash: "abc".into(),
            byte_size: 42,
            git_commit: None,
            git_path: None,
            source: "agent".into(),
            created_by: "u1".into(),
            created_at: "2026-07-11T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&meta).unwrap();
        assert!(
            json.get("content").is_none(),
            "SddRevisionMeta must never serialize a content field"
        );
    }

    // ── Knowledge migration types (v60) ──────────────────────────────────────

    #[test]
    fn destination_kind_roundtrips_through_serde_and_str() {
        for (variant, wire) in [
            (DestinationKind::Memory, "memory"),
            (DestinationKind::Convention, "convention"),
            (DestinationKind::Task, "task"),
            (DestinationKind::SddArtifact, "sdd_artifact"),
            (DestinationKind::Harness, "harness"),
            (
                DestinationKind::HarnessConfigReview,
                "harness_config_review",
            ),
        ] {
            assert_eq!(variant.as_str(), wire);
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            assert_eq!(DestinationKind::from_str(wire).unwrap(), variant);
            assert_eq!(
                serde_json::from_value::<DestinationKind>(json!(wire)).unwrap(),
                variant
            );
        }
    }

    /// The accepted set is closed. A destination the database will not store
    /// must not deserialize into something the code believes it can commit.
    #[test]
    fn destination_kind_rejects_unknown_string() {
        assert!(DestinationKind::from_str("notion_page").is_err());
        assert!(serde_json::from_value::<DestinationKind>(json!("notion_page")).is_err());
        assert!(serde_json::from_value::<DestinationKind>(json!("Memory")).is_err());
    }

    /// Source kinds are kebab-case on the wire because that is what the CLI
    /// flag and the database CHECK both use — `--source repo-docs`.
    #[test]
    fn source_kind_serializes_kebab_case() {
        for (variant, wire) in [
            (SourceKind::RepoDocs, "repo-docs"),
            (SourceKind::GitHistory, "git-history"),
            (SourceKind::ClaudeMemories, "claude-memories"),
            (SourceKind::DbSchema, "db-schema"),
            (SourceKind::Noop, "noop"),
        ] {
            assert_eq!(variant.as_str(), wire);
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(wire));
            assert_eq!(SourceKind::from_str(wire).unwrap(), variant);
        }
        assert!(
            SourceKind::from_str("repo_docs").is_err(),
            "snake_case is not the wire format"
        );
    }

    /// Optimistic concurrency only works if every caller declares the version
    /// it acted on. An absent `expected_version` must be a deserialization
    /// failure, not a default.
    #[test]
    fn review_request_without_expected_version_fails_to_deserialize() {
        let missing = json!({ "candidate_id": "c1", "action": "approved" });
        assert!(
            serde_json::from_value::<ReviewActionRequest>(missing).is_err(),
            "expected_version must be mandatory"
        );

        let present = json!({ "candidate_id": "c1", "action": "approved", "expected_version": 3 });
        let parsed: ReviewActionRequest = serde_json::from_value(present).unwrap();
        assert_eq!(parsed.expected_version, 3);
        assert_eq!(parsed.action, ReviewVerdict::Approved);
    }

    #[test]
    fn candidate_input_defaults_optional_fields() {
        let minimal = json!({
            "source_identity": "repo-docs:docs/a.md:abc",
            "destination_kind": "convention",
            "content": "Always prefer X over Y."
        });
        let parsed: CandidateInput = serde_json::from_value(minimal).unwrap();
        assert_eq!(parsed.destination_kind, DestinationKind::Convention);
        assert!(parsed.source_excerpt.is_none());
        assert!(parsed.confidence.is_none());
        assert_eq!(parsed.destination_hint, json!(null));
    }

    #[test]
    fn stage_result_is_tagged_by_outcome() {
        let staged = serde_json::to_value(StageResult::Staged { id: "c1".into() }).unwrap();
        assert_eq!(staged["result"], json!("staged"));
        assert_eq!(staged["id"], json!("c1"));

        let skipped = serde_json::to_value(StageResult::Skipped {
            reason: "already_committed".into(),
        })
        .unwrap();
        assert_eq!(skipped["result"], json!("skipped"));
        assert_eq!(skipped["reason"], json!("already_committed"));
    }

    #[test]
    fn migration_run_roundtrips() {
        let run = MigrationRun {
            id: "r1".into(),
            org_id: "org1".into(),
            client_id: Some("cl1".into()),
            project_id: None,
            source_kind: SourceKind::RepoDocs,
            status: "staging".into(),
            source_ref: Some("./".into()),
            runner_version: Some("2.1.233".into()),
            attestation: json!({}),
            created_by: "u1".into(),
            created_at: "2026-08-15T00:00:00Z".into(),
            updated_at: "2026-08-15T00:00:00Z".into(),
        };
        let wire = serde_json::to_value(&run).unwrap();
        assert_eq!(wire["source_kind"], json!("repo-docs"));
        assert!(
            wire.get("project_id").is_none(),
            "None scope fields are omitted, not null"
        );
        let back: MigrationRun = serde_json::from_value(wire).unwrap();
        assert_eq!(back.client_id.as_deref(), Some("cl1"));
    }
}
