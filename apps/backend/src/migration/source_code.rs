//! The source-code connector: a repository's own code, read for the conventions
//! and decisions embedded in it.
//!
//! The other connectors read what a team *wrote about* itself — docs, commits,
//! memories. This one reads the code itself, because the most binding
//! conventions a codebase has are usually the ones nobody wrote down: the error
//! type everything returns, the layering everything obeys, the pattern every
//! handler copies. A model reading a file can name those; a human reviews every
//! proposal before it becomes knowledge.
//!
//! # The unit is the file
//!
//! A convention or a decision lives at the scale of a file or module, not a
//! single function, so a file is one unit. A file too large to classify in one
//! pass is split into line windows — the exception, not the rule, so most
//! candidates carry a whole file's worth of context.
//!
//! # It does not index
//!
//! Vectorising the code for semantic search is a *different* action, served by
//! the `/v1/code` subsystem the TUI triggers separately. This connector only
//! produces review candidates; the two share a repository, not a code path.

use anyhow::Result;
use sha2::{Digest, Sha256};

use super::{CandidatePayload, Connector, ScanOptions, ScanReport, SourceItem};
use crate::indexer::{
    chunker::{Chunker, LineWindowChunker},
    walker::{read_file, walk_files},
};

/// A file larger than this is split into line windows rather than classified
/// whole: past roughly this size a single prompt both risks the model's context
/// and buries the one convention worth finding in a wall of code. Chosen in
/// bytes because that is what the token budget is estimated from.
const MAX_UNIT_BYTES: usize = 20_000;

pub struct SourceCodeConnector {
    /// Repository name — the root directory's own name, never an absolute path,
    /// so a source identity never carries the operator's home directory into a
    /// shared corpus.
    pub repo: String,
}

/// What a scan found, including files deliberately left out.
#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    pub files_scanned: usize,
    pub units: usize,
    pub excluded: Vec<(String, String)>,
}

impl SourceCodeConnector {
    pub fn new(repo: impl Into<String>) -> Self {
        Self { repo: repo.into() }
    }

    /// The repository name for a root path — its last component, or `repo` when
    /// the path has none (`.` and `/` both land here).
    pub fn repo_name_for(root: &str) -> String {
        std::path::Path::new(root)
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "repo".to_string())
    }

    /// `source-code:{repo}:{path}#{anchor}:{sha16}`.
    ///
    /// The hash is of the chunk, not the file: editing one function leaves the
    /// rest of a windowed file with the identities they already had, so a rescan
    /// after a small edit proposes one thing rather than the whole file again.
    fn identity(&self, path: &str, anchor: &str, chunk: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(chunk.as_bytes());
        let sha = hex::encode(hasher.finalize());
        format!("source-code:{}:{}#{}:{}", self.repo, path, anchor, &sha[..16])
    }

    /// Whether a repository-relative path survives the operator's include and
    /// exclude filters. Includes narrow the scan to matching subpaths; excludes
    /// drop matching subpaths on top of the walker's own ignores. Substring
    /// matching keeps this predictable next to the routing globs.
    fn passes_filters(rel: &str, opts: &ScanOptions) -> bool {
        if !opts.includes.is_empty() && !opts.includes.iter().any(|inc| rel.contains(inc.as_str())) {
            return false;
        }
        if opts.excludes.iter().any(|exc| rel.contains(exc.as_str())) {
            return false;
        }
        true
    }

    pub fn scan_with_summary(&self, opts: &ScanOptions) -> Result<(Vec<SourceItem>, ScanSummary)> {
        // `walk_files` already returns only real source files: it respects
        // `.gitignore`, prunes `node_modules`/`target`/`.git`, drops lockfiles
        // and minified bundles, and admits only an extension allowlist. So the
        // connector filters for the operator's own include/exclude and nothing
        // more — the heavy exclusion is the walker's job, done once.
        let files = walk_files(&opts.root)?;
        let chunker = LineWindowChunker::default();
        let mut items = Vec::new();
        let mut summary = ScanSummary::default();

        for (seen, file) in files.iter().enumerate() {
            let rel = relative_path(&file.path, &opts.root);
            opts.note(seen + 1, &rel);

            if !Self::passes_filters(&rel, opts) {
                continue;
            }
            let Some((content, sha)) = read_file(&file.path) else {
                continue; // binary or unreadable — the walker admitted it, we skip it
            };
            if content.trim().is_empty() {
                summary.excluded.push((rel, "empty file".to_string()));
                continue;
            }
            summary.files_scanned += 1;

            if content.len() <= MAX_UNIT_BYTES {
                // The common case: one unit per file, whole-file context.
                items.push(SourceItem {
                    source_identity: self.identity(&rel, "file", &content),
                    display_origin: rel.clone(),
                    routing_path: Some(rel.clone()),
                    raw: content,
                    meta: serde_json::json!({
                        "path": rel,
                        "language": file.language,
                    }),
                });
            } else {
                // A large file, split into windows. Each carries its start line
                // so a reviewer can find it, and its own identity so an edit to
                // one part does not re-propose the whole file.
                for chunk in chunker.chunk(&rel, &sha, file.language.as_deref(), &content) {
                    let anchor = format!("L{}", chunk.start_line);
                    let symbol = chunk.symbol.clone().unwrap_or_default();
                    let origin = if symbol.is_empty() {
                        format!("{rel}:{}", chunk.start_line)
                    } else {
                        format!("{rel} › {symbol}")
                    };
                    items.push(SourceItem {
                        source_identity: self.identity(&rel, &anchor, &chunk.content),
                        display_origin: origin,
                        routing_path: Some(rel.clone()),
                        raw: chunk.content,
                        meta: serde_json::json!({
                            "path": rel,
                            "language": file.language,
                            "symbol": symbol,
                            "start_line": chunk.start_line,
                        }),
                    });
                }
            }
        }

        summary.units = items.len();
        Ok((items, summary))
    }
}

/// Path relative to the scan root. An absolute path in a shared corpus carries
/// the operator's home directory to everyone who can read it.
fn relative_path(path: &str, root: &str) -> String {
    let root = root.trim_end_matches('/');
    path.strip_prefix(root)
        .unwrap_or(path)
        .trim_start_matches('/')
        .to_string()
}

impl Connector for SourceCodeConnector {
    fn source_kind(&self) -> &'static str {
        "source-code"
    }

    fn scan(&self, opts: &ScanOptions) -> Result<Vec<SourceItem>> {
        Ok(self.scan_with_summary(opts)?.0)
    }

    fn scan_report(&self, opts: &ScanOptions) -> Result<ScanReport> {
        let (items, summary) = self.scan_with_summary(opts)?;
        Ok(ScanReport {
            documents: summary.files_scanned,
            units: items.len(),
            bytes: items.iter().map(|i| i.raw.len()).sum(),
            excluded: summary.excluded,
            items,
        })
    }

    fn classify_prompt(&self, item: &SourceItem) -> String {
        let path = item.meta.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let language = item
            .meta
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("code");
        format!(
            "You are reading one {language} file from a software team's codebase to extract the \
             durable engineering knowledge embedded in it — the conventions it follows and the \
             technical decisions it embodies — so they can be PROPOSED, never committed, as team \
             knowledge. A human reviews everything you return.\n\n\
             File: {path}\n\n\
             ---\n{content}\n---\n\n\
             Return ONE JSON object and nothing else:\n\
             {{\"source_identity\": \"\", \"destination_kind\": \"memory|convention|skip\", \
             \"content\": \"...\", \"source_excerpt\": \"...\", \"confidence\": 0.0, \
             \"destination_hint\": {{\"title\": \"...\", \"type\": \"...\"}}}}\n\n\
             Rules:\n\
             1. PROPOSE, do not decide. A human accepts or rejects this.\n\
             2. `content` is the KNOWLEDGE, not the code: state the convention or decision in \
             prose an agent could act on (\"handlers return AppError, never panic\"; \"auth is a \
             middleware layer, not per-route\"). Do NOT paste the code into `content`.\n\
             3. `source_excerpt` MUST be a short snippet copied VERBATIM from the file above — the \
             few lines that evidence the claim. Never paraphrase it.\n\
             4. `convention` is a rule the code follows that others must follow too. `memory` is a \
             technical decision, an architectural fact, or a non-obvious discovery — set \
             `destination_hint.type` to one of `decision`, `pattern`, `architecture`, or \
             `discovery`.\n\
             5. If the file carries no reusable knowledge — glue, generated code, a trivial \
             wrapper, a test fixture — return \"skip\" and say why in `content`. Proposing \
             everything turns review into a job nobody does; most files should be skipped.",
            language = language,
            path = path,
            content = item.raw,
        )
    }

    /// No deterministic candidate: the knowledge in a file is exactly what needs
    /// a model to name. Under `--no-llm` a code unit therefore produces nothing
    /// rather than a fabricated convention — which is the honest outcome for the
    /// one mode where no model may read the material.
    fn fallback(&self, _item: &SourceItem) -> Option<CandidatePayload> {
        None
    }
}

#[cfg(test)]
mod tests;
