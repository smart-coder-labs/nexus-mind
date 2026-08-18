//! The consumer side of the runner's NDJSON stream.
//!
//! # Why this type is duplicated instead of imported
//!
//! The producer is `nexusmind::migration::events::RunEvent` in the backend
//! crate. Importing it would mean a path dependency on a crate that pulls in
//! fastembed, ONNX and a dozen tree-sitter grammars — a second multi-gigabyte
//! target directory for a terminal app. NDJSON *is* the contract; a consumer
//! that re-declares the wire shape is the normal cost of that boundary, and the
//! `stream_from_the_real_runner_parses` test in `runner.rs` is what keeps the
//! two honest.
//!
//! # Why parsing is lenient
//!
//! An event this build has never heard of is not an error. The runner may be
//! newer than the TUI — that is the whole reason the events are tagged by name.
//! Unknown lines become `Unknown` and are counted, never fatal.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    Started {
        source: String,
        path: String,
        dry_run: bool,
        no_llm: bool,
        max_tokens: Option<i64>,
    },
    Scanning {
        seen: usize,
        current: String,
    },
    Scanned {
        documents: usize,
        units: usize,
        bytes: usize,
        estimated_tokens: usize,
        excluded: usize,
    },
    ConfigLoaded {
        repository_id: String,
        path: String,
        sha256: String,
        project_count: usize,
    },
    RoutingGroup {
        alias: String,
        project_id: String,
        client_id: Option<String>,
        item_count: usize,
        sample_paths: Vec<String>,
    },
    RoutingIssue {
        kind: String,
        count: usize,
        sample: Option<String>,
    },
    RoutingReady {
        groups: usize,
        mapped_items: usize,
        unmapped_items: usize,
    },
    RunCreated {
        alias: String,
        project_id: String,
        run_id: String,
    },
    Excluded {
        reason: String,
        count: usize,
        sample: String,
    },
    Classifying {
        index: usize,
        total: usize,
        origin: String,
    },
    Agent {
        index: usize,
        total: usize,
        origin: String,
        prompt: String,
        response: String,
        ok: bool,
        error: Option<String>,
        tokens_spent: i64,
        duration_ms: u64,
    },
    Classified {
        index: usize,
        total: usize,
        origin: String,
        destination_kind: String,
        via: String,
        tokens_spent: i64,
    },
    Staged {
        run_id: String,
        staged: usize,
        skipped: usize,
        rejected: usize,
    },
    Finished {
        ok: bool,
        scanned: usize,
        classified: usize,
        fallbacks: usize,
        failed: usize,
        tokens_spent: i64,
        aborted_on_budget: bool,
        error: Option<String>,
    },
}

/// One parsed line.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedLine {
    Event(RunEvent),
    /// A well-formed event this build does not model. Carries its name so the
    /// operator can see that something happened rather than nothing.
    Unknown(String),
    /// Not JSON at all. Almost always a stray log line — shown verbatim,
    /// because hiding it is how a silent failure stays silent.
    Noise(String),
}

pub fn parse_line(line: &str) -> Option<ParsedLine> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    match serde_json::from_str::<RunEvent>(line) {
        Ok(e) => Some(ParsedLine::Event(e)),
        Err(_) => match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => Some(ParsedLine::Unknown(
                v.get("event")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unnamed")
                    .to_string(),
            )),
            Err(_) => Some(ParsedLine::Noise(line.to_string())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_event_parses_into_its_variant() {
        let line = r#"{"event":"scanned","documents":3,"units":69,"bytes":58867,"estimated_tokens":14716,"excluded":0}"#;
        assert_eq!(
            parse_line(line),
            Some(ParsedLine::Event(RunEvent::Scanned {
                documents: 3,
                units: 69,
                bytes: 58867,
                estimated_tokens: 14716,
                excluded: 0,
            }))
        );
    }

    /// A newer runner must not break an older TUI.
    #[test]
    fn a_scan_heartbeat_parses() {
        assert_eq!(
            parse_line(r#"{"event":"scanning","seen":25,"current":"docs/API_SPEC.md"}"#),
            Some(ParsedLine::Event(RunEvent::Scanning {
                seen: 25,
                current: "docs/API_SPEC.md".into(),
            }))
        );
    }

    #[test]
    fn an_agent_exchange_parses() {
        let line = r#"{"event":"agent","index":1,"total":9,"origin":"a.md","prompt":"q",
                       "response":"{}","ok":false,"error":"nope","tokens_spent":12,
                       "duration_ms":900}"#;
        match parse_line(line) {
            Some(ParsedLine::Event(RunEvent::Agent { ok, error, .. })) => {
                assert!(!ok);
                assert_eq!(error.as_deref(), Some("nope"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_unmodelled_event_is_named_not_fatal() {
        assert_eq!(
            parse_line(r#"{"event":"embedded","count":12}"#),
            Some(ParsedLine::Unknown("embedded".into()))
        );
    }

    #[test]
    fn a_stray_log_line_is_surfaced_verbatim() {
        assert_eq!(
            parse_line("  WARN classifier retry 1/3  "),
            Some(ParsedLine::Noise("WARN classifier retry 1/3".into()))
        );
    }

    #[test]
    fn blank_lines_are_dropped() {
        assert_eq!(parse_line("   "), None);
        assert_eq!(parse_line(""), None);
    }
}
