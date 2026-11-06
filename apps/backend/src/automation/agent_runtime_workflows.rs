//! Resolve per-step resources without changing workspace infrastructure or phase policies.
use anyhow::{bail, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use crate::factory::workflows::Definition;
use super::agent_runtime::{prepare_config, plan};

pub fn prepare(conn: &Connection, org: &str, user: &str, definition: &Definition) -> Result<Definition> {
    let mut d = definition.clone();
    for step in &mut d.steps {
        let Some(options) = step.options.as_object_mut() else { continue };
        options.remove("runtime_snapshot");
        if options.get("agent_id").is_some_and(|v| !v.is_string()) || options.get("executor").is_some_and(|v| !v.is_string()) { bail!("invalid_agent_selection"); }
        let agent_id = options.get("agent_id").and_then(Value::as_str).filter(|id| !id.is_empty());
        if agent_id.is_none() && !options.contains_key("runtime") && !options.contains_key("executor") { continue }
        let agent = agent_id.map(|id| crate::db::queries::get_autonomous_agent_detail(conn, org, id)
            .and_then(|a| a.ok_or_else(|| anyhow::anyhow!("agent_not_found")))).transpose()?;
        if agent.as_ref().is_some_and(|a| a.definition.status == "archived") { bail!("agent_archived"); }
        let executor = options.get("executor").and_then(Value::as_str)
            .or_else(|| agent.as_ref().and_then(|a| a.revision.config["executor"].as_str())).unwrap_or("claude").to_string();
        let mut runtime = agent.as_ref().and_then(|a| a.revision.config.get("runtime")).cloned().unwrap_or_else(|| json!({}));
        if let Some(overrides) = options.get("runtime") {
            let overrides = overrides.as_object().ok_or_else(|| anyhow::anyhow!("invalid_agent_runtime"))?;
            for (key, value) in overrides { runtime[key] = value.clone(); }
        }
        let config = prepare_config(conn, org, user, &json!({"executor":executor,"runtime":runtime}))?;
        let resolved = serde_json::from_value(config["runtime_resolved"].clone())?;
        let plan = plan(&resolved, &executor, "/tmp/nexusmind-agent-runtime", json!({"command":"nexusmind-mcp","env":{}}))?;
        options.insert("runtime_snapshot".into(), json!({"executor":executor,"config":config,"plan":plan,"approved_by":user,"agent_revision_id":agent.as_ref().map(|a| &a.revision.id)}));
        for capability in ["agent-runtime:v2".to_string(), format!("executor:{executor}")] {
            if !step.capabilities.contains(&capability) { step.capabilities.push(capability); }
        }
    }
    if serde_json::to_vec(&d)?.len() > 4 * 1_048_576 { bail!("workflow_runtime_snapshot_too_large"); }
    Ok(d)
}

pub fn verify(conn: &Connection, org: &str, creator: &str, definition: &Definition) -> Result<()> {
    for step in &definition.steps {
        if let Some(snapshot) = step.options.get("runtime_snapshot") {
            let user = snapshot["approved_by"].as_str().ok_or_else(|| anyhow::anyhow!("runtime_approval_missing"))?;
            if user != creator { bail!("runtime_approval_mismatch"); }
            let current = prepare_config(conn, org, user, &snapshot["config"])?;
            if current != snapshot["config"] { bail!("runtime_snapshot_changed"); }
            let executor = crate::db::queries::autonomous_executor(&current)?;
            let runtime = serde_json::from_value(current["runtime_resolved"].clone())?;
            let plan = plan(&runtime, executor, "/tmp/nexusmind-agent-runtime", json!({"command":"nexusmind-mcp","env":{}}))?;
            if serde_json::to_value(plan)? != snapshot["plan"] || snapshot["executor"].as_str() != Some(executor) { bail!("runtime_plan_mismatch"); }
        }
    }
    Ok(())
}
