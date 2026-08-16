//! The Claude Code connector: a developer's own machine, turned into candidates.
//!
//! Before NexusMind, everyone accumulated two different things locally and both
//! are still there.
//!
//! **Knowledge** — typed memory files, `MEMORY.md`, `CLAUDE.md`, `AGENTS.md`,
//! `.cursor/rules`. Nobody writes an agent memory out of obligation; they write
//! it because something was expensive to find out.
//!
//! **Tools** — skills, agents, commands, hooks, output styles, plugins, themes,
//! and the configuration that wires them together. This is the harness: the
//! engineering that makes an agent useful in *this* team rather than in the
//! abstract, and the half that hurts more to lose because it costs more to build
//! and gets rebuilt worse.
//!
//! Both halves are private by accident: they live on one laptop and vanish when
//! that person moves on.
//!
//! # Redaction is a precondition, not hygiene
//!
//! `validate_safe_manifest_content` refuses any content containing `/users/`,
//! `bearer `, `ghp_`, `nm_live` or an OpenAI key. Local files are full of
//! exactly that, because they were never written to be shared. Everything here
//! goes through [`super::redact`] **before** a manifest is built, and the hashes
//! are computed from the redacted text — never the original.

use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::redact::{redact, RedactionReport};
use super::{CandidatePayload, Connector, ScanOptions, SourceItem};

/// A component larger than this cannot go in a manifest — the validator caps it.
/// Oversized assets are skipped **with a reason**, never truncated: half a skill
/// does not install.
const MAX_COMPONENT_BYTES: usize = 64 * 1024;

/// Directories whose contents were downloaded from somebody else.
///
/// **Not overridable.** A plugin cache is full of skills and agents obtained from
/// marketplaces — NexusMind's own plugin is in there. Republishing them as the
/// team's harnesses is a licensing problem, not a feature, and a flag that lets
/// you do it is a flag somebody uses without noticing.
const NEVER_SCANNED: &[&str] = &["/plugins/cache/", "/plugins/marketplaces/", "/node_modules/"];

/// Session transcripts. Enormous, low signal, and the most sensitive material on
/// the machine. Out of scope by decision, not by omission.
const TRANSCRIPT_EXTENSIONS: &[&str] = &["jsonl"];

/// Directory names never descended into.
///
/// This walk deliberately sets `git_ignore(false)`, because `.claude/` and
/// `.cursor/` are frequently gitignored and are exactly what we came for. The
/// cost of that decision is that build output and object stores are walked too:
/// over this repository it meant examining 98,325 files to find 249, and taking
/// 27 seconds to do it — long enough that the operator concludes it has hung.
///
/// Only directories that carry no *reported* meaning belong here. A plugin
/// cache is skipped too — but by `is_never_scanned`, which puts it in the
/// report with its reason, so the operator can see the third-party assets were
/// deliberately left alone. Pruning those instead would make them vanish
/// silently, which is a worse report for a scan that is barely faster: over
/// this repository the walk is dominated by build output, not by caches.
const NEVER_DESCENDED: &[&str] = &[".git", "target"];

pub struct ClaudeMemoriesConnector {
    /// `global`, or the slug of the project the material belongs to. Never the
    /// machine or user name — that would be PII inside a primary key.
    pub host_scope: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Agent,
    Skill,
    Command,
    Hook,
    OutputStyle,
    Theme,
}

impl AssetKind {
    /// The harness `format` string this asset maps to.
    pub fn format(&self) -> &'static str {
        match self {
            AssetKind::Agent => "agent",
            AssetKind::Skill => "skill",
            AssetKind::Command => "command",
            AssetKind::Hook => "hook",
            AssetKind::OutputStyle => "output_style",
            AssetKind::Theme => "theme",
        }
    }

    /// The component `kind` the validator demands for this format.
    pub fn component_kind(&self) -> &'static str {
        match self {
            AssetKind::Theme => "theme_json",
            _ => "file",
        }
    }

    pub fn media_type(&self) -> &'static str {
        match self {
            AssetKind::Hook => "text/x-shellscript",
            AssetKind::Theme => "application/json",
            _ => "text/markdown",
        }
    }

    /// `hook` and `claude_code_plugin` must declare themselves executable — the
    /// validator refuses them otherwise, and rightly: they are code that runs on
    /// somebody else's machine.
    pub fn is_executable(&self) -> bool {
        matches!(self, AssetKind::Hook)
    }

    /// Which `.claude/` subdirectory holds this kind, and what a file of that
    /// kind looks like.
    pub fn from_path(path: &str) -> Option<Self> {
        let p = path.to_lowercase();
        if p.contains("/agents/") && p.ends_with(".md") {
            return Some(AssetKind::Agent);
        }
        if p.contains("/commands/") && p.ends_with(".md") {
            return Some(AssetKind::Command);
        }
        if p.contains("/hooks/") && p.ends_with(".sh") {
            return Some(AssetKind::Hook);
        }
        if p.contains("/output-styles/") && p.ends_with(".md") {
            return Some(AssetKind::OutputStyle);
        }
        if p.contains("/themes/") && p.ends_with(".json") {
            return Some(AssetKind::Theme);
        }
        if p.contains("/skills/") && p.ends_with(".md") {
            return Some(AssetKind::Skill);
        }
        None
    }
}

/// Frontmatter of a local memory file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryFrontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    pub memory_type: Option<String>,
}

/// Split `---`-delimited frontmatter from the body.
///
/// Deliberately a small hand parser rather than a YAML dependency: the format is
/// three fields deep and pulling in a parser for it would be more surface than
/// the problem.
///
/// The body is returned as a **slice of the original**, never re-joined from its
/// lines. Rebuilding it would normalise line endings and indentation — the same
/// class of bug that `redact_emails` had, and one that matters here because a
/// memory's fenced code blocks are part of what makes it worth keeping.
pub fn parse_frontmatter(input: &str) -> (MemoryFrontmatter, String) {
    let mut fm = MemoryFrontmatter::default();

    let leading = input.len() - input.trim_start().len();
    let trimmed = &input[leading..];
    let Some(after_open) = trimmed.strip_prefix("---") else {
        return (fm, input.to_string());
    };
    let after_open = after_open.trim_start_matches(['\r', '\n']);

    // Find the closing delimiter line without rebuilding anything.
    let mut offset = 0usize;
    let mut split = None;
    for line in after_open.split_inclusive('\n') {
        if line.trim() == "---" {
            split = Some((&after_open[..offset], &after_open[offset + line.len()..]));
            break;
        }
        offset += line.len();
    }
    let Some((header, body)) = split else {
        // An unterminated block is not frontmatter; treat the whole file as body.
        return (fm, input.to_string());
    };

    for line in header.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        match key.trim() {
            "name" => fm.name = Some(value),
            "description" => fm.description = Some(value),
            // Both the flat `type:` and the nested `metadata.type:` shapes appear
            // in the wild; the nested one arrives as an indented `type:` line.
            "type" => fm.memory_type = Some(value),
            _ => {}
        }
    }

    (fm, body.trim_start_matches(['\r', '\n']).to_string())
}

/// Where a local memory type lands.
///
/// The declared type is the primary signal: these files were already classified
/// by whoever wrote them, and asking a model to reclassify from scratch discards
/// human information and spends tokens on a solved problem.
pub fn destination_for_type(memory_type: Option<&str>) -> (&'static str, Option<&'static str>) {
    match memory_type.unwrap_or("").trim() {
        "feedback" => ("feedback", None),
        "project" => ("project", None),
        "reference" => ("discovery", None),
        // A personal preference turned into a team convention is how one
        // person's habit becomes twelve people's rule. It stays personal, and
        // promoting it requires an explicit human action.
        "user" | "preference" => ("preference", Some("personal")),
        "" => ("discovery", None),
        other => match other {
            "architecture" | "decision" | "bugfix" | "discovery" | "config" | "pattern" => {
                (Box::leak(other.to_string().into_boxed_str()), None)
            }
            _ => ("discovery", None),
        },
    }
}

/// `[[wikilink]]` targets, in order of appearance.
pub fn wikilinks(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let name = after[..end].trim();
        if !name.is_empty() && !out.contains(&name.to_string()) {
            out.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

fn sha256_of(content: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(content.as_bytes())))
}

/// One manifest component, already redacted and hashed from the redacted text.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestComponent {
    pub kind: String,
    pub path: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub content: String,
}

/// Builds a typed manifest the harness validator accepts.
///
/// Hashes are computed from the **redacted** content, because that is what
/// travels. Computing them from the original would produce an integrity mismatch
/// the validator rejects — and would be a lie about what is in the component.
pub fn build_manifest(
    kind: AssetKind,
    relative_path: &str,
    raw_content: &str,
) -> Result<(serde_json::Value, RedactionReport)> {
    let (content, report) = redact(raw_content);

    if content.len() > MAX_COMPONENT_BYTES {
        anyhow::bail!(
            "asset_too_large: {} bytes exceeds the {MAX_COMPONENT_BYTES}-byte manifest cap",
            content.len()
        );
    }
    if relative_path.starts_with('/') || relative_path.contains("..") {
        anyhow::bail!("unsafe_path: manifest paths must be relative");
    }

    let component = serde_json::json!({
        "kind": kind.component_kind(),
        "path": relative_path,
        "media_type": kind.media_type(),
        "size_bytes": content.len(),
        "sha256": sha256_of(&content),
        "content": content,
    });

    let mut security = serde_json::json!({
        "requires_approval": true,
        "secret_scan_status": "passed",
    });
    if kind.is_executable() {
        security["executable"] = serde_json::json!(true);
    }

    let manifest = serde_json::json!({
        "schema_version": "1.1",
        "format": kind.format(),
        "targets": ["claude"],
        "components": [component],
        "provenance": {
            "source": "migration:claude-memories",
            "redaction": report.summary(),
        },
        "security": security,
    });

    // Fail here rather than at commit time, when a human is already waiting.
    crate::models::types::validate_typed_harness_manifest(&manifest)
        .map_err(|e| anyhow::anyhow!("invalid_manifest: {e}"))?;

    Ok((manifest, report))
}

fn rel_is_transcript(path: &str) -> bool {
    TRANSCRIPT_EXTENSIONS
        .iter()
        .any(|ext| path.to_lowercase().ends_with(&format!(".{ext}")))
}

/// Is this path something we must never read?
pub fn is_never_scanned(path: &str) -> bool {
    let p = format!("/{}", path.trim_start_matches('/')).to_lowercase();
    NEVER_SCANNED.iter().any(|frag| p.contains(frag))
        || TRANSCRIPT_EXTENSIONS
            .iter()
            .any(|ext| p.ends_with(&format!(".{ext}")))
}

/// A file that instructs an agent how to behave — a rule, not an observation.
pub fn is_instruction_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or("").to_uppercase();
    name == "CLAUDE.MD"
        || name == "AGENTS.MD"
        || path.to_lowercase().contains("/.cursor/rules")
        || path.to_lowercase().contains("copilot-instructions")
}

/// Configuration that holds credentials. Never a harness version.
pub fn is_credentialed_config(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or("");
    matches!(
        name,
        "settings.json" | "settings.local.json" | ".mcp.json" | "keybindings.json"
    )
}

/// `MEMORY.md` is an index over the other files, not a source in its own right.
pub fn is_memory_index(path: &str) -> bool {
    path.rsplit('/').next().unwrap_or("").eq_ignore_ascii_case("MEMORY.md")
}

impl ClaudeMemoriesConnector {
    pub fn new(host_scope: impl Into<String>) -> Self {
        Self {
            host_scope: host_scope.into(),
        }
    }

    pub fn identity_memory(&self, relpath: &str, content: &str) -> String {
        let sha = Sha256::digest(content.as_bytes());
        format!(
            "claude:{}:{}:{}",
            self.host_scope,
            relpath,
            &hex::encode(sha)[..16]
        )
    }

    pub fn identity_harness(&self, format: &str, relpath: &str, content: &str) -> String {
        let sha = Sha256::digest(content.as_bytes());
        format!(
            "claude-harness:{}:{}:{}",
            format,
            relpath,
            &hex::encode(sha)[..16]
        )
    }

    pub fn identity_config(&self, tool: &str, relpath: &str, content: &str) -> String {
        let sha = Sha256::digest(content.as_bytes());
        format!(
            "claude-config:{}:{}:{}",
            tool,
            relpath,
            &hex::encode(sha)[..16]
        )
    }
}

impl Connector for ClaudeMemoriesConnector {
    fn source_kind(&self) -> &'static str {
        "claude-memories"
    }

    fn scan(&self, opts: &ScanOptions) -> Result<Vec<SourceItem>> {
        Ok(self.scan_report(opts)?.items)
    }

    /// Reports what was skipped as well as what was found.
    ///
    /// The excluded count is the interesting half here: a real `~/.claude`
    /// holds hundreds of cached third-party skills, and an operator deciding
    /// whether to run this needs to see that they were left alone rather than
    /// wonder.
    fn scan_report(&self, opts: &ScanOptions) -> Result<super::ScanReport> {
        let mut report = super::ScanReport::default();
        let root = std::path::Path::new(&opts.root);
        if !root.is_dir() {
            return Ok(report);
        }
        let mut items = Vec::new();

        let walker = ignore::WalkBuilder::new(&opts.root)
            .hidden(false) // `.claude/` and `.cursor/` are hidden by design
            .git_ignore(false)
            .require_git(false)
            .filter_entry(|entry| {
                // Prune whole subtrees rather than filtering their files one by
                // one: the cost of this walk is dominated by directories nobody
                // wants to look inside.
                if entry.file_type().map(|t| t.is_dir()) != Some(true) {
                    return true;
                }
                let name = entry.file_name().to_string_lossy().to_lowercase();
                !NEVER_DESCENDED.contains(&name.as_str())
            })
            .build();

        let mut seen = 0usize;
        for entry in walker.flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let abs = entry.path().to_string_lossy().to_string();
            let rel = abs
                .strip_prefix(&opts.root)
                .unwrap_or(&abs)
                .trim_start_matches('/')
                .to_string();
            seen += 1;
            opts.note(seen, &rel);

            // Not overridable, deliberately: `opts` is never consulted here.
            if is_never_scanned(&rel) {
                report.excluded.push((
                    rel,
                    if rel_is_transcript(&abs) {
                        "session transcripts are out of scope".to_string()
                    } else {
                        "third-party asset — republishing it is a licensing problem".to_string()
                    },
                ));
                continue;
            }
            // Narrowing, applied *after* the non-overridable filter above so
            // an `--include` can never reach into a cache. Without this the
            // flags were accepted, documented, and silently ignored: asking for
            // one directory classified all 249 assets, at the operator's
            // expense. Substring semantics, matching `repo-docs`.
            if !opts.includes.is_empty() && !opts.includes.iter().any(|inc| rel.contains(inc)) {
                report
                    .excluded
                    .push((rel, "outside the requested --include paths".to_string()));
                continue;
            }
            if opts.excludes.iter().any(|exc| rel.contains(exc)) {
                report
                    .excluded
                    .push((rel, "matched an --exclude path".to_string()));
                continue;
            }
            if is_memory_index(&rel) {
                report
                    .excluded
                    .push((rel, "an index over the other files, not a source".to_string()));
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            if raw.trim().is_empty() {
                continue;
            }

            let category = if is_credentialed_config(&rel) {
                "config"
            } else if AssetKind::from_path(&rel).is_some() {
                "harness"
            } else if is_instruction_file(&rel) {
                "instruction"
            } else if rel.ends_with(".md") {
                "memory"
            } else {
                continue;
            };

            // Counted here, not at read time, so `documents` means the same in
            // every connector: files that actually produced a unit. A file read
            // and skipped is not a document scanned.
            report.documents += 1;
            items.push(SourceItem {
                source_identity: match category {
                    "harness" => self.identity_harness(
                        AssetKind::from_path(&rel).unwrap().format(),
                        &rel,
                        &raw,
                    ),
                    "config" => self.identity_config("claude", &rel, &raw),
                    _ => self.identity_memory(&rel, &raw),
                },
                display_origin: rel.clone(),
                raw,
                meta: serde_json::json!({ "path": rel, "category": category }),
            });
        }

        items.sort_by(|a, b| a.source_identity.cmp(&b.source_identity));
        report.bytes = items.iter().map(|i| i.raw.len()).sum();
        report.units = items.len();
        report.items = items;
        Ok(report)
    }

    fn classify_prompt(&self, item: &SourceItem) -> String {
        let path = item.meta.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let (redacted, _) = redact(&item.raw);
        format!(
            "You are classifying one file from a developer's local agent setup so it can be \
             PROPOSED — never committed — as team knowledge. A human reviews everything.\n\n\
             File: {path}\n\n---\n{redacted}\n---\n\n\
             Return ONE JSON object: {{\"source_identity\": \"\", \"destination_kind\": \
             \"memory|convention|skip\", \"content\": \"...\", \"source_excerpt\": \"...\", \
             \"confidence\": 0.0, \"destination_hint\": {{\"title\": \"...\", \"type\": \"...\"}}}}\n\n\
             Rules:\n\
             1. PROPOSE, do not decide.\n\
             2. `source_excerpt` MUST be copied verbatim from above.\n\
             3. This file already declares its own type where it has one. Respect it; you are \
             here to title and summarise, not to reclassify.\n\
             4. A personal preference stays personal. Never propose one as a team convention.\n\
             5. If it carries no reusable knowledge, return \"skip\" and say why.",
        )
    }

    fn fallback(&self, item: &SourceItem) -> Option<CandidatePayload> {
        let path = item.meta.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let category = item
            .meta
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("memory");
        let (content, report) = redact(&item.raw);

        match category {
            "harness" => {
                let kind = AssetKind::from_path(path)?;
                // A manifest that would be rejected fails its own candidate here,
                // not at commit time with a human already waiting.
                let (manifest, _) = build_manifest(kind, path, &item.raw).ok()?;
                let slug = path
                    .rsplit('/')
                    .next()
                    .unwrap_or("asset")
                    .trim_end_matches(".md")
                    .trim_end_matches(".sh")
                    .trim_end_matches(".json")
                    .to_string();
                Some(CandidatePayload {
                    source_identity: item.source_identity.clone(),
                    destination_kind: "harness".to_string(),
                    content: content.clone(),
                    destination_hint: serde_json::json!({
                        "slug": slug,
                        "name": slug,
                        "manifest": manifest,
                        "redaction": report.summary(),
                    }),
                    source_excerpt: Some(first_lines(&content)),
                    confidence: None,
                    provenance_kind: Some("verified_manifest".to_string()),
                })
            }
            "config" => Some(CandidatePayload {
                source_identity: item.source_identity.clone(),
                destination_kind: "harness_config_review".to_string(),
                content: content.clone(),
                destination_hint: serde_json::json!({
                    "source_tool": "claude",
                    "redacted_config": serde_json::from_str::<serde_json::Value>(&content)
                        .unwrap_or(serde_json::json!({ "raw": content })),
                    "redaction_report": {
                        "home_paths": report.home_paths,
                        "tokens": report.tokens,
                        "connection_strings": report.connection_strings,
                        "emails": report.emails,
                        "summary": report.summary(),
                    },
                    "content_hash": sha256_of(&content),
                }),
                source_excerpt: Some(first_lines(&content)),
                confidence: None,
                provenance_kind: Some("verified_manifest".to_string()),
            }),
            "instruction" => Some(CandidatePayload {
                source_identity: item.source_identity.clone(),
                destination_kind: "convention".to_string(),
                content: content.clone(),
                destination_hint: serde_json::json!({
                    "title": path.rsplit('/').next().unwrap_or("Agent instructions"),
                    "category": "agent",
                    "redaction": report.summary(),
                }),
                source_excerpt: Some(first_lines(&content)),
                confidence: None,
                provenance_kind: Some("verified_manifest".to_string()),
            }),
            _ => {
                let (fm, body) = parse_frontmatter(&item.raw);
                let (body, body_report) = redact(&body);
                let (mem_type, scope) = destination_for_type(fm.memory_type.as_deref());
                let mut hint = serde_json::json!({
                    "title": fm.name.clone().unwrap_or_else(|| {
                        path.rsplit('/').next().unwrap_or("memory").trim_end_matches(".md").to_string()
                    }),
                    "type": mem_type,
                    "redaction": body_report.summary(),
                });
                if let Some(scope) = scope {
                    hint["scope"] = serde_json::json!(scope);
                }
                let links = wikilinks(&body);
                if !links.is_empty() {
                    hint["links"] = serde_json::json!(links);
                }
                Some(CandidatePayload {
                    source_identity: item.source_identity.clone(),
                    destination_kind: "memory".to_string(),
                    content: body.clone(),
                    destination_hint: hint,
                    source_excerpt: Some(first_lines(&body)),
                    confidence: None,
                    provenance_kind: Some("verified_manifest".to_string()),
                })
            }
        }
    }
}

fn first_lines(content: &str) -> String {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    /// Pruning must never be the *only* thing stopping a third-party asset:
    /// every directory pruned for speed that carries a licensing meaning is
    /// still refused by `is_never_scanned` on the full path.
    #[test]
    fn pruning_does_not_replace_the_never_scanned_filter() {
        for path in [
            "plugins/cache/nexusmind/skills/sdd/SKILL.md",
            "plugins/marketplaces/acme/agents/x.md",
            "web/node_modules/pkg/CLAUDE.md",
        ] {
            assert!(
                super::is_never_scanned(path),
                "{path} must be refused on its own merits, not merely unvisited"
            );
        }
    }

    use super::*;
    use crate::models::types::validate_typed_harness_manifest;
    use std::fs;
    use tempfile::TempDir;

    fn connector() -> ClaudeMemoriesConnector {
        ClaudeMemoriesConnector::new("nexu-loop-agents")
    }

    fn opts(dir: &TempDir) -> ScanOptions {
        ScanOptions {
            root: dir.path().to_string_lossy().to_string(),
            includes: vec![],
            excludes: vec![],
            ..Default::default()
}
    }

    // ── Frontmatter and typing ───────────────────────────────────────────────

    #[test]
    fn frontmatter_type_drives_the_destination() {
        for (declared, expected) in [
            ("feedback", "feedback"),
            ("project", "project"),
            ("reference", "discovery"),
            ("architecture", "architecture"),
        ] {
            let (kind, scope) = destination_for_type(Some(declared));
            assert_eq!(kind, expected, "type {declared}");
            assert_eq!(scope, None);
        }
    }

    /// One person's habit must not become twelve people's rule.
    #[test]
    fn user_type_stays_personal_and_never_becomes_a_convention() {
        let (kind, scope) = destination_for_type(Some("user"));
        assert_eq!(kind, "preference");
        assert_eq!(scope, Some("personal"));
        assert_ne!(kind, "convention");
    }

    #[test]
    fn a_memory_without_frontmatter_still_scans() {
        let (fm, body) = parse_frontmatter("Just a note about the deploy.\n");
        assert_eq!(fm, MemoryFrontmatter::default());
        assert!(body.contains("deploy"));
        assert_eq!(destination_for_type(None).0, "discovery");
    }

    #[test]
    fn frontmatter_is_parsed_and_stripped_from_the_body() {
        let input = "---\nname: deploy-gotcha\ndescription: what broke\ntype: bugfix\n---\n\nThe pod was not restarted.\n";
        let (fm, body) = parse_frontmatter(input);
        assert_eq!(fm.name.as_deref(), Some("deploy-gotcha"));
        assert_eq!(fm.memory_type.as_deref(), Some("bugfix"));
        assert!(body.contains("pod was not restarted"));
        assert!(!body.contains("description:"), "frontmatter must not leak into the body");
    }

    #[test]
    fn wikilinks_are_extracted_in_order_without_duplicates() {
        let links = wikilinks("see [[alpha]] and [[beta]], and [[alpha]] again");
        assert_eq!(links, vec!["alpha", "beta"]);
    }

    // ── Harness manifests ────────────────────────────────────────────────────

    #[test]
    fn each_asset_kind_maps_to_its_own_harness_format() {
        for (path, expected) in [
            (".claude/agents/reviewer.md", "agent"),
            (".claude/commands/deploy.md", "command"),
            (".claude/hooks/pre-commit.sh", "hook"),
            (".claude/output-styles/direct.md", "output_style"),
            (".claude/themes/dark.json", "theme"),
            (".claude/skills/qa/SKILL.md", "skill"),
        ] {
            let kind = AssetKind::from_path(path).unwrap_or_else(|| panic!("{path} unmapped"));
            assert_eq!(kind.format(), expected, "{path}");
        }
        assert!(AssetKind::from_path("docs/README.md").is_none());
    }

    #[test]
    fn hook_manifests_are_marked_executable() {
        let (m, _) = build_manifest(
            AssetKind::Hook,
            "hooks/pre-commit.sh",
            "#!/bin/sh\nexit 0\n",
        )
        .unwrap();
        assert_eq!(m["security"]["executable"], serde_json::json!(true));
        assert_eq!(m["security"]["requires_approval"], serde_json::json!(true));
    }

    /// The test that decides whether this connector is useful at all: without it
    /// the candidates would only fail at commit time, with a human waiting.
    #[test]
    fn every_emitted_manifest_passes_the_real_validator() {
        let cases = [
            (AssetKind::Agent, "agents/reviewer.md", "# Reviewer\n\nReview carefully.\n"),
            (AssetKind::Command, "commands/deploy.md", "# Deploy\n\nRun the pipeline.\n"),
            (AssetKind::Hook, "hooks/pre-commit.sh", "#!/bin/sh\nexit 0\n"),
            (AssetKind::OutputStyle, "output-styles/direct.md", "# Direct\n\nBe direct.\n"),
            (AssetKind::Theme, "themes/dark.json", "{\"name\":\"Dark\"}"),
            (AssetKind::Skill, "skills/qa/SKILL.md", "---\nname: qa\n---\n\nRun QA.\n"),
        ];
        for (kind, path, content) in cases {
            let (manifest, _) = build_manifest(kind, path, content)
                .unwrap_or_else(|e| panic!("{path} failed to build: {e}"));
            validate_typed_harness_manifest(&manifest)
                .unwrap_or_else(|e| panic!("{path} produced a manifest the validator rejects: {e}"));
        }
    }

    /// Hashes are computed from the redacted text because that is what travels.
    /// Hashing the original would mismatch — and would misdescribe the component.
    #[test]
    fn manifest_hashes_are_computed_from_the_redacted_content() {
        let raw = "# Agent\n\nRuns from /Users/cesar/.claude and uses ghp_abcdefghijklmnopqrst.\n";
        let (manifest, report) = build_manifest(AssetKind::Agent, "agents/a.md", raw).unwrap();
        let component = &manifest["components"][0];
        let content = component["content"].as_str().unwrap();

        assert!(report.home_paths >= 1 && report.tokens >= 1);
        assert!(!content.to_lowercase().contains("/users/"));
        assert!(!content.contains("ghp_"));
        assert_eq!(component["size_bytes"].as_u64().unwrap() as usize, content.len());
        assert_eq!(component["sha256"].as_str().unwrap(), sha256_of(content));
        validate_typed_harness_manifest(&manifest).unwrap();
    }

    #[test]
    fn manifest_paths_are_relative_and_never_contain_a_home_directory() {
        assert!(build_manifest(AssetKind::Agent, "/Users/me/agents/a.md", "# A\n").is_err());
        assert!(build_manifest(AssetKind::Agent, "../escape/a.md", "# A\n").is_err());
        let (m, _) = build_manifest(AssetKind::Agent, "agents/a.md", "# A\n").unwrap();
        assert_eq!(m["components"][0]["path"], serde_json::json!("agents/a.md"));
    }

    /// Half a skill does not install. Oversized assets are skipped with a reason.
    #[test]
    fn oversized_assets_are_skipped_with_a_reason() {
        let huge = "x".repeat(MAX_COMPONENT_BYTES + 1);
        let err = build_manifest(AssetKind::Agent, "agents/a.md", &huge).unwrap_err();
        assert!(err.to_string().contains("asset_too_large"), "{err}");
    }

    // ── Exclusions ───────────────────────────────────────────────────────────

    #[test]
    fn plugin_cache_assets_are_excluded() {
        for path in [
            ".claude/plugins/cache/nexusmind/skills/memory/SKILL.md",
            ".claude/plugins/marketplaces/foo/agents/x.md",
        ] {
            assert!(is_never_scanned(path), "{path} must never be scanned");
        }
        assert!(!is_never_scanned(".claude/skills/mine/SKILL.md"));
    }

    /// A flag that lets you republish somebody else's work is a flag somebody
    /// uses without noticing.
    #[test]
    fn the_cache_exclusion_cannot_be_overridden() {
        let dir = TempDir::new().unwrap();
        let cached = dir.path().join(".claude/plugins/cache/vendor/agents");
        fs::create_dir_all(&cached).unwrap();
        fs::write(cached.join("theirs.md"), "# Somebody else's agent\n").unwrap();
        fs::create_dir_all(dir.path().join(".claude/agents")).unwrap();
        fs::write(dir.path().join(".claude/agents/mine.md"), "# My agent\n").unwrap();

        // Options that would widen the scan as far as they can.
        let wide = ScanOptions {
            root: dir.path().to_string_lossy().to_string(),
            includes: vec!["plugins".to_string(), "cache".to_string()],
            excludes: vec![],
            ..Default::default()
};
        let items = connector().scan(&wide).unwrap();
        let paths: Vec<&str> = items
            .iter()
            .map(|i| i.meta["path"].as_str().unwrap_or(""))
            .collect();

        // Aimed straight at the cache, and it yields nothing at all. `mine.md`
        // is absent too, and correctly so: `--include` narrows. This assertion
        // used to expect it, which only ever passed because the include list
        // was ignored entirely.
        assert!(
            !paths.iter().any(|p| p.contains("cache")),
            "cached third-party assets must stay out regardless of options: {paths:?}"
        );
        assert!(
            paths.is_empty(),
            "an include pointing only at the cache can reach nothing: {paths:?}"
        );

        // And without the include, the operator's own agent is still found —
        // so the exclusion is about the cache, not about the scan being inert.
        let plain = ScanOptions {
            root: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        let paths: Vec<String> = connector()
            .scan(&plain)
            .unwrap()
            .iter()
            .map(|i| i.meta["path"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(paths.iter().any(|p| p.contains("agents/mine.md")), "{paths:?}");
        assert!(!paths.iter().any(|p| p.contains("cache")), "{paths:?}");
    }

    /// `--include` was accepted and ignored, so a request for one directory
    /// classified everything — and was billed for it.
    #[test]
    fn include_narrows_the_scan() {
        let dir = TempDir::new().unwrap();
        for rel in [".claude/agents/mine.md", ".claude/skills/other/SKILL.md"] {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "# something\n").unwrap();
        }

        let narrowed = ScanOptions {
            root: dir.path().to_string_lossy().to_string(),
            includes: vec![".claude/agents".to_string()],
            ..Default::default()
        };
        let paths: Vec<String> = connector()
            .scan(&narrowed)
            .unwrap()
            .iter()
            .map(|i| i.meta["path"].as_str().unwrap_or("").to_string())
            .collect();
        assert_eq!(paths.len(), 1, "only the requested directory: {paths:?}");
        assert!(paths[0].contains("agents/mine.md"));
    }

    #[test]
    fn exclude_removes_a_path_and_says_so() {
        let dir = TempDir::new().unwrap();
        for rel in [".claude/agents/mine.md", ".claude/skills/other/SKILL.md"] {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "# something\n").unwrap();
        }
        let opts = ScanOptions {
            root: dir.path().to_string_lossy().to_string(),
            excludes: vec!["skills".to_string()],
            ..Default::default()
        };
        let report = connector().scan_report(&opts).unwrap();
        assert_eq!(report.units, 1);
        assert!(
            report.excluded.iter().any(|(p, r)| p.contains("SKILL.md")
                && r.contains("--exclude")),
            "an excluded path must be reported, not vanish: {:?}",
            report.excluded
        );
    }

    #[test]
    fn transcripts_are_never_scanned() {
        assert!(is_never_scanned("projects/x/session-abc.jsonl"));
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("session.jsonl"), "{\"a\":1}\n").unwrap();
        assert!(connector().scan(&opts(&dir)).unwrap().is_empty());
    }

    #[test]
    fn the_memory_index_is_not_a_source() {
        assert!(is_memory_index("memory/MEMORY.md"));
        assert!(!is_memory_index("memory/a-real-memory.md"));
    }

    // ── End to end over a realistic tree ─────────────────────────────────────

    fn realistic_tree() -> TempDir {
        let dir = TempDir::new().unwrap();
        let c = dir.path().join(".claude");
        for sub in ["agents", "commands", "hooks", "output-styles", "skills/qa", "projects/proj/memory", "plugins/cache/vendor/agents"] {
            fs::create_dir_all(c.join(sub)).unwrap();
        }
        fs::write(c.join("agents/reviewer.md"), "# Reviewer\n\nReview carefully.\n").unwrap();
        fs::write(c.join("commands/deploy.md"), "# Deploy\n\nRun it.\n").unwrap();
        fs::write(c.join("hooks/pre-commit.sh"), "#!/bin/sh\nexit 0\n").unwrap();
        fs::write(c.join("output-styles/direct.md"), "# Direct\n\nBe direct.\n").unwrap();
        fs::write(c.join("skills/qa/SKILL.md"), "---\nname: qa\n---\n\nRun QA.\n").unwrap();
        fs::write(c.join("plugins/cache/vendor/agents/theirs.md"), "# Theirs\n").unwrap();
        fs::write(
            c.join("projects/proj/memory/MEMORY.md"),
            "- [a](a.md) — index line\n",
        )
        .unwrap();
        fs::write(
            c.join("projects/proj/memory/gotcha.md"),
            "---\nname: deploy-gotcha\ntype: user\n---\n\nI prefer short commits. See [[other]].\n",
        )
        .unwrap();
        fs::write(
            c.join("settings.json"),
            "{\"env\":{\"ANTHROPIC_API_KEY\":\"sk-proj0123456789abcdefghij\"}}",
        )
        .unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "# Rules\n\nAlways write the test first.\n").unwrap();
        dir
    }

    #[test]
    fn a_realistic_tree_maps_every_file_to_the_right_destination() {
        let dir = realistic_tree();
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();

        let mut by_dest: std::collections::HashMap<String, Vec<String>> = Default::default();
        for item in &items {
            let cand = c.fallback(item).expect("every scanned item yields a candidate");
            by_dest
                .entry(cand.destination_kind.clone())
                .or_default()
                .push(item.meta["path"].as_str().unwrap_or("").to_string());
        }

        let harnesses = by_dest.get("harness").cloned().unwrap_or_default();
        assert_eq!(harnesses.len(), 5, "agent, command, hook, output-style, skill: {harnesses:?}");
        assert!(by_dest.get("convention").map(|v| v.len()).unwrap_or(0) >= 1, "CLAUDE.md");
        assert_eq!(by_dest.get("harness_config_review").map(|v| v.len()).unwrap_or(0), 1);
        assert!(by_dest.get("memory").map(|v| v.len()).unwrap_or(0) >= 1);

        // Nothing from the cache, and no index file.
        for paths in by_dest.values() {
            assert!(!paths.iter().any(|p| p.contains("cache")));
            assert!(!paths.iter().any(|p| p.ends_with("MEMORY.md")));
        }
    }

    #[test]
    fn settings_files_propose_a_config_review_not_a_harness() {
        let dir = realistic_tree();
        let c = connector();
        let item = c
            .scan(&opts(&dir))
            .unwrap()
            .into_iter()
            .find(|i| i.meta["path"].as_str().unwrap_or("").ends_with("settings.json"))
            .expect("settings.json must be scanned");

        let cand = c.fallback(&item).unwrap();
        assert_eq!(cand.destination_kind, "harness_config_review");
        assert_ne!(cand.destination_kind, "harness");

        let hint = &cand.destination_hint;
        assert!(hint.get("redaction_report").is_some(), "the report must travel with it");
        assert!(!cand.content.contains("sk-proj0123456789"), "the key must be gone");
        assert!(hint["content_hash"].as_str().unwrap().starts_with("sha256:"));
    }

    #[test]
    fn a_personal_memory_keeps_its_scope_and_its_links() {
        let dir = realistic_tree();
        let c = connector();
        let item = c
            .scan(&opts(&dir))
            .unwrap()
            .into_iter()
            .find(|i| i.meta["path"].as_str().unwrap_or("").ends_with("gotcha.md"))
            .unwrap();

        let cand = c.fallback(&item).unwrap();
        assert_eq!(cand.destination_kind, "memory");
        assert_eq!(cand.destination_hint["type"], serde_json::json!("preference"));
        assert_eq!(cand.destination_hint["scope"], serde_json::json!("personal"));
        assert_eq!(cand.destination_hint["links"], serde_json::json!(["other"]));
        assert_eq!(cand.destination_hint["title"], serde_json::json!("deploy-gotcha"));
    }

    #[test]
    fn agent_instruction_files_propose_conventions() {
        let dir = realistic_tree();
        let c = connector();
        let item = c
            .scan(&opts(&dir))
            .unwrap()
            .into_iter()
            .find(|i| i.meta["path"].as_str().unwrap_or("") == "CLAUDE.md")
            .unwrap();
        assert_eq!(c.fallback(&item).unwrap().destination_kind, "convention");
    }

    #[test]
    fn identities_are_stable_and_carry_no_user_name() {
        let dir = realistic_tree();
        let c = connector();
        let first: Vec<String> = c.scan(&opts(&dir)).unwrap().into_iter().map(|i| i.source_identity).collect();
        let second: Vec<String> = c.scan(&opts(&dir)).unwrap().into_iter().map(|i| i.source_identity).collect();
        assert_eq!(first, second);

        let root = dir.path().to_string_lossy().to_string();
        for id in &first {
            assert!(!id.contains(&root), "identity leaks a path: {id}");
            assert!(!id.to_lowercase().contains("/users/"));
        }
    }

    #[test]
    fn the_prompt_ships_redacted_content_only() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("note.md"),
            "Runs from /Users/cesar/x with ghp_abcdefghijklmnopqrst\n",
        )
        .unwrap();
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        let prompt = c.classify_prompt(&items[0]);
        assert!(!prompt.to_lowercase().contains("/users/"), "the prompt must not leak a home path");
        assert!(!prompt.contains("ghp_"));
        assert!(prompt.contains("PROPOSE, do not decide"));
        assert!(prompt.contains("personal preference stays personal"));
    }

    // ── The scan report ──────────────────────────────────────────────────────

    /// `documents` must mean the same thing in every connector: files that
    /// actually produced a unit. Counting files merely read would make this
    /// connector report thousands while yielding hundreds, and the two numbers
    /// would not be comparable with `repo-docs`.
    #[test]
    fn scan_report_counts_documents_consistently_with_units() {
        let dir = realistic_tree();
        let report = connector().scan_report(&opts(&dir)).unwrap();
        assert_eq!(
            report.documents, report.units,
            "one unit per file in this connector, so the two must agree"
        );
        assert_eq!(report.units, report.items.len());
        assert!(report.bytes > 0 && report.estimated_tokens() > 0);
    }

    /// The exclusion count is the interesting half: an operator needs to see
    /// that the third-party assets were left alone rather than wonder.
    #[test]
    fn scan_report_explains_every_exclusion() {
        let dir = realistic_tree();
        fs::write(dir.path().join(".claude/session.jsonl"), "{\"a\":1}\n").unwrap();

        let report = connector().scan_report(&opts(&dir)).unwrap();
        assert!(!report.excluded.is_empty());
        for (path, reason) in &report.excluded {
            assert!(!path.is_empty() && !reason.is_empty(), "{path} has no reason");
        }

        let reasons: Vec<&str> = report.excluded.iter().map(|(_, r)| r.as_str()).collect();
        assert!(
            reasons.iter().any(|r| r.contains("licensing")),
            "the cached third-party agent must be reported as excluded"
        );
        assert!(reasons.iter().any(|r| r.contains("transcripts")));
        assert!(reasons.iter().any(|r| r.contains("index over the other files")));
    }
}
