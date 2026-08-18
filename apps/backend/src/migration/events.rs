//! The machine-readable side of a migration run.
//!
//! `migrate-knowledge --json` emits one of these per line on stdout — NDJSON,
//! flushed as it goes — so a supervising process can show real progress instead
//! of waiting for a wall of text at the end.
//!
//! # Why a stream and not a final blob
//!
//! A scan over 3377 sections takes minutes. A tool that prints nothing until it
//! finishes is a tool nobody trusts: the operator cannot tell a slow run from a
//! hung one, and the first thing they do is kill it.
//!
//! # Why NDJSON and not a richer protocol
//!
//! One JSON object per line survives pipes, `tee`, log collectors and a human
//! reading it with `jq`. Anything framed or binary would buy nothing here and
//! cost the ability to debug the stream by looking at it.

use serde::{Deserialize, Serialize};

/// One line of the event stream.
///
/// `#[serde(tag = "event")]` keeps every line self-describing: a consumer that
/// meets an event it does not know can skip it by name rather than guess from
/// shape. New variants are therefore additive, not breaking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    /// Emitted first, always. Tells the consumer what it is watching.
    Started {
        source: String,
        path: String,
        dry_run: bool,
        no_llm: bool,
        max_tokens: Option<i64>,
    },

    /// The scan is still walking. Emitted periodically, not per source: a
    /// repository with ten thousand commits would otherwise produce ten
    /// thousand lines, and the consumer needs a heartbeat, not a census.
    Scanning { seen: usize, current: String },

    /// The scan finished. Everything after this is classification and staging.
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

    /// Deliberately skipped sources, grouped by reason.
    ///
    /// One event per distinct reason rather than per file. A scan of this
    /// repository skips over twenty thousand paths — almost all of them
    /// `node_modules` — and a stream that names each one individually is a
    /// dump, not a progress report. `sample` keeps one concrete path per
    /// reason, which is what an operator actually checks: *which* file was
    /// this, and does the reason make sense for it.
    Excluded {
        reason: String,
        count: usize,
        sample: String,
    },

    /// Progress through classification. `index` is 1-based so `3/69` reads the
    /// way a human counts.
    Classifying {
        index: usize,
        total: usize,
        origin: String,
    },

    /// One exchange with the model: what was asked, and what came back.
    ///
    /// Emitted only in `--json` mode, and both sides are truncated — a prompt
    /// carries the whole file. Enough to see *why* a classification came out
    /// the way it did, which is the difference between "24 fell back" and
    /// knowing the model was answering a different question entirely.
    Agent {
        index: usize,
        total: usize,
        origin: String,
        prompt: String,
        response: String,
        /// False when the answer could not be used, whatever the reason.
        ok: bool,
        /// Present when the answer was unusable.
        error: Option<String>,
        tokens_spent: i64,
        duration_ms: u64,
    },

    /// One unit turned into a candidate — or did not.
    Classified {
        index: usize,
        total: usize,
        origin: String,
        destination_kind: String,
        /// `classified` when the model answered, `fallback` when it did not and
        /// the connector's deterministic path took over, `failed` when neither
        /// produced anything.
        via: String,
        tokens_spent: i64,
    },

    /// The backend accepted (or refused) a batch. Nothing has been committed:
    /// staging is the point where a human takes over.
    Staged {
        run_id: String,
        staged: usize,
        skipped: usize,
        rejected: usize,
    },

    /// Terminal. Exactly one of these ends every run, including a failed one.
    Finished {
        ok: bool,
        scanned: usize,
        classified: usize,
        fallbacks: usize,
        failed: usize,
        tokens_spent: i64,
        aborted_on_budget: bool,
        /// Present only when `ok` is false.
        error: Option<String>,
    },
}

impl RunEvent {
    /// Serialize as one NDJSON line, newline included.
    ///
    /// Infallible on purpose: an event that cannot be serialized would be a bug
    /// in this file, and losing the stream over it would be worse than emitting
    /// a marker the consumer can see.
    pub fn to_line(&self) -> String {
        match serde_json::to_string(self) {
            Ok(json) => format!("{json}\n"),
            Err(e) => format!(
                "{{\"event\":\"finished\",\"ok\":false,\"error\":\"unserializable event: {e}\"}}\n"
            ),
        }
    }
}

/// Caps a side of an exchange before it goes on the wire.
///
/// A classifier prompt embeds an entire file; a stream that carried them whole
/// would be larger than the corpus being migrated. The cut is marked so nobody
/// mistakes a truncation for the model's actual answer.
pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{head}\n… [{} chars truncated]", text.chars().count() - max)
}

/// Writes events to stdout, flushing each line.
///
/// The flush is the whole point: without it the pipe buffers and the supervising
/// process sees nothing until the run ends, which is exactly the behaviour this
/// module exists to avoid.
pub struct EventSink {
    enabled: bool,
}

impl EventSink {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn emit(&self, event: &RunEvent) {
        if !self.enabled {
            return;
        }
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(event.to_line().as_bytes());
        let _ = out.flush();
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_is_self_describing_and_newline_terminated() {
        let e = RunEvent::Scanned {
            documents: 3,
            units: 69,
            bytes: 58867,
            estimated_tokens: 14716,
            excluded: 26,
        };
        let line = e.to_line();
        assert!(line.ends_with('\n'), "NDJSON needs the newline");
        assert_eq!(line.lines().count(), 1, "one event, one line");

        let parsed: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(parsed["event"], "scanned", "the tag names the variant");
        assert_eq!(parsed["units"], 69);
    }

    #[test]
    fn events_round_trip() {
        let cases = vec![
            RunEvent::Scanning {
                seen: 120,
                current: "docs/adr/ADR-014.md".into(),
            },
            RunEvent::Started {
                source: "repo-docs".into(),
                path: "/repo".into(),
                dry_run: true,
                no_llm: false,
                max_tokens: Some(50_000),
            },
            RunEvent::Excluded {
                reason: "not engineering knowledge".into(),
                count: 12,
                sample: "docs/marketing/x.md".into(),
            },
            RunEvent::Classified {
                index: 3,
                total: 69,
                origin: "docs/adr/ADR-001.md".into(),
                destination_kind: "memory".into(),
                via: "fallback".into(),
                tokens_spent: 0,
            },
            RunEvent::Agent {
                index: 3,
                total: 69,
                origin: "docs/adr/ADR-001.md".into(),
                prompt: "Classify this…".into(),
                response: "{\"destination_kind\":\"memory\"}".into(),
                ok: true,
                error: None,
                tokens_spent: 1081,
                duration_ms: 4200,
            },
            RunEvent::Staged {
                run_id: "r1".into(),
                staged: 69,
                skipped: 0,
                rejected: 0,
            },
            RunEvent::Finished {
                ok: true,
                scanned: 69,
                classified: 60,
                fallbacks: 9,
                failed: 0,
                tokens_spent: 14_716,
                aborted_on_budget: false,
                error: None,
            },
        ];
        for case in cases {
            let line = case.to_line();
            let back: RunEvent = serde_json::from_str(line.trim())
                .unwrap_or_else(|e| panic!("{line} did not round-trip: {e}"));
            assert_eq!(back, case);
        }
    }

    /// A consumer must be able to skip an event it does not know by name, which
    /// is what makes new variants additive rather than breaking.
    #[test]
    fn an_unknown_event_is_identifiable_without_parsing_it() {
        let line = r#"{"event":"something_new","field":1}"#;
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["event"], "something_new");
        assert!(serde_json::from_str::<RunEvent>(line).is_err());
    }

    #[test]
    fn clipping_marks_the_cut_and_leaves_short_text_alone() {
        assert_eq!(clip("short", 10), "short");
        let clipped = clip(&"x".repeat(50), 10);
        assert!(clipped.starts_with(&"x".repeat(10)));
        assert!(clipped.contains("40 chars truncated"), "{clipped}");
    }

    /// Truncation must not split a multi-byte character.
    #[test]
    fn clipping_counts_characters_not_bytes() {
        let text = "áéíóúñ¿?".repeat(10);
        let clipped = clip(&text, 5);
        assert!(clipped.starts_with("áéíóú"), "{clipped}");
    }

    /// A disabled sink writes nothing at all: the human-readable mode must stay
    /// human-readable.
    #[test]
    fn a_disabled_sink_is_silent() {
        let sink = EventSink::new(false);
        assert!(!sink.is_enabled());
        sink.emit(&RunEvent::Excluded {
            reason: "y".into(),
            count: 1,
            sample: "x".into(),
        });
    }
}
