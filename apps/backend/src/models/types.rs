use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::fmt;

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
            Role::Admin  => 2,
            Role::Member => 1,
            Role::Viewer => 0,
        }
    }
}

impl FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin"  => Ok(Role::Admin),
            "member" => Ok(Role::Member),
            "viewer" => Ok(Role::Viewer),
            other    => Err(format!("unknown role: {other}")),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Role::Admin  => "admin",
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
    pub fn is_admin(&self) -> bool {
        matches!(self, UserRole::Standard(Role::Admin))
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
    pub types:    Vec<FacetCount>,
    pub scopes:   Vec<FacetCount>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectStats {
    pub total_memories: i64,
    pub memories_this_week: i64,
    pub last_memory_at: Option<String>,
    pub top_tags: Vec<String>,
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
    #[serde(default)]
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
}

/// Daily memory count — one entry per calendar day.
#[derive(Debug, Serialize, Clone)]
pub struct DailyCount {
    pub date: String,  // "YYYY-MM-DD"
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
    pub daily_counts: Vec<DailyCount>,  // last 30 days
    pub by_type: Vec<NameCount>,        // top 5 types
    pub by_project: Vec<NameCount>,     // top 5 projects
    pub total: i64,
    pub this_week: i64,                 // last 7 days
    pub this_month: i64,                // last 30 days
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
pub struct ImportMemoriesRequest {
    pub memories: Vec<ImportMemory>,
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
#[derive(Debug, Deserialize)]
pub struct MergeMemoriesRequest {
    pub keep_id: String,
    pub merge_id: String,
}

/// Request body for `POST /v1/admin/memories/bulk-tag`.
#[derive(Debug, Deserialize)]
pub struct BulkTagRequest {
    pub ids: Vec<String>,
    pub action: String,
    pub tag: String,
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
    pub agent_id: String,
    pub memory_count: i64,
    pub last_activity: String,
}

/// A single day entry in the memory creation heatmap.
/// Returned by `GET /v1/admin/stats/memory-heatmap`.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct HeatmapDay {
    pub day: String,   // "YYYY-MM-DD"
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert!("Admin".parse::<Role>().is_err(), "case-sensitive: uppercase must fail");
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
        assert!(entry.previous_hash.is_none(), "previous_hash must default to None");
        assert!(entry.current_hash.is_none(), "current_hash must default to None");

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
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: Policy = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
        assert_eq!(back.rule_type, "model_whitelist");
        assert_eq!(back.enabled, true);
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
            PolicyConfig::BudgetLimit { max_tokens_per_day, max_requests_per_day } => {
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

        let partial: UpdatePolicyRequest =
            serde_json::from_str(r#"{"enabled": false}"#).unwrap();
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
        assert!(v.get("last_indexed").is_none(), "last_indexed must be omitted when None");
        assert!(v.get("file_count").is_none(), "file_count must be omitted when None");
    }

    #[test]
    fn index_project_request_roundtrip() {
        let req = IndexProjectRequest {
            project: "myapp".into(),
            root_path: Some("/workspace/myapp".into()),
            repo_url: None,
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
}
