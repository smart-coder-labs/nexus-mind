use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{ErrorKind, Write},
    path::Path,
    time::Duration,
};

#[derive(Clone, Debug, Default)]
pub struct ContextBundle {
    pub text: String,
    pub sources: Vec<String>,
}

fn config_slug(value: &str) -> String {
    let mut slug = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 64 {
            break;
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// Create the public, schema-v1 repository config once. Existing project routing
/// belongs to the repository owner and must never be replaced by `nexus init`.
pub fn initialize_repository_config(
    root: &Path,
    project: Option<&str>,
    project_id: Option<&str>,
) -> Result<Option<String>> {
    let path = root.join(".nexusmind.yaml");
    if path.exists() {
        let body = fs::read_to_string(&path).context("could not read existing .nexusmind.yaml")?;
        let config: serde_yaml::Value =
            serde_yaml::from_str(&body).context("existing .nexusmind.yaml is invalid YAML")?;
        let selected = project.map(str::to_string).or_else(|| {
            config
                .get("defaults")?
                .get("project")?
                .as_str()
                .map(str::to_string)
        });
        return Ok(selected);
    }
    let requested = project.unwrap_or_else(|| {
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
    });
    let alias = config_slug(requested);
    if alias.is_empty() {
        bail!("project name must contain an ASCII letter or number");
    }
    let id = project_id.unwrap_or(requested).trim();
    if id.is_empty() || id.len() > 255 {
        bail!("project ID must have 1–255 characters");
    }
    let repository_id = config_slug(
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repository"),
    );
    if repository_id.is_empty() {
        bail!("repository name must contain an ASCII letter or number");
    }
    let config = json!({
        "version": 1,
        "repository": {"id": repository_id},
        "defaults": {"project": alias, "agent_profile": "essential"},
        "projects": {alias.clone(): {"project_id": id, "paths": ["**"]}},
        "agents": {"profiles": {"essential": {"capabilities": ["context.read", "memory.read", "task.read", "code.read"]}}}
    });
    let body = serde_yaml::to_string(&config)?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => file
            .write_all(body.as_bytes())
            .context("could not write .nexusmind.yaml")?,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return initialize_repository_config(root, project, project_id);
        }
        Err(error) => return Err(error).context("could not create .nexusmind.yaml"),
    }
    // The REST search API accepts the caller's project name, while the YAML
    // routing key must be a schema-valid slug. Preserve the former locally.
    Ok(Some(project.unwrap_or(&alias).to_string()))
}

fn project_name(root: &Path) -> String {
    if let Ok(value) = env::var("NEXUSMIND_PROJECT") {
        if !value.trim().is_empty() {
            return value;
        }
    }
    if let Ok(body) = fs::read(root.join(".nexus/config.json")) {
        if let Ok(value) = serde_json::from_slice::<Value>(&body) {
            if let Some(project) = value.get("project").and_then(Value::as_str) {
                if !project.trim().is_empty() {
                    return project.to_string();
                }
            }
        }
    }
    let config_path = root.join(".nexusmind.yaml");
    if let Ok(body) = fs::read_to_string(config_path) {
        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&body) {
            if let Some(project) = value
                .get("defaults")
                .and_then(|v| v.get("project"))
                .and_then(|v| v.as_str())
            {
                return project.to_string();
            }
        }
    }
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default")
        .to_string()
}

pub fn configured() -> bool {
    ["NEXUSMIND_BASE_URL", "NEXUSMIND_API_KEY"]
        .into_iter()
        .all(|name| env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

/// Retrieve compact, scoped evidence before invoking a coding runtime. This is
/// optional for offline sessions; a half-configured client is an error.
pub fn retrieve(root: &Path, objective: &str) -> Result<Option<ContextBundle>> {
    let base = env::var("NEXUSMIND_BASE_URL").ok();
    let key = env::var("NEXUSMIND_API_KEY").ok();
    let (base, key) = match (base, key) {
        (None, None) => return Ok(None),
        (Some(base), Some(key)) if !base.trim().is_empty() && !key.trim().is_empty() => (base, key),
        _ => bail!("set both NEXUSMIND_BASE_URL and NEXUSMIND_API_KEY"),
    };
    let client = Client::builder().timeout(Duration::from_secs(8)).build()?;
    let project = project_name(root);
    let query = objective.chars().take(1800).collect::<String>();
    let mut bundle = ContextBundle::default();
    let base = base.trim_end_matches('/');

    let memory_response = client
        .post(format!("{base}/v1/memory/search"))
        .bearer_auth(&key)
        .json(&json!({"query": query, "project": project, "limit": 5, "compact": true}))
        .send()
        .context("NexusMind memory search failed")?;
    if !memory_response.status().is_success() {
        bail!(
            "NexusMind memory search returned HTTP {}",
            memory_response.status()
        );
    }
    let memory_data: Value = memory_response
        .json()
        .context("invalid NexusMind memory response")?;
    if let Some(memories) = memory_data.get("memories").and_then(Value::as_array) {
        for memory in memories.iter().take(5) {
            let title = memory
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("memory");
            let preview = memory.get("preview").and_then(Value::as_str).unwrap_or("");
            let id = memory.get("id").and_then(Value::as_str).unwrap_or("");
            bundle
                .text
                .push_str(&format!("Memory {title}: {preview}\n"));
            if !id.is_empty() {
                bundle.sources.push(format!("memory:{id}"));
            }
        }
    }

    let code_response = client
        .post(format!("{base}/v1/code/search"))
        .bearer_auth(&key)
        .json(&json!({"query": query, "project": project, "top_k": 4}))
        .send()
        .context("NexusMind code search failed")?;
    if code_response.status().is_success() {
        let code_data: Value = code_response
            .json()
            .context("invalid NexusMind code response")?;
        if let Some(hits) = code_data.as_array() {
            for hit in hits.iter().take(4) {
                let path = hit.get("file_path").and_then(Value::as_str).unwrap_or("");
                let symbol = hit.get("symbol").and_then(Value::as_str).unwrap_or("");
                let content = hit
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(800)
                    .collect::<String>();
                if !path.is_empty() {
                    bundle
                        .text
                        .push_str(&format!("Code {path} {symbol}:\n{content}\n"));
                    bundle.sources.push(format!("code:{path}:{symbol}"));
                }
            }
        }
    } else if code_response.status().as_u16() != 404 {
        bail!(
            "NexusMind code search returned HTTP {}",
            code_response.status()
        );
    }
    Ok(Some(bundle))
}

#[cfg(test)]
mod init_tests {
    use super::*;

    #[test]
    fn creates_schema_v1_config_with_essential_profile() {
        let root = env::temp_dir().join(format!("nexus-init-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let selected =
            initialize_repository_config(&root, Some("My App"), Some("prj-real-id")).unwrap();
        assert_eq!(selected.as_deref(), Some("My App"));
        let config: Value =
            serde_yaml::from_str(&fs::read_to_string(root.join(".nexusmind.yaml")).unwrap())
                .unwrap();
        assert_eq!(config["version"], 1);
        assert_eq!(config["defaults"]["project"], "my-app");
        assert_eq!(config["defaults"]["agent_profile"], "essential");
        assert_eq!(config["projects"]["my-app"]["project_id"], "prj-real-id");
        assert_eq!(config["projects"]["my-app"]["paths"][0], "**");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_config_is_not_overwritten() {
        let root = env::temp_dir().join(format!("nexus-init-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let original = "version: 1\ndefaults:\n  project: existing\n";
        fs::write(root.join(".nexusmind.yaml"), original).unwrap();
        assert_eq!(
            initialize_repository_config(&root, None, None)
                .unwrap()
                .as_deref(),
            Some("existing")
        );
        assert_eq!(
            fs::read_to_string(root.join(".nexusmind.yaml")).unwrap(),
            original
        );
        fs::remove_dir_all(root).unwrap();
    }
}
