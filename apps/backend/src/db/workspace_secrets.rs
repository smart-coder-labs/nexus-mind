//! Per-repository workspace secrets: a `.env` written into the checkout and an
//! `.npmrc` used only by the runner to download lockfile-pinned packages.
//!
//! Values are encrypted at rest with [`crate::crypto`] and are write-only for
//! people: the admin API returns key names and registry hosts, never values.
//! A runner receives plaintext only for a step it currently leases, and never
//! for an independent review step.
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub const MAX_CONTENT: usize = 64 * 1024;
pub const KINDS: [&str; 2] = ["env", "npmrc"];

pub fn migrate(c: &Connection) -> Result<()> {
    if c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? >= 98 {
        return Ok(());
    }
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    if tx.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? < 98 {
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS factory_workspace_repo_secrets(\
                workspace_id TEXT NOT NULL REFERENCES factory_workspaces(id) ON DELETE CASCADE,\
                repository TEXT NOT NULL,\
                kind TEXT NOT NULL CHECK(kind IN ('env','npmrc')),\
                ciphertext TEXT NOT NULL,\
                summary_json TEXT NOT NULL DEFAULT '{}',\
                updated_by TEXT REFERENCES users(id),\
                updated_at TEXT NOT NULL DEFAULT(datetime('now')),\
                PRIMARY KEY(workspace_id,repository,kind)); \
             PRAGMA user_version=98;",
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn env_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        && key.len() <= 128
}

/// Literal `KEY=value` lines (Docker env-file syntax, never shell). Returns key names.
pub fn validate_env(content: &str) -> Result<Vec<String>> {
    if content.len() > MAX_CONTENT || content.contains('\0') {
        bail!("repo_secret_too_large")
    }
    let mut keys = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, _)) = line.split_once('=') else { bail!("repo_secret_env_invalid") };
        if !env_key(key.trim()) {
            bail!("repo_secret_env_invalid")
        }
        let key = key.trim().to_string();
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    if keys.is_empty() {
        bail!("repo_secret_env_invalid")
    }
    Ok(keys)
}

fn https_registry(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url.trim()).ok()?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() || parsed.query().is_some() || parsed.fragment().is_some() {
        return None;
    }
    parsed.host_str().map(|h| h.to_ascii_lowercase())
}

/// Only registry routing and registry authentication keys. Anything that can
/// change how npm executes (scripts, shells, node options, git, prefixes or
/// other config files) is refused, as is `${VAR}` interpolation, which would
/// silently resolve to nothing on the runner.
pub fn validate_npmrc(content: &str) -> Result<Value> {
    if content.len() > MAX_CONTENT || content.contains('\0') {
        bail!("repo_secret_too_large")
    }
    let mut registries: Vec<String> = Vec::new();
    let mut scopes: Vec<String> = Vec::new();
    let mut authenticated: Vec<String> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { bail!("repo_secret_npmrc_invalid") };
        let (key, value) = (key.trim(), value.trim());
        if value.is_empty() {
            bail!("repo_secret_npmrc_invalid")
        }
        if value.contains("${") {
            bail!("repo_secret_npmrc_interpolation")
        }
        if key == "registry" || key == "always-auth" {
            if key == "registry" {
                let host = https_registry(value).ok_or_else(|| anyhow::anyhow!("repo_secret_npmrc_registry"))?;
                if !registries.contains(&host) { registries.push(host); }
            }
            continue;
        }
        if let Some(scope) = key.strip_suffix(":registry") {
            let valid_scope = scope.len() > 1 && scope.starts_with('@') && scope[1..].chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c));
            if !valid_scope {
                bail!("repo_secret_npmrc_invalid")
            }
            let host = https_registry(value).ok_or_else(|| anyhow::anyhow!("repo_secret_npmrc_registry"))?;
            if !registries.contains(&host) { registries.push(host); }
            let scope = scope.to_ascii_lowercase();
            if !scopes.contains(&scope) { scopes.push(scope); }
            continue;
        }
        if let Some(rest) = key.strip_prefix("//") {
            let Some((location, setting)) = rest.rsplit_once(':') else { bail!("repo_secret_npmrc_invalid") };
            if !["_authToken", "_auth", "username", "_password", "email", "always-auth"].contains(&setting) {
                bail!("repo_secret_npmrc_key_not_allowed")
            }
            let host = location.split('/').next().unwrap_or("").to_ascii_lowercase();
            if host.is_empty() || host.contains('@') {
                bail!("repo_secret_npmrc_invalid")
            }
            if ["_authToken", "_auth", "_password"].contains(&setting) && !authenticated.contains(&host) {
                authenticated.push(host);
            }
            continue;
        }
        bail!("repo_secret_npmrc_key_not_allowed")
    }
    if registries.is_empty() && authenticated.is_empty() {
        bail!("repo_secret_npmrc_invalid")
    }
    Ok(json!({"registries": registries, "scopes": scopes, "authenticated": authenticated}))
}

fn workspace_repositories(c: &Connection, org: &str, workspace: &str) -> Result<Vec<String>> {
    let repos: String = c
        .query_row(
            "SELECT repositories_json FROM factory_workspaces WHERE id=?1 AND org_id=?2",
            params![workspace, org],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("not_found"))?;
    let repos: Vec<Value> = serde_json::from_str(&repos)?;
    Ok(repos.iter().filter_map(|r| r["path"].as_str().map(str::to_string)).collect())
}

pub fn list(c: &Connection, org: &str, workspace: &str) -> Result<Value> {
    workspace_repositories(c, org, workspace)?;
    let mut s = c.prepare(
        "SELECT repository,kind,summary_json,updated_at FROM factory_workspace_repo_secrets \
         WHERE workspace_id=?1 ORDER BY repository,kind",
    )?;
    let rows = s
        .query_map([workspace], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let secrets: Vec<Value> = rows
        .into_iter()
        .map(|(repository, kind, summary, updated_at)| {
            json!({"repository": repository, "kind": kind, "summary": serde_json::from_str::<Value>(&summary).unwrap_or_else(|_| json!({})), "updated_at": updated_at})
        })
        .collect();
    Ok(json!({"secrets": secrets, "encryption_configured": crate::crypto::is_configured()}))
}

pub fn save(c: &Connection, org: &str, user: &str, workspace: &str, repository: &str, kind: &str, content: &str) -> Result<Value> {
    if !workspace_repositories(c, org, workspace)?.iter().any(|r| r == repository) {
        bail!("repo_secret_repository_invalid")
    }
    let summary = match kind {
        "env" => json!({"keys": validate_env(content)?}),
        "npmrc" => validate_npmrc(content)?,
        _ => bail!("repo_secret_kind_invalid"),
    };
    if !crate::crypto::is_configured() {
        bail!("encryption_required")
    }
    let ciphertext = crate::crypto::encrypt(content).ok_or_else(|| anyhow::anyhow!("encryption_required"))?;
    c.execute(
        "INSERT INTO factory_workspace_repo_secrets(workspace_id,repository,kind,ciphertext,summary_json,updated_by) VALUES (?1,?2,?3,?4,?5,?6) \
         ON CONFLICT(workspace_id,repository,kind) DO UPDATE SET ciphertext=excluded.ciphertext,summary_json=excluded.summary_json,updated_by=excluded.updated_by,updated_at=datetime('now')",
        params![workspace, repository, kind, ciphertext, summary.to_string(), user],
    )?;
    list(c, org, workspace)
}

pub fn delete(c: &Connection, org: &str, workspace: &str, repository: &str, kind: &str) -> Result<Value> {
    workspace_repositories(c, org, workspace)?;
    if !KINDS.contains(&kind) {
        bail!("repo_secret_kind_invalid")
    }
    c.execute(
        "DELETE FROM factory_workspace_repo_secrets WHERE workspace_id=?1 AND repository=?2 AND kind=?3",
        params![workspace, repository, kind],
    )?;
    list(c, org, workspace)
}

/// Plaintext secrets for the repositories pinned in a run, gated by the step lease.
pub fn for_runner(c: &Connection, org: &str, runner: &str, run: &str, step: &str, lease_token: &str, now: i64) -> Result<Value> {
    super::workflow_execution::lease(c, org, runner, run, step, lease_token, now)?;
    let snapshot: String = c.query_row("SELECT snapshot_json FROM factory_workflow_runs WHERE id=?1", [run], |r| r.get(0))?;
    let snapshot: Value = serde_json::from_str(&snapshot)?;
    let kind = snapshot["definition"]["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|s| s["id"] == step))
        .and_then(|s| s["kind"].as_str())
        .ok_or_else(|| anyhow::anyhow!("lease_conflict"))?;
    if kind == "review" {
        // Independent reviewers reason about code and evidence, never secrets.
        return Ok(json!({"repositories": {}}));
    }
    let workspace = snapshot["workspace"]["id"].as_str().ok_or_else(|| anyhow::anyhow!("not_found"))?;
    let pinned: Vec<&str> = snapshot["workspace"]["repositories"]
        .as_array()
        .map(|repos| repos.iter().filter_map(|r| r["path"].as_str()).collect())
        .unwrap_or_default();
    let mut s = c.prepare("SELECT repository,kind,ciphertext FROM factory_workspace_repo_secrets WHERE workspace_id=?1")?;
    let rows = s
        .query_map([workspace], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut repositories = serde_json::Map::new();
    let mut provided = Vec::new();
    for (repository, kind, ciphertext) in rows {
        if !pinned.contains(&repository.as_str()) {
            continue;
        }
        let plaintext = crate::crypto::decrypt(&ciphertext).ok_or_else(|| anyhow::anyhow!("repo_secret_unavailable"))?;
        repositories
            .entry(repository.clone())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("object")
            .insert(kind.clone(), Value::String(plaintext));
        provided.push(json!({"repository": repository, "kind": kind}));
    }
    if !provided.is_empty() {
        // Audit which secrets reached the runner, never their values.
        super::workflows::event(c, run, Some(step), "secrets.provided", json!({"secrets": provided}))?;
    }
    Ok(json!({"repositories": repositories}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_requires_literal_assignments() {
        assert_eq!(validate_env("# c\nA=1\nexport B_2=\"x y\"\n\nA=3").unwrap(), vec!["A", "B_2"]);
        assert!(validate_env("not an assignment").is_err());
        assert!(validate_env("1A=x").is_err());
        assert!(validate_env("").is_err());
    }

    #[test]
    fn npmrc_accepts_registry_routing_and_auth_only() {
        let summary = validate_npmrc(
            "@kasymir:registry=https://npm.pkg.github.com\n//npm.pkg.github.com/:_authToken=ghp_example\nalways-auth=true\n",
        )
        .unwrap();
        assert_eq!(summary["scopes"], json!(["@kasymir"]));
        assert_eq!(summary["registries"], json!(["npm.pkg.github.com"]));
        assert_eq!(summary["authenticated"], json!(["npm.pkg.github.com"]));
        for bad in [
            "script-shell=/bin/sh",
            "node-options=--require x",
            "registry=http://insecure.example",
            "@x:registry=https://user:pass@host",
            "//npm.pkg.github.com/:_authToken=${NPM_TOKEN}",
            "//host/:cafile=/etc/x",
            "globalconfig=/tmp/x",
        ] {
            assert!(validate_npmrc(bad).is_err(), "{bad}");
        }
    }
}
