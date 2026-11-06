use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NextAction {
    Continue,
    RetrieveMore,
    RunTests,
    Retry,
    Replan,
    SwitchRuntime,
    HumanReview,
    Finish,
}

impl NextAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::RetrieveMore => "retrieve_more",
            Self::RunTests => "run_tests",
            Self::Retry => "retry",
            Self::Replan => "replan",
            Self::SwitchRuntime => "switch_runtime",
            Self::HumanReview => "human_review",
            Self::Finish => "finish",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCapabilities {
    pub streaming: bool,
    pub tool_calling: bool,
    pub native_file_editing: bool,
    pub shell_execution: bool,
    pub session_resume: bool,
    pub subagents: bool,
    pub structured_output: bool,
    pub long_running_execution: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeDescriptor {
    pub id: String,
    pub provider: String,
    pub model: Option<String>,
    pub execution_mode: String,
    pub capabilities: RuntimeCapabilities,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Verification {
    pub tests_passed: u32,
    pub tests_failed: u32,
    pub lint_passed: Option<bool>,
    pub typecheck_passed: Option<bool>,
    pub commands: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskExecutionState {
    pub task_id: String,
    pub objective: String,
    pub repository: String,
    pub requirements_total: u32,
    pub requirements_covered: u32,
    pub unresolved_requirements: Vec<String>,
    pub changed_files: Vec<String>,
    pub changed_symbols: Vec<String>,
    pub impact_radius: u32,
    pub affected_processes: Vec<String>,
    pub affected_contracts: Vec<String>,
    pub verification: Verification,
    pub unresolved_evidence: Vec<String>,
    pub attempts: u32,
    pub tool_errors: u32,
    pub tokens_used: u64,
    pub estimated_cost_usd: f64,
    #[serde(default)]
    pub agent_summary: String,
    #[serde(default)]
    pub change_summary: String,
    #[serde(default)]
    pub verification_output: String,
    #[serde(default)]
    pub context_sources: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub id: String,
    pub task_id: String,
    pub state_hash: String,
    pub decision_type: String,
    pub candidates: Vec<NextAction>,
    pub selected: NextAction,
    pub confidence: f32,
    pub reason: String,
    pub runtime: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub runtime: String,
    pub state: TaskExecutionState,
    pub decisions: Vec<DecisionRecord>,
    pub transcript: Vec<String>,
    pub status: String,
    #[serde(default)]
    pub provider_session_id: Option<String>,
}
