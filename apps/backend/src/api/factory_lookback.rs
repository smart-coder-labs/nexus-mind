//! Read-only F5 lookback recommendations from recurring completed tasks and
//! existing NexusMind bugfix memories. This module never changes tasks, memories,
//! factory policy, or autonomous execution state.

use axum::{extract::{Query, State}, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};

use crate::{
    api::helpers::require_permission,
    db::queries::{self, TaskListFilters},
    models::types::{ApiError, AuthContext, Memory, Task, UserRole},
    store::{sqlite::SqliteStore, MemoryFilters, MemoryStore},
};

const HISTORY_LIMIT: i64 = 500;
const RECOMMENDATION_LIMIT: usize = 20;

#[derive(Deserialize, Default)]
pub struct LookbackQuery {
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LookbackEvidenceTask {
    pub id: String,
    pub title: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LookbackRecommendation {
    pub memory_id: String,
    pub memory_title: String,
    pub memory_project: String,
    pub recurring_terms: Vec<String>,
    pub recurrence_count: usize,
    pub task_evidence: Vec<LookbackEvidenceTask>,
    pub explanation: String,
}

#[derive(Serialize)]
pub struct LookbackResponse {
    pub recommendations: Vec<LookbackRecommendation>,
    pub completed_tasks_scanned: usize,
    pub bugfix_memories_scanned: usize,
    pub history_limit: usize,
    pub history_truncated: bool,
}

fn api_error(status: StatusCode, code: &str, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: message.into(), code: code.to_owned() }))
}

fn viewer_user_id(auth: &AuthContext) -> Option<&str> {
    if matches!(auth.role, UserRole::Custom(ref role) if role == "super_user") { None } else { Some(&auth.user_id) }
}

fn tokens(text: &str) -> std::collections::BTreeSet<String> {
    const STOP: &[&str] = &[
        "about", "add", "added", "adding", "adds", "after", "again", "also", "and", "are",
        "arregla", "arreglar", "arreglado", "bug", "bugfix", "bugfixes", "bugs", "como", "con", "corregir", "corregido",
        "cuando", "de", "del", "desde", "do", "el", "en", "error", "errors", "errores", "for", "from",
        "fix", "fixed", "fixes", "have", "how", "implement", "implementa", "implementacion", "implementación", "implementado", "implementar",
        "implemented", "implementing", "implements",
        "into", "issue", "issues", "la", "las", "los", "mais", "más", "not", "para", "pero", "por",
        "problema", "problemas", "que", "qué", "reparar", "reparado", "tarea", "tareas", "task", "tasks", "this", "that", "the",
        "then", "there", "these", "they", "this", "una", "uno", "update", "updated", "updates", "updating",
        "actualiza", "actualizado", "actualizar", "agrega", "agregado", "agregar", "añade", "añadido", "añadir",
        "solucion", "solución", "solucionar", "was", "when", "with", "without", "you",
    ];
    let normalized = text.to_lowercase();
    normalized
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3 && !STOP.contains(word))
        .map(str::to_owned)
        .collect()
}

fn task_tokens(task: &Task) -> std::collections::BTreeSet<String> {
    let mut text = task.title.clone();
    if let Some(description) = &task.description { text.push(' '); text.push_str(description); }
    for label in &task.labels { text.push(' '); text.push_str(label); }
    tokens(&text)
}

fn memory_tokens(memory: &Memory) -> std::collections::BTreeSet<String> {
    let mut text = memory.title.clone().unwrap_or_default();
    text.push(' ');
    text.push_str(&memory.content);
    if let Some(topic) = &memory.topic_key { text.push(' '); text.push_str(topic); }
    for tag in &memory.tags { text.push(' '); text.push_str(tag); }
    tokens(&text)
}

fn recommendation_sort(a: &LookbackRecommendation, b: &LookbackRecommendation) -> std::cmp::Ordering {
    b.recurrence_count.cmp(&a.recurrence_count)
        .then_with(|| b.recurring_terms.len().cmp(&a.recurring_terms.len()))
        .then_with(|| a.memory_title.cmp(&b.memory_title))
}

fn build_recommendations(tasks: &[Task], memories: &[Memory]) -> Vec<LookbackRecommendation> {
    let mut task_term_hits: std::collections::BTreeMap<String, Vec<&Task>> = std::collections::BTreeMap::new();
    for task in tasks.iter().filter(|task| task.status == "done") {
        for term in task_tokens(task) { task_term_hits.entry(term).or_default().push(task); }
    }
    let recurring: std::collections::BTreeMap<String, Vec<&Task>> = task_term_hits
        .into_iter().filter(|(_, matches)| matches.len() >= 2).collect();

    let mut recommendations = Vec::new();
    for memory in memories.iter().filter(|memory| memory.memory_type.as_deref() == Some("bugfix")) {
        let memory_terms = memory_tokens(memory);
        let terms: Vec<String> = recurring.iter().filter_map(|(term, matching_tasks)| {
            let scoped_count = matching_tasks.iter().filter(|task|
                memory.scope == "org" || task.project == memory.project
            ).count();
            (memory_terms.contains(term) && scoped_count >= 2).then(|| term.clone())
        }).collect();
        if terms.is_empty() { continue; }

        let mut evidence: std::collections::BTreeMap<String, &Task> = std::collections::BTreeMap::new();
        for term in &terms {
            if let Some(matches) = recurring.get(term) {
                for task in matches {
                    if memory.scope == "org" || task.project == memory.project {
                        evidence.insert(task.id.clone(), *task);
                    }
                }
            }
        }
        let mut task_evidence: Vec<_> = evidence.values().map(|task| LookbackEvidenceTask {
            id: task.id.clone(), title: task.title.clone(), updated_at: task.updated_at.clone(),
        }).collect();
        task_evidence.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
        let recurrence_count = task_evidence.len();
        let title = memory.title.clone().unwrap_or_else(|| "Bugfix memory".to_owned());
        let explanation = format!(
            "La memoria bugfix «{title}» comparte los términos {} con {recurrence_count} tareas completadas. Revisa si la solución documentada aplica.",
            terms.iter().map(|term| format!("«{term}»")).collect::<Vec<_>>().join(", ")
        );
        recommendations.push(LookbackRecommendation {
            memory_id: memory.id.clone(), memory_title: title, memory_project: memory.project.clone(),
            recurring_terms: terms, recurrence_count, task_evidence, explanation,
        });
    }
    recommendations.sort_by(recommendation_sort);
    recommendations.truncate(RECOMMENDATION_LIMIT);
    recommendations
}

pub async fn recommendations(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<LookbackQuery>,
) -> Result<Json<LookbackResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Database lock error"))?;
    require_permission(&conn, &auth, query.project.as_deref(), "task:read")?;
    require_permission(&conn, &auth, query.project.as_deref(), "memory:read")?;
    let filters = TaskListFilters { project: query.project.clone(), status: Some("done".into()), ..Default::default() };
    let tasks = queries::list_tasks(&conn, &auth.org_id, viewer_user_id(&auth), &filters, HISTORY_LIMIT, 0)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", error.to_string()))?;
    let total_tasks = queries::count_tasks(&conn, &auth.org_id, viewer_user_id(&auth), &filters)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", error.to_string()))?;
    drop(conn);

    let viewer = viewer_user_id(&auth);
    let memory_filters = MemoryFilters {
        user_id: None, tool: None, project: query.project.as_deref(), memory_type: Some("bugfix"),
        scope: None, session_id: None, limit: HISTORY_LIMIT, offset: 0, include_archived: false,
        from_date: None, to_date: None, collection_id: None, viewer_user_id: viewer,
    };
    let memory_page = store.list(&auth.org_id, &memory_filters)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", error.to_string()))?;
    let recommendations = build_recommendations(&tasks, &memory_page.memories);
    Ok(Json(LookbackResponse {
        recommendations,
        completed_tasks_scanned: tasks.len(),
        bugfix_memories_scanned: memory_page.memories.len(),
        history_limit: HISTORY_LIMIT as usize,
        history_truncated: total_tasks > HISTORY_LIMIT || memory_page.total > HISTORY_LIMIT,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, title: &str, status: &str) -> Task {
        Task { id: id.into(), org_id: "org".into(), project: "project".into(), title: title.into(), description: None,
            status: status.into(), priority: "medium".into(), due_date: None, parent_id: None, sprint_id: None,
            created_by: "user".into(), created_at: "2026-01-01".into(), updated_at: "2026-01-02".into(),
            archived_at: None, assignees: vec![], labels: vec![], comment_count: 0, spec_links: vec![], subtask_count: 0 }
    }

    fn memory(id: &str, kind: &str, title: &str, content: &str) -> Memory {
        Memory { id: id.into(), org_id: "org".into(), user_id: "user".into(), project: "project".into(), tool: "nexusmind".into(),
            content: content.into(), tags: vec![], created_at: "2026-01-01".into(), title: Some(title.into()), memory_type: Some(kind.into()),
            scope: "project".into(), topic_key: None, session_id: None, revision_count: 1, normalized_hash: None, project_id: None,
            archived_at: None, pinned: false, collection_id: None, admin_note: None, delete_after: None, status: "active".into() }
    }

    #[test]
    fn recommends_matching_bugfix_memory_for_repeated_completed_task_terms() {
        let tasks = vec![task("t1", "Fix oauth refresh timeout", "done"), task("t2", "OAuth refresh timeout after deploy", "done")];
        let memories = vec![memory("m1", "bugfix", "OAuth refresh timeout", "Refresh token clock skew caused timeout")];
        let recommendations = build_recommendations(&tasks, &memories);
        assert_eq!(recommendations.len(), 1);
        assert!(recommendations[0].recurring_terms.contains(&"oauth".to_owned()));
        assert_eq!(recommendations[0].recurrence_count, 2);
        assert_eq!(recommendations[0].task_evidence.len(), 2);
    }

    #[test]
    fn does_not_recommend_non_bugfix_memories_or_open_task_matches() {
        let tasks = vec![task("t1", "Fix oauth timeout", "done"), task("t2", "Fix oauth timeout", "in_progress")];
        let memories = vec![memory("m1", "decision", "OAuth timeout", "Retry policy")];
        assert!(build_recommendations(&tasks, &memories).is_empty());
    }

    #[test]
    fn one_off_terms_do_not_create_recommendation() {
        let tasks = vec![task("t1", "Fix oauth timeout", "done"), task("t2", "Update billing dashboard", "done")];
        let memories = vec![memory("m1", "bugfix", "OAuth refresh", "timeout")];
        assert!(build_recommendations(&tasks, &memories).is_empty());
    }

    #[test]
    fn project_memory_does_not_match_recurrence_from_other_projects() {
        let mut other = task("t2", "Fix oauth timeout", "done");
        other.project = "other-project".into();
        let tasks = vec![task("t1", "Fix oauth timeout", "done"), other];
        let memories = vec![memory("m1", "bugfix", "OAuth timeout", "Retry policy")];
        assert!(build_recommendations(&tasks, &memories).is_empty());
    }

    #[test]
    fn generic_issue_and_edit_words_do_not_drive_recommendations() {
        let tasks = vec![
            task("t1", "Fix bug issue task error update add implement", "done"),
            task("t2", "Fix bug issue task error update add implement", "done"),
        ];
        let memories = vec![memory("m1", "bugfix", "Fix bugfix issue", "Task error after update")];
        assert!(build_recommendations(&tasks, &memories).is_empty());
    }

    #[test]
    fn domain_terms_remain_matchable_after_generic_stopword_filtering() {
        let kept = tokens("oauth timeout fix bug issue task update add implement");
        assert!(kept.contains("oauth"));
        assert!(kept.contains("timeout"));
        for generic in ["fix", "bug", "issue", "task", "update", "add", "implement"] {
            assert!(!kept.contains(generic));
        }
    }
}
