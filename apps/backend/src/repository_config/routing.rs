use super::{ConfigError, ConfigSnapshot, ProjectConfig};
use globset::{GlobBuilder, GlobMatcher};
use serde::Serialize;
use std::path::{Component, Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Specificity(
    pub usize,
    pub usize,
    pub std::cmp::Reverse<usize>,
    pub std::cmp::Reverse<usize>,
    pub usize,
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "selection", rename_all = "snake_case")]
pub enum ResolutionBasis {
    Pattern {
        pattern: String,
        specificity: Specificity,
    },
    Default,
    ExplicitOverride,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedProject {
    pub alias: String,
    pub project_id: String,
    pub client_id: Option<String>,
    pub basis: ResolutionBasis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResolutionStatus {
    Resolved(ResolvedProject),
    Unmapped,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DestinationOverride {
    pub project_id: Option<String>,
    pub client_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedGroup {
    pub destination: ResolvedProject,
    pub item_indices: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoutingPlan {
    pub groups: Vec<PlannedGroup>,
    pub unmapped_indices: Vec<usize>,
}

struct CompiledPattern {
    raw: String,
    matcher: GlobMatcher,
    specificity: Specificity,
}
struct CompiledProject {
    alias: String,
    config: ProjectConfig,
    paths: Vec<CompiledPattern>,
    exclude: Vec<GlobMatcher>,
}

pub struct ProjectResolver {
    snapshot: ConfigSnapshot,
    projects: Vec<CompiledProject>,
}

impl ProjectResolver {
    pub fn compile(snapshot: ConfigSnapshot) -> Result<Self, ConfigError> {
        let mut projects = Vec::new();
        for (alias, project) in &snapshot.config.projects {
            let paths = project
                .paths
                .iter()
                .map(|p| {
                    compile_pattern(p).map(|(matcher, specificity)| CompiledPattern {
                        raw: p.clone(),
                        matcher,
                        specificity,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let exclude = project
                .exclude
                .iter()
                .map(|p| compile_pattern(p).map(|v| v.0))
                .collect::<Result<Vec<_>, _>>()?;
            projects.push(CompiledProject {
                alias: alias.clone(),
                config: project.clone(),
                paths,
                exclude,
            });
        }
        Ok(Self { snapshot, projects })
    }

    pub fn resolve(&self, path: &Path) -> Result<ResolutionStatus, ConfigError> {
        self.resolve_with_override(path, &DestinationOverride::default())
    }

    pub fn resolve_with_override(
        &self,
        path: &Path,
        override_: &DestinationOverride,
    ) -> Result<ResolutionStatus, ConfigError> {
        if override_.client_id.is_some() && override_.project_id.is_none() {
            return Err(ConfigError::new(
                "ROUTING_OVERRIDE_INVALID",
                "an explicit client requires an explicit project",
            ));
        }
        if let Some(project_id) = override_.project_id.as_deref() {
            if project_id.trim().is_empty() {
                return Err(ConfigError::new(
                    "ROUTING_OVERRIDE_INVALID",
                    "explicit project must not be blank",
                ));
            }
            return Ok(ResolutionStatus::Resolved(ResolvedProject {
                alias: self
                    .projects
                    .iter()
                    .find(|p| p.config.project_id == project_id)
                    .map(|p| p.alias.clone())
                    .unwrap_or_else(|| "explicit".to_string()),
                project_id: project_id.to_string(),
                client_id: override_.client_id.clone(),
                basis: ResolutionBasis::ExplicitOverride,
            }));
        }
        let normalized = normalize_path(path)?;
        let mut matches = Vec::new();
        for project in &self.projects {
            if project.exclude.iter().any(|p| p.is_match(&normalized)) {
                continue;
            }
            if let Some(best) = project
                .paths
                .iter()
                .filter(|p| p.matcher.is_match(&normalized))
                .max_by(|a, b| {
                    a.specificity
                        .cmp(&b.specificity)
                        .then_with(|| b.raw.cmp(&a.raw))
                })
            {
                matches.push((best.specificity, project, best.raw.as_str()));
            }
        }
        matches.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        if let Some((score, project, pattern)) = matches.first() {
            if matches
                .get(1)
                .is_some_and(|other| other.0 == *score && other.1.alias != project.alias)
            {
                return Err(ConfigError::new(
                    "ROUTING_AMBIGUOUS",
                    format!("`{normalized}` matches equally specific projects"),
                ));
            }
            return Ok(ResolutionStatus::Resolved(resolved(
                project,
                ResolutionBasis::Pattern {
                    pattern: (*pattern).to_string(),
                    specificity: *score,
                },
            )));
        }
        if let Some(alias) = self.snapshot.config.defaults.project.as_deref() {
            let project = self
                .projects
                .iter()
                .find(|p| p.alias == alias)
                .expect("validated default");
            return Ok(ResolutionStatus::Resolved(resolved(
                project,
                ResolutionBasis::Default,
            )));
        }
        Ok(ResolutionStatus::Unmapped)
    }

    pub fn snapshot(&self) -> &ConfigSnapshot {
        &self.snapshot
    }

    pub fn plan_paths<'a, I>(
        &self,
        paths: I,
        override_: &DestinationOverride,
    ) -> Result<RoutingPlan, ConfigError>
    where
        I: IntoIterator<Item = Option<&'a str>>,
    {
        let mut grouped: std::collections::BTreeMap<(String, Option<String>), PlannedGroup> =
            std::collections::BTreeMap::new();
        let mut unmapped_indices = Vec::new();
        for (index, path) in paths.into_iter().enumerate() {
            let status = match path {
                Some(path) => self.resolve_with_override(Path::new(path), override_)?,
                None if override_.project_id.is_some() => {
                    self.resolve_with_override(Path::new(""), override_)?
                }
                None => self.resolve_default(),
            };
            match status {
                ResolutionStatus::Resolved(destination) => {
                    let key = (
                        destination.project_id.clone(),
                        destination.client_id.clone(),
                    );
                    grouped
                        .entry(key)
                        .or_insert_with(|| PlannedGroup {
                            destination: destination.clone(),
                            item_indices: Vec::new(),
                        })
                        .item_indices
                        .push(index);
                }
                ResolutionStatus::Unmapped => unmapped_indices.push(index),
            }
        }
        Ok(RoutingPlan {
            groups: grouped.into_values().collect(),
            unmapped_indices,
        })
    }

    fn resolve_default(&self) -> ResolutionStatus {
        let Some(alias) = self.snapshot.config.defaults.project.as_deref() else {
            return ResolutionStatus::Unmapped;
        };
        let project = self
            .projects
            .iter()
            .find(|p| p.alias == alias)
            .expect("validated default");
        ResolutionStatus::Resolved(resolved(project, ResolutionBasis::Default))
    }
}

fn resolved(project: &CompiledProject, basis: ResolutionBasis) -> ResolvedProject {
    ResolvedProject {
        alias: project.alias.clone(),
        project_id: project.config.project_id.clone(),
        client_id: project.config.client_id.clone(),
        basis,
    }
}

fn normalize_path(path: &Path) -> Result<String, ConfigError> {
    if path.is_absolute() {
        return Err(ConfigError::new(
            "ROUTING_INVALID_PATTERN",
            "routing paths must be repository-relative",
        ));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(v) => parts.push(v.to_str().ok_or_else(|| {
                ConfigError::new("ROUTING_INVALID_PATTERN", "routing path must be UTF-8")
            })?),
            Component::CurDir => {}
            _ => {
                return Err(ConfigError::new(
                    "ROUTING_INVALID_PATTERN",
                    "routing path escapes repository",
                ))
            }
        }
    }
    Ok(parts.join("/"))
}

fn compile_pattern(raw: &str) -> Result<(GlobMatcher, Specificity), ConfigError> {
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.contains('\\')
        || raw.contains('[')
        || raw.contains(']')
        || raw.contains('{')
        || raw.contains('}')
        || raw.contains("..")
    {
        return Err(ConfigError::new(
            "ROUTING_INVALID_PATTERN",
            format!("unsupported repository pattern `{raw}`"),
        ));
    }
    let segments: Vec<&str> = raw.split('/').collect();
    if segments
        .iter()
        .any(|s| s.is_empty() || *s == "." || (s.contains("**") && *s != "**"))
    {
        return Err(ConfigError::new(
            "ROUTING_INVALID_PATTERN",
            format!("unsupported repository pattern `{raw}`"),
        ));
    }
    let literal_segments = segments
        .iter()
        .filter(|s| !s.contains('*') && !s.contains('?'))
        .count();
    let literal_chars = raw
        .chars()
        .filter(|c| *c != '*' && *c != '?' && *c != '/')
        .count();
    let double_star = segments.iter().filter(|s| **s == "**").count();
    let wildcards = raw.matches('*').count() - double_star * 2 + raw.matches('?').count();
    let glob = GlobBuilder::new(raw)
        .literal_separator(true)
        .build()
        .map_err(|_| {
            ConfigError::new(
                "ROUTING_INVALID_PATTERN",
                format!("invalid repository pattern `{raw}`"),
            )
        })?;
    Ok((
        glob.compile_matcher(),
        Specificity(
            literal_segments,
            literal_chars,
            std::cmp::Reverse(double_star),
            std::cmp::Reverse(wildcards),
            segments.len(),
        ),
    ))
}
