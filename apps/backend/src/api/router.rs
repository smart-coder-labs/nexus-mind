use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Extension, Router,
};
use rusqlite::Connection;
use std::sync::Arc;
use tower_cookies::CookieManagerLayer;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::api::{
    admin, agents, audit, auth, automation, autonomous_agents, autonomous_webhooks, backup,
    clients, code, context, conventions, docs, github_auth, harnesses, health, internal, memory,
    middleware as api_mw, migrations as migrations_api, policy, rate_limit, sdd, search, sessions,
    tasks, usage, users, webhooks,
};
use crate::config::Config;
use crate::email::EmailConfig;
use crate::embed::EmbedService;
use crate::store::sqlite::SqliteStore;

pub fn build(conn: Connection, config: Config) -> Router {
    // Build the store so we can also return a cloneable handle to the
    // underlying connection. Callers that need a long-lived reference to the
    // SQLite store (e.g. the background backup job) should use `build_with_store`
    // and clone the returned `SqliteStore`.
    let (router, _store) = build_with_store(conn, config);
    router
}

/// Same as [`build`] but also returns the constructed [`SqliteStore`] so the
/// caller can hold an extra reference for background tasks.
pub fn build_with_store(conn: Connection, config: Config) -> (Router, SqliteStore) {
    let config = Arc::new(config);
    let embed = if std::env::var("NEXUSMIND_EMBED_ENABLED").as_deref() == Ok("true") {
        match EmbedService::init() {
            Ok(svc) => {
                tracing::info!("Embedding service initialized (nomic-embed-text-v1.5)");
                Some(svc)
            }
            Err(e) => {
                tracing::warn!("Embedding service unavailable — semantic search disabled: {e}");
                None
            }
        }
    } else {
        tracing::info!("Embedding service disabled (set NEXUSMIND_EMBED_ENABLED=true to enable)");
        None
    };

    let store = match embed {
        Some(svc) => SqliteStore::new(conn).with_embed(svc),
        None => SqliteStore::new(conn),
    };

    let email_config: Option<Arc<EmailConfig>> = match (
        config.smtp_username.clone(),
        config.smtp_password.clone(),
        config.smtp_from.clone(),
    ) {
        (Some(username), Some(password), Some(from)) => Some(Arc::new(EmailConfig {
            smtp_host: config.smtp_host.clone(),
            smtp_port: config.smtp_port,
            smtp_username: username,
            smtp_password: password,
            smtp_from: from,
            app_base_url: config.app_base_url.clone(),
        })),
        _ => {
            tracing::warn!("SMTP not configured (SMTP_USERNAME, SMTP_PASSWORD, SMTP_FROM required). Emails will not be sent.");
            None
        }
    };

    let rate_state = rate_limit::RateLimitState::new(store.conn());

    let protected = Router::new()
        .route("/v1/memory/store", post(memory::store))
        .route("/v1/memory/search", post(memory::search))
        .route("/v1/memory/export", get(memory::export))
        .route("/v1/memory/graph", get(memory::get_graph))
        .route("/v1/memory/bulk", delete(memory::bulk_delete))
        .route(
            "/v1/memory/:id",
            get(memory::get_by_id)
                .delete(memory::delete)
                .patch(memory::update),
        )
        .route("/v1/memory/:id/archive", post(memory::archive))
        .route("/v1/memory/:id/restore", post(memory::restore))
        .route(
            "/v1/memory/:id/pin",
            post(memory::pin).delete(memory::unpin),
        )
        .route("/v1/memory/:id/unpin", post(memory::unpin))
        .route("/v1/memory/:id/promote", post(memory::promote))
        .route("/v1/memory", get(memory::list))
        .route(
            "/v1/sessions",
            get(sessions::list_sessions_handler).post(sessions::create_session_handler),
        )
        .route(
            "/v1/sessions/:id",
            get(sessions::get_session_handler).patch(sessions::patch_session_handler),
        )
        .route(
            "/v1/sessions/:id/memories",
            get(sessions::list_session_memories_handler),
        )
        .route("/v1/users", get(users::list))
        .route("/v1/users/invite", post(users::invite))
        .route("/v1/users/:id", delete(users::remove))
        .route("/v1/users/:id/rotate-key", post(users::rotate_key))
        .route("/v1/users/:id/role", patch(users::update_role))
        .route(
            "/v1/roles",
            get(admin::list_roles_api).post(admin::create_role_api),
        )
        .route(
            "/v1/roles/:id",
            delete(admin::delete_role_api).patch(admin::update_role_api),
        )
        .route(
            "/v1/clients",
            get(clients::list_clients).post(clients::create_client),
        )
        .route(
            "/v1/clients/:id",
            patch(clients::update_client).delete(clients::delete_client),
        )
        .route("/v1/clients/:id/archive", post(clients::archive_client))
        .route(
            "/v1/migrations",
            get(migrations_api::list_runs).post(migrations_api::create_run),
        )
        .route(
            "/v1/migrations/:id",
            get(migrations_api::get_run).delete(migrations_api::delete_run),
        )
        .route(
            "/v1/migrations/:id/candidates",
            get(migrations_api::list_candidates).post(migrations_api::stage_candidates),
        )
        .route("/v1/migrations/:id/review", post(migrations_api::review))
        .route("/v1/migrations/:id/commit", post(migrations_api::commit))
        .route(
            "/v1/migrations/:id/cancel",
            post(migrations_api::cancel_run),
        )
        .route("/v1/migrations/:id/report", get(migrations_api::get_report))
        .route("/v1/docs/search", get(docs::search))
        .route("/v1/docs/index-status", get(docs::index_status))
        .route("/v1/usage", post(usage::ingest))
        .route("/v1/usage/summary", get(usage::summary))
        .route("/v1/usage/timeseries", get(usage::timeseries))
        .route("/v1/usage/backfill", post(usage::backfill))
        .route(
            "/v1/clients/:id/members",
            get(clients::list_members).post(clients::add_member),
        )
        .route(
            "/v1/clients/:id/members/:user_id",
            delete(clients::remove_member),
        )
        .route(
            "/v1/projects",
            get(admin::list_projects_api).post(admin::create_project_api),
        )
        .route(
            "/v1/projects/:id",
            get(admin::get_project_api)
                .delete(admin::delete_project_api)
                .patch(admin::update_project_api),
        )
        .route("/v1/projects/:id/archive", post(admin::archive_project_api))
        .route("/v1/projects/:id/restore", post(admin::restore_project_api))
        .route(
            "/v1/projects/:project_id/members",
            get(admin::list_project_members_api).post(admin::upsert_project_member_api),
        )
        .route(
            "/v1/projects/:project_id/members/:user_id",
            delete(admin::delete_project_member_api),
        )
        .route(
            "/v1/projects/:id/settings",
            get(admin::get_project_settings_api).patch(admin::update_project_settings_api),
        )
        .route("/v1/projects/:id/stats", get(admin::get_project_stats_api))
        .route(
            "/v1/policies",
            get(policy::list_policies).post(policy::create_policy),
        )
        .route(
            "/v1/policies/:id",
            patch(policy::update_policy).delete(policy::delete_policy),
        )
        .route("/v1/policy/check", post(policy::check_policy))
        .route("/v1/automation/profiles", get(automation::list_profiles))
        .route(
            "/v1/automation/authorize",
            post(automation::authorize_profile),
        )
        .route(
            "/v1/conventions",
            get(conventions::list_conventions).post(conventions::create_convention),
        )
        .route(
            "/v1/conventions/:id",
            get(conventions::get_convention)
                .patch(conventions::update_convention)
                .delete(conventions::delete_convention),
        )
        .route(
            "/v1/conventions/:id/archive",
            post(conventions::archive_convention),
        )
        .route(
            "/v1/conventions/:id/restore",
            post(conventions::restore_convention),
        )
        .route(
            "/v1/harnesses",
            get(harnesses::list_harnesses).post(harnesses::create_harness),
        )
        .route("/v1/harnesses/:id", get(harnesses::get_harness))
        .route(
            "/v1/harnesses/:id/archive",
            post(harnesses::archive_harness),
        )
        .route(
            "/v1/harnesses/:id/versions",
            post(harnesses::publish_version),
        )
        .route(
            "/v1/harnesses/:id/versions/:version",
            get(harnesses::get_version),
        )
        .route(
            "/v1/harnesses/:id/publish",
            post(harnesses::publish_version),
        )
        .route(
            "/v1/harnesses/:id/versions/:version/download",
            get(harnesses::download_version),
        )
        .route(
            "/v1/harnesses/:id/versions/:version/approval",
            post(harnesses::approve_install),
        )
        .route(
            "/v1/harnesses/:id/versions/:version/install-result",
            post(harnesses::record_install_result),
        )
        .route(
            "/v1/harness-recommendations",
            get(harnesses::recommendations),
        )
        .route(
            "/v1/harness-config-reviews",
            get(harnesses::list_config_reviews).post(harnesses::create_config_review),
        )
        .route(
            "/v1/harness-config-reviews/:id",
            get(harnesses::get_config_review),
        )
        .route(
            "/v1/harness-config-reviews/:id/comments",
            get(harnesses::list_config_review_comments)
                .post(harnesses::create_config_review_comment),
        )
        // ── SDD artifacts ──
        // Static paths first: /v1/sdd/search and the /v1/sdd/artifacts collection must be
        // registered before /v1/sdd/artifacts/:id, or ":id" would swallow them.
        .route("/v1/sdd/search", get(sdd::search_handler))
        .route(
            "/v1/sdd/artifacts",
            get(sdd::get_artifact_by_key_handler).put(sdd::put_artifact_handler),
        )
        .route("/v1/sdd/artifacts/:id", get(sdd::get_artifact_handler))
        .route(
            "/v1/sdd/artifacts/:id/revisions",
            get(sdd::list_artifact_revisions_handler),
        )
        // GET only, deliberately: revisions are immutable, so PUT/PATCH/DELETE here must 405.
        .route(
            "/v1/sdd/artifacts/:id/revisions/:rev",
            get(sdd::get_artifact_revision_handler),
        )
        .route(
            "/v1/sdd/changes",
            get(sdd::list_changes_handler).post(sdd::create_change_handler),
        )
        .route(
            "/v1/sdd/changes/:id",
            get(sdd::get_change_handler)
                .patch(sdd::patch_change_handler)
                .delete(sdd::delete_change_handler),
        )
        .route(
            "/v1/sdd/changes/:id/artifacts",
            get(sdd::list_change_artifacts_handler),
        )
        .route(
            "/v1/sdd/changes/:id/tasks",
            get(sdd::list_change_tasks_handler),
        )
        .route(
            "/v1/sdd/changes/:id/memories",
            post(sdd::link_change_memory_handler),
        )
        .route(
            "/v1/sdd/changes/:id/memories/:memory_id",
            delete(sdd::unlink_change_memory_handler),
        )
        // Which living specifications this change has merged its deltas into.
        .route(
            "/v1/sdd/changes/:id/specs",
            get(sdd::list_change_specs_handler),
        )
        // ── The living specification: openspec/specs/{capability}/spec.md ──
        // Same ordering rule: the static /v1/sdd/specs collection is registered BEFORE
        // /v1/sdd/specs/:id, or ":id" would swallow it.
        .route(
            "/v1/sdd/specs",
            get(sdd::get_specs_handler).put(sdd::put_spec_handler),
        )
        .route("/v1/sdd/specs/:id", get(sdd::get_spec_handler))
        .route(
            "/v1/sdd/specs/:id/revisions",
            get(sdd::list_spec_revisions_handler),
        )
        // GET only, deliberately: spec revisions are immutable, so a write here must 405.
        .route(
            "/v1/sdd/specs/:id/revisions/:rev",
            get(sdd::get_spec_revision_handler),
        )
        .route(
            "/v1/tasks/resolve-by-spec",
            post(tasks::resolve_by_spec_handler),
        )
        .route(
            "/v1/tasks",
            get(tasks::list_tasks_handler).post(tasks::create_task_handler),
        )
        .route(
            "/v1/tasks/:id",
            get(tasks::get_task_handler)
                .patch(tasks::patch_task_handler)
                .delete(tasks::delete_task_handler),
        )
        .route("/v1/tasks/:id/subtasks", get(tasks::list_subtasks_handler))
        .route("/v1/tasks/:id/assignees", post(tasks::assign_task_handler))
        .route(
            "/v1/tasks/:id/assignees/:user_id",
            delete(tasks::unassign_task_handler),
        )
        .route("/v1/tasks/:id/labels", post(tasks::add_task_label_handler))
        .route(
            "/v1/tasks/:id/labels/:label",
            delete(tasks::remove_task_label_handler),
        )
        .route(
            "/v1/tasks/:id/comments",
            get(tasks::list_task_comments_handler).post(tasks::add_task_comment_handler),
        )
        .route(
            "/v1/tasks/:id/comments/:comment_id",
            delete(tasks::delete_task_comment_handler),
        )
        .route(
            "/v1/tasks/:id/spec-links",
            get(tasks::list_task_spec_links_handler).post(tasks::link_task_spec_handler),
        )
        .route(
            "/v1/tasks/:id/spec-links/:name",
            delete(tasks::unlink_task_spec_handler),
        )
        .route(
            "/v1/sprints",
            get(tasks::list_sprints_handler).post(tasks::create_sprint_handler),
        )
        .route(
            "/v1/sprints/:id",
            get(tasks::get_sprint_handler)
                .patch(tasks::patch_sprint_handler)
                .delete(tasks::delete_sprint_handler),
        )
        .route(
            "/v1/sprints/:id/retrospectives",
            get(tasks::list_retrospectives_handler).post(tasks::create_retrospective_handler),
        )
        .route("/v1/context", get(context::get_global_context))
        .route("/v1/context/type/:type", get(context::get_type_context))
        .route("/v1/context/session/:id", get(context::get_session_context))
        .route(
            "/v1/context/project/:project",
            get(context::get_project_context),
        )
        .route("/v1/code/index", post(code::post_index))
        .route("/v1/code/search", post(code::post_search))
        .route("/v1/code/locate", post(code::post_locate))
        .route("/v1/code/status/:project", get(code::get_status))
        .route("/v1/code/context", get(code::get_context))
        .route("/v1/code/graph", get(code::get_graph))
        .route("/v1/code/snippet", get(code::get_snippet))
        .route("/v1/code/projects", get(code::list_projects))
        .route(
            "/v1/code/projects/:id",
            delete(code::delete_project).patch(code::update_code_project),
        )
        .route(
            "/v1/code/projects/:id/schedule",
            patch(code::update_schedule),
        )
        .route("/v1/code/projects/:id/files", get(code::get_project_files))
        .route("/v1/code/projects/:id/reindex", post(code::post_reindex))
        .route("/v1/code/projects/:id/archive", post(code::archive_project))
        .route("/v1/code/projects/:id/restore", post(code::restore_project))
        .route("/v1/audit", get(audit::query))
        .route("/v1/audit/export", get(audit::export))
        .route("/v1/audit/log", post(audit::post_audit))
        .route("/v1/admin/dashboard", get(admin::dashboard))
        .route("/v1/admin/stats", get(admin::stats))
        .route("/v1/admin/stats/memory-facets", get(admin::memory_facets))
        .route(
            "/v1/admin/stats/trends",
            get(admin::get_memory_trends_handler),
        )
        .route("/v1/admin/stats/tags", get(admin::get_tag_stats_handler))
        .route("/v1/admin/stats/duplicates", get(admin::get_duplicates))
        .route(
            "/v1/admin/stats/agent-activity",
            get(admin::get_agent_activity),
        )
        .route(
            "/v1/admin/stats/memory-heatmap",
            get(admin::get_memory_heatmap),
        )
        .route(
            "/v1/admin/stats/top-contributors",
            get(admin::get_top_contributors),
        )
        .route("/v1/admin/stats/usage", get(admin::usage_stats))
        .route("/v1/admin/onboarding", get(admin::get_onboarding))
        .route(
            "/v1/admin/org/projects/over-enrolled",
            get(admin::over_enrolled_projects_handler),
        )
        .route(
            "/v1/admin/org",
            get(admin::get_org).patch(admin::update_org),
        )
        .route(
            "/v1/admin/org/settings",
            get(admin::get_org_settings_api).patch(admin::update_org_settings_api),
        )
        .route(
            "/v1/admin/settings/retention-preview",
            get(admin::get_retention_preview),
        )
        .route(
            "/v1/webhooks",
            get(webhooks::list_webhooks).post(webhooks::create_webhook),
        )
        .route(
            "/v1/webhooks/:id",
            patch(webhooks::update_webhook).delete(webhooks::delete_webhook),
        )
        .route("/v1/webhooks/:id/test", post(webhooks::test_webhook))
        .route(
            "/v1/webhooks/:id/deliveries",
            get(webhooks::list_deliveries),
        )
        .route(
            "/v1/webhooks/deliveries/:delivery_id/retry",
            post(webhooks::retry_delivery),
        )
        .route(
            "/v1/admin/keys",
            get(admin::list_org_keys).post(admin::create_org_key),
        )
        .route(
            "/v1/admin/keys/:key_id",
            get(admin::get_org_key)
                .patch(admin::update_org_key)
                .delete(admin::revoke_org_key),
        )
        .route("/v1/admin/keys/:key_id/rotate", post(admin::rotate_org_key))
        .route(
            "/v1/admin/keys/:key_id/revoke",
            post(admin::revoke_org_key_post),
        )
        .route("/v1/admin/users", get(admin::list_users_admin))
        .route(
            "/v1/admin/users/:user_id/reset-key",
            post(admin::reset_user_key),
        )
        .route(
            "/v1/admin/users/:user_id/disable",
            post(admin::disable_user),
        )
        .route("/v1/admin/users/:user_id/enable", post(admin::enable_user))
        .route("/v1/admin/users/:id/note", patch(admin::update_user_note))
        .route(
            "/v1/admin/memories/:id/note",
            patch(admin::update_memory_note),
        )
        .route(
            "/v1/admin/memories/:id/schedule-delete",
            patch(admin::schedule_memory_delete),
        )
        .route(
            "/v1/admin/org/announcement",
            patch(admin::update_org_announcement),
        )
        .route("/v1/admin/org/logo", patch(admin::update_org_logo))
        .route(
            "/v1/admin/memories/health",
            get(admin::get_memory_health_handler),
        )
        .route("/v1/admin/memories/import", post(admin::import_memories))
        .route("/v1/admin/memories/merge", post(admin::merge_memories))
        .route(
            "/v1/admin/memories/bulk-tag",
            post(admin::bulk_tag_memories),
        )
        .route("/v1/admin/tags/rename", post(admin::rename_tag))
        .route("/v1/admin/export", get(admin::export_org_config))
        .route("/v1/admin/import", post(admin::import_org_config))
        .route("/v1/search", get(search::get_global_search))
        .route(
            "/v1/admin/auth/change-password",
            post(auth::change_password),
        )
        .route("/v1/auth/change-password", post(auth::change_password))
        .route("/v1/admin/auth/me", get(auth::me))
        .route("/v1/admin/notifications", get(admin::get_notifications))
        .route(
            "/v1/admin/notifications/mark-all-read",
            post(admin::mark_all_notifications_read),
        )
        .route("/v1/admin/invites", post(admin::create_invite_link))
        .route(
            "/v1/admin/collections",
            get(admin::list_collections_api).post(admin::create_collection_api),
        )
        .route(
            "/v1/admin/collections/:id",
            delete(admin::delete_collection_api),
        )
        .route(
            "/v1/memories/:id/collection",
            post(admin::assign_memory_collection_api),
        )
        .route(
            "/v1/agents",
            get(agents::list_agents).post(agents::create_agent),
        )
        .route(
            "/v1/agents/:id",
            get(agents::get_agent).patch(agents::update_agent),
        )
        .route(
            "/v1/agents/:id/assignments",
            get(agents::list_agent_assignments),
        )
        .route(
            "/v1/autonomous-agents/templates",
            get(autonomous_agents::list_templates),
        )
        .route(
            "/v1/autonomous-agents/runtime",
            get(autonomous_agents::get_runtime_health)
                .post(autonomous_agents::check_runtime_health),
        )
        .route(
            "/v1/autonomous-agents/settings",
            get(autonomous_agents::get_org_settings).patch(autonomous_agents::patch_org_settings),
        )
        .route(
            "/v1/autonomous-agents/metrics",
            get(autonomous_agents::get_metrics),
        )
        .route(
            "/v1/autonomous-agents",
            get(autonomous_agents::list_definitions).post(autonomous_agents::create_definition),
        )
        .route(
            "/v1/autonomous-agents/:id",
            get(autonomous_agents::get_definition).patch(autonomous_agents::update_definition),
        )
        .route(
            "/v1/autonomous-agents/:id/validate",
            post(autonomous_agents::validate_definition),
        )
        .route(
            "/v1/autonomous-agents/:id/enable",
            post(autonomous_agents::enable_definition),
        )
        .route(
            "/v1/autonomous-agents/:id/disable",
            post(autonomous_agents::disable_definition),
        )
        .route(
            "/v1/autonomous-agents/:id/archive",
            post(autonomous_agents::archive_definition),
        )
        .route(
            "/v1/autonomous-agents/:id/schedule",
            get(autonomous_agents::get_schedule).put(autonomous_agents::put_schedule),
        )
        .route(
            "/v1/autonomous-agents/:id/run",
            post(autonomous_agents::run_now),
        )
        .route(
            "/v1/autonomous-agents/:id/targets",
            get(autonomous_agents::list_targets).post(autonomous_agents::put_target),
        )
        .route(
            "/v1/autonomous-agent-runs",
            get(autonomous_agents::list_runs),
        )
        .route(
            "/v1/autonomous-agent-runs/:id",
            get(autonomous_agents::get_run),
        )
        .route(
            "/v1/autonomous-agent-runs/:id/cancel",
            post(autonomous_agents::cancel_run),
        )
        .route(
            "/v1/autonomous-agent-runs/:id/archive",
            post(autonomous_agents::archive_run),
        )
        .route(
            "/v1/autonomous-agent-runs/:id/unarchive",
            post(autonomous_agents::unarchive_run),
        )
        .route(
            "/v1/autonomous-agent-runs/archive-all",
            post(autonomous_agents::archive_all_runs),
        )
        .route(
            "/v1/autonomous-agent-runs/:id/events",
            get(autonomous_agents::list_run_events),
        )
        .route(
            "/v1/autonomous-agent-runs/:id/transcript",
            get(autonomous_agents::list_run_transcript),
        )
        .route(
            "/v1/autonomous-agent-findings",
            get(autonomous_agents::list_findings),
        )
        .route(
            "/v1/autonomous-agent-findings/archive-all",
            post(autonomous_agents::archive_all_findings),
        )
        .route(
            "/v1/autonomous-agent-runs/:id/continue",
            post(autonomous_agents::continue_run),
        )
        .route(
            "/v1/autonomous-agent-findings/:id",
            patch(autonomous_agents::patch_finding),
        )
        .route(
            "/v1/autonomous-agent-findings/:id/github-issue",
            post(autonomous_agents::create_finding_issue),
        )
        .route(
            "/v1/autonomous-agent-findings/:id/publish-linkedin",
            post(autonomous_agents::publish_finding_linkedin),
        )
        .route(
            "/v1/autonomous-agents/linkedin/authorize",
            get(autonomous_agents::linkedin_authorize),
        )
        .route(
            "/v1/autonomous-agent-linkedin-connections",
            get(autonomous_agents::linkedin_connections),
        )
        .route(
            "/v1/autonomous-agent-deliveries",
            get(autonomous_agents::list_deliveries),
        )
        .route(
            "/v1/autonomous-agent-deliveries/:id/retry",
            post(autonomous_agents::retry_delivery),
        )
        .route(
            "/v1/autonomous-agent-connectors",
            get(autonomous_agents::list_connectors).put(autonomous_agents::put_connector),
        )
        .route(
            "/v1/autonomous-agent-connectors/:id",
            delete(autonomous_agents::revoke_connector),
        )
        .route(
            "/v1/backups",
            get(backup::list_backups_handler).post(backup::create_backup_handler),
        )
        .route("/v1/backups/:id", get(backup::get_backup_handler))
        .route(
            "/v1/backups/:id/restore",
            post(backup::restore_backup_handler),
        )
        .route(
            "/v1/backups/:id/download",
            get(backup::download_backup_handler),
        )
        .route("/v1/github/auth", get(github_auth::get_auth_url))
        .route("/v1/github/callback", post(github_auth::post_callback))
        .route("/v1/github/status", get(github_auth::get_status))
        .route(
            "/v1/github/connection",
            delete(github_auth::delete_connection),
        )
        .route(
            "/v1/github/disconnect",
            delete(github_auth::delete_connection),
        )
        // Blanket audit is the innermost layer (added first) so it wraps the
        // handler and runs after auth has set the AuthContext. It records an
        // audit entry for every successful mutating request whose handler does
        // not already self-log (see middleware::AUDIT_SKIP_PATTERNS).
        .layer(middleware::from_fn_with_state(store.conn(), api_mw::audit))
        // Rate limit runs after auth (inner layer = runs second at runtime).
        // Auth is outermost (last `.layer()`) so it runs first.
        .layer(middleware::from_fn_with_state(
            rate_state,
            rate_limit::rate_limit,
        ))
        .layer(middleware::from_fn_with_state(store.conn(), api_mw::auth));

    let cors_origins = config.cors_origins.clone();
    let admin_origin = config.admin_origin.clone();

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            move |origin: &axum::http::HeaderValue, _parts: &axum::http::request::Parts| {
                if cors_origins == "*" {
                    return true;
                }
                let origin_str = match origin.to_str() {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                if origin_str == admin_origin {
                    return true;
                }
                // Allow Cloudflare Pages preview deployments of our own frontends.
                // Preview URLs carry a per-deploy hash prefix (e.g.
                // https://ded118df.nexusmind-backoffice.pages.dev), so an exact
                // allowlist can't keep up — match the project suffix instead.
                const PAGES_PREVIEW_SUFFIXES: &[&str] = &[
                    ".nexusmind-backoffice.pages.dev",
                    ".nexusmind-admin.pages.dev",
                    ".nexusmind-landing.pages.dev",
                ];
                if origin_str.starts_with("https://")
                    && PAGES_PREVIEW_SUFFIXES
                        .iter()
                        .any(|s| origin_str.ends_with(s))
                {
                    return true;
                }
                cors_origins
                    .split(',')
                    .any(|allowed| allowed.trim() == origin_str)
            },
        ))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ])
        .allow_credentials(true);

    let internal_routes = Router::new()
        .route("/internal/metrics", get(internal::get_metrics))
        .route(
            "/internal/orgs",
            get(internal::list_orgs).post(internal::create_org),
        )
        .route(
            "/internal/orgs/:id",
            get(internal::get_org)
                .patch(internal::update_org)
                .delete(internal::delete_org),
        )
        .route("/internal/orgs/:id/users", get(internal::list_org_users))
        .route(
            "/internal/orgs/:id/impersonate",
            post(internal::impersonate_org),
        )
        .route("/internal/users", get(internal::list_users))
        .route("/internal/users/:id/suspend", post(internal::suspend_user))
        .route("/internal/audit", get(internal::list_audit))
        .route("/internal/search", get(internal::internal_search));

    let router = Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/orgs", get(admin::list_orgs).post(admin::create_org))
        .route("/v1/orgs/:id/users", get(admin::list_org_users))
        .route("/v1/admin/auth/login", post(auth::login))
        .route("/v1/admin/auth/set-password", post(auth::set_password))
        .route("/v1/admin/auth/request-reset", post(auth::request_reset))
        .route("/v1/admin/auth/logout", post(auth::logout))
        .route("/v1/auth/forgot-password", post(auth::request_reset))
        .route("/v1/auth/reset-password/confirm", post(auth::set_password))
        .route(
            "/v1/autonomous-agents/github/webhook",
            post(autonomous_webhooks::github_webhook),
        )
        // Public LinkedIn OAuth redirect target — LinkedIn calls it directly, so it
        // cannot carry a session; the signed `state` carries org/user/destination.
        .route(
            "/v1/autonomous-agents/linkedin/callback",
            get(autonomous_agents::linkedin_callback),
        )
        // Public evidence redirect: re-signs R2 screenshot URLs on every request so
        // finding images (and GitHub-embedded evidence) never expire. Unauthenticated
        // by necessity — <img>/GitHub camo cannot send auth headers.
        .route(
            "/evidence/:run_id/:name",
            get(autonomous_agents::get_evidence),
        )
        .route("/v1/invites/:token", get(admin::get_invite_link))
        .route("/v1/invites/:token/redeem", post(admin::redeem_invite))
        .merge(protected)
        .merge(internal_routes)
        .layer(Extension(email_config))
        .layer(Extension(config.superuser_key.clone()))
        .layer(Extension(Arc::clone(&config)))
        .layer(cors)
        .layer(CookieManagerLayer::new())
        .layer(middleware::from_fn(api_mw::accept_json))
        .layer(TraceLayer::new_for_http())
        .with_state(store.clone());

    (router, store)
}
