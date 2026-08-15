//! Redaction: what must be removed before local material leaves the machine.
//!
//! This is not hygiene, it is a precondition. The harness manifest validator
//! (`models::types::validate_safe_manifest_content`) rejects any content
//! containing `/users/`, `bearer `, `ghp_`, `nm_live`, `raw-secret` or an OpenAI
//! key. A skill that merely *mentions* a home directory fails its manifest, so a
//! connector that does not redact produces no valid harness at all.
//!
//! It also matters for a reason the validator does not know about: local memory
//! files are full of absolute paths, tokens and connection strings precisely
//! because they were never written to be shared.
//!
//! # The report travels with the candidate
//!
//! Redaction happens **before staging**, not before commit: sensitive material
//! must never reach the review queue. And a reviewer has to be able to see that
//! three things were removed and what kind they were — content silently altered
//! is content you cannot trust.

use serde::{Deserialize, Serialize};

/// What a redaction pass removed, by category and count.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionReport {
    pub home_paths: usize,
    pub tokens: usize,
    pub connection_strings: usize,
    pub emails: usize,
}

impl RedactionReport {
    pub fn total(&self) -> usize {
        self.home_paths + self.tokens + self.connection_strings + self.emails
    }
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
    /// A short human-readable summary for the review UI. Empty when nothing was
    /// touched, so the absence of a note means the content is untouched rather
    /// than that nobody looked.
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut parts = Vec::new();
        for (n, label) in [
            (self.home_paths, "home path"),
            (self.tokens, "credential"),
            (self.connection_strings, "connection string"),
            (self.emails, "email address"),
        ] {
            if n > 0 {
                parts.push(format!("{n} {label}{}", if n == 1 { "" } else { "s" }));
            }
        }
        format!("redacted: {}", parts.join(", "))
    }
}

/// Case-insensitive prefix test that allocates nothing.
///
/// The obvious version — `rest.to_lowercase().starts_with(p)` — lowercases the
/// entire remaining text once per character, which is what made the first draft
/// of this module quadratic: 64 KB took 72 seconds. Comparing only the first
/// `needle.len()` bytes is O(needle).
fn starts_with_ci(haystack: &str, needle: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    h.len() >= n.len() && h[..n.len()].eq_ignore_ascii_case(n)
}

/// Replace a user's home directory with `~`.
///
/// `/Users/cesar/Documents/x` → `~/Documents/x`. The username is the thing being
/// removed: it identifies a person, and the validator refuses the literal
/// `/users/` regardless of case.
fn redact_home_paths(input: &str, report: &mut RedactionReport) -> String {
    const PREFIXES: &[&str] = &["/users/", "/home/", "c:\\users\\", "c:/users/"];
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;

    while i < input.len() {
        if !input.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &input[i..];
        if let Some(prefix) = PREFIXES.iter().find(|p| starts_with_ci(rest, p)) {
            let sep = if prefix.contains('\\') { '\\' } else { '/' };
            let tail = &rest[prefix.len()..];
            let user_len = tail.find(sep).unwrap_or(tail.len());
            out.push('~');
            i += prefix.len() + user_len;
            report.home_paths += 1;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Token shapes worth removing wholesale. Each is a prefix followed by an opaque
/// run of token characters.
const TOKEN_PREFIXES: &[&str] = &["ghp_", "github_pat_", "gho_", "nm_live_", "nm_demo_", "sk-"];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn redact_tokens(input: &str, report: &mut RedactionReport) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;

    while i < input.len() {
        if !input.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &input[i..];

        if let Some(prefix) = TOKEN_PREFIXES.iter().find(|p| starts_with_ci(rest, p)) {
            let after = &rest[prefix.len()..];
            let len: usize = after.chars().take_while(|c| is_token_char(*c)).count();
            // `sk-` is a substring of ordinary words ("task-specific", "risk-based"),
            // so only treat it as a key when what follows is long and opaque.
            if *prefix != "sk-" || len >= 16 {
                out.push_str("<redacted:token>");
                i += prefix.len() + len;
                report.tokens += 1;
                continue;
            }
        }

        // `Bearer <something>` — the validator refuses the literal `bearer `.
        if starts_with_ci(rest, "bearer ") {
            let after = &rest[7..];
            let len: usize = after.chars().take_while(|c| is_token_char(*c)).count();
            if len > 0 {
                out.push_str("<redacted:token>");
                i += 7 + len;
                report.tokens += 1;
                continue;
            }
        }

        let ch = rest.chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

const DB_SCHEMES: &[&str] = &[
    "postgres://",
    "postgresql://",
    "mysql://",
    "mongodb://",
    "redis://",
    "amqp://",
];

/// Keep the scheme, drop everything up to the next whitespace or quote: the fact
/// that the team uses Postgres is knowledge; the password is not.
fn redact_connection_strings(input: &str, report: &mut RedactionReport) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;

    while i < input.len() {
        if !input.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &input[i..];
        if let Some(scheme) = DB_SCHEMES.iter().find(|s| starts_with_ci(rest, s)) {
            let after = &rest[scheme.len()..];
            let len: usize = after
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'' && *c != '`')
                .map(|c| c.len_utf8())
                .sum();
            out.push_str(scheme);
            out.push_str("<redacted>");
            i += scheme.len() + len;
            report.connection_strings += 1;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Replace email-shaped spans in place.
///
/// The obvious implementation — `split_whitespace()` then `join(" ")` —
/// **rewrites the whitespace of every document it touches**: a trailing newline
/// disappears, runs of spaces collapse, and the indentation of a shell script or
/// a fenced code block inside a skill is destroyed. The hash then describes
/// mangled text, and the harness that gets installed is not the one that was
/// reviewed.
///
/// So this walks the input and rewrites only the spans that are actually
/// addresses, leaving every other byte exactly where it was.
fn redact_emails(input: &str, report: &mut RedactionReport) -> String {
    fn is_local_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
    }
    fn is_domain_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '-')
    }

    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;

    while let Some(rel_at) = input[cursor..].find('@') {
        let at = cursor + rel_at;

        // Walk left over the local part.
        let mut start = at;
        for (idx, ch) in input[cursor..at].char_indices().rev() {
            if is_local_char(ch) {
                start = cursor + idx;
            } else {
                break;
            }
        }

        // Walk right over the domain.
        let mut end = at + 1;
        for (idx, ch) in input[at + 1..].char_indices() {
            if is_domain_char(ch) {
                end = at + 1 + idx + ch.len_utf8();
            } else {
                break;
            }
        }

        let local = &input[start..at];
        let domain = &input[at + 1..end];
        // A bare `@` (a mention, a decorator) is not an address, and neither is
        // a domain with no dot.
        let is_address = !local.is_empty() && domain.contains('.') && !domain.ends_with('.');

        if is_address {
            out.push_str(&input[cursor..start]);
            out.push_str("<redacted:email>");
            report.emails += 1;
            cursor = end;
        } else {
            out.push_str(&input[cursor..at + 1]);
            cursor = at + 1;
        }
    }
    out.push_str(&input[cursor..]);
    out
}

/// Redact in order: paths, then tokens, then connection strings, then emails.
///
/// Order matters. A connection string contains what looks like an email
/// (`user@host`), so it must be handled before the email pass, or the same
/// secret gets counted twice and half-replaced.
pub fn redact(input: &str) -> (String, RedactionReport) {
    let mut report = RedactionReport::default();
    let out = redact_home_paths(input, &mut report);
    let out = redact_tokens(&out, &mut report);
    let out = redact_connection_strings(&out, &mut report);
    let out = redact_emails(&out, &mut report);
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::types::validate_typed_harness_manifest;

    #[test]
    fn home_paths_become_a_tilde() {
        let (out, r) = redact("see /Users/cesar/Documents/notes.md for details");
        assert_eq!(out, "see ~/Documents/notes.md for details");
        assert_eq!(r.home_paths, 1);
        assert!(!out.to_lowercase().contains("/users/"));
    }

    #[test]
    fn home_paths_are_matched_regardless_of_case() {
        // The validator lowercases before scanning, so `/Users/` and `/users/`
        // are equally fatal and both must be caught.
        for input in [
            "/Users/ana/x",
            "/users/ana/x",
            "/home/ana/x",
            "C:\\Users\\ana\\x",
        ] {
            let (out, r) = redact(input);
            assert_eq!(r.home_paths, 1, "{input} must be redacted");
            assert!(!out.to_lowercase().contains("users"), "{out}");
        }
    }

    #[test]
    fn credentials_are_removed() {
        let (out, r) = redact("token ghp_abcdefghijklmnopqrstuvwxyz0123 and nm_live_secretvalue1");
        assert!(!out.contains("ghp_"));
        assert!(!out.contains("nm_live"));
        assert_eq!(r.tokens, 2);
    }

    /// `sk-` is a substring of ordinary English. Redacting it blindly would
    /// mangle "task-specific" and "risk-based" — the exact false positive the
    /// manifest scanner's own comment warns about.
    #[test]
    fn ordinary_words_containing_sk_survive() {
        let (out, r) = redact("this is task-specific and risk-based, not disk-backed");
        assert_eq!(out, "this is task-specific and risk-based, not disk-backed");
        assert_eq!(r.tokens, 0);

        let (out, r) = redact("key sk-proj0123456789abcdefghijkl");
        assert!(!out.contains("sk-proj"));
        assert_eq!(r.tokens, 1);
    }

    #[test]
    fn bearer_headers_are_removed() {
        let (out, r) = redact("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6");
        assert!(!out.to_lowercase().contains("bearer e"));
        assert_eq!(r.tokens, 1);
    }

    /// The scheme is knowledge; the password is not.
    #[test]
    fn connection_strings_keep_their_scheme_and_lose_the_rest() {
        let (out, r) = redact("DB is postgres://admin:hunter2@db.internal:5432/prod today");
        assert!(out.contains("postgres://<redacted>"));
        assert!(!out.contains("hunter2"));
        assert!(!out.contains("db.internal"));
        assert_eq!(r.connection_strings, 1);
    }

    #[test]
    fn emails_are_removed() {
        let (out, r) = redact("ping cesar@example.com about it");
        assert!(!out.contains("cesar@example.com"));
        assert_eq!(r.emails, 1);
    }

    /// A connection string contains something email-shaped. Redacting paths and
    /// connection strings first is what stops the same secret being counted
    /// twice and half-replaced.
    #[test]
    fn a_connection_string_is_not_also_counted_as_an_email() {
        let (_, r) = redact("postgres://user:pw@host.internal/db");
        assert_eq!(r.connection_strings, 1);
        assert_eq!(r.emails, 0, "the userinfo is part of the DSN, not an address");
    }

    #[test]
    fn clean_text_is_untouched_and_reports_nothing() {
        let input = "The team always writes the failing test first.";
        let (out, r) = redact(input);
        assert_eq!(out, input);
        assert!(r.is_empty());
        assert_eq!(r.summary(), "", "silence means untouched, not unexamined");
    }

    #[test]
    fn the_summary_is_readable() {
        let (_, r) = redact("/Users/a/x and ghp_abcdefghijklmnopqrstuvwx and b@c.com");
        assert_eq!(r.summary(), "redacted: 1 home path, 1 credential, 1 email address");
    }

    /// The point of the whole module: content that would fail the manifest
    /// validator must pass after redaction.
    #[test]
    fn redacted_content_survives_the_real_manifest_validator() {
        let dirty = "# Reviewer\n\nRuns from /Users/cesar/.claude and uses ghp_abcdefghijklmnopqrst.\n";
        let (clean, report) = redact(dirty);
        assert_eq!(report.home_paths, 1);
        assert_eq!(report.tokens, 1);

        let manifest = serde_json::json!({
            "schema_version": "1.1",
            "format": "agent",
            "targets": ["claude"],
            "components": [{
                "kind": "file",
                "path": "agents/reviewer.md",
                "media_type": "text/markdown",
                "size_bytes": clean.len(),
                "sha256": format!("sha256:{}", hex::encode(<sha2::Sha256 as sha2::Digest>::digest(clean.as_bytes()))),
                "content": clean,
            }],
            "provenance": { "source": "migration" },
            "security": { "requires_approval": true, "secret_scan_status": "passed" }
        });

        validate_typed_harness_manifest(&manifest)
            .expect("redacted content must satisfy the validator that rejected the original");

        // And the original really would have been rejected.
        let mut dirty_manifest = manifest.clone();
        dirty_manifest["components"][0]["content"] = serde_json::json!(dirty);
        assert!(
            validate_typed_harness_manifest(&dirty_manifest).is_err(),
            "the unredacted original must be refused — otherwise this module is pointless"
        );
    }

    /// Pins the complexity, not the wall clock.
    ///
    /// The first draft rebuilt the remaining text as a fresh `String` and
    /// lowercased it once per character. That is O(n²): 64 KB took 72 seconds,
    /// and a real `~/.claude/` scan would have been unusable. One megabyte here
    /// takes milliseconds; a quadratic implementation would take hours, so the
    /// generous bound catches the regression without flaking on a slow runner.
    #[test]
    fn redaction_is_linear_not_quadratic() {
        let big = "The team always writes the failing test first. ".repeat(22_000);
        assert!(big.len() > 1_000_000);

        let started = std::time::Instant::now();
        let (out, report) = redact(&big);
        let elapsed = started.elapsed();

        assert_eq!(out.len(), big.len(), "clean text is returned unchanged");
        assert!(report.is_empty());
        assert!(
            elapsed.as_secs() < 5,
            "1 MB took {elapsed:?} — that is quadratic behaviour returning"
        );
    }

    /// The bug the linearity test actually found: `split_whitespace().join(" ")`
    /// rewrote whitespace everywhere. A hook script losing its indentation is a
    /// hook script that no longer runs, and its hash would describe the mangled
    /// version rather than the reviewed one.
    #[test]
    fn redaction_preserves_every_byte_it_does_not_replace() {
        for input in [
            "#!/bin/sh\nif true; then\n    echo indented\nfi\n",
            "line one\n\n\nline four with   three   spaces\n",
            "trailing space at the end ",
            "```python\ndef f():\n    return 1\n```\n",
        ] {
            let (out, report) = redact(input);
            assert_eq!(out, input, "clean content must survive byte for byte");
            assert!(report.is_empty());
        }
    }

    #[test]
    fn an_email_is_replaced_without_disturbing_its_surroundings() {
        let (out, r) = redact("  contact:  ana.lopez+tag@example.co.uk  now\n");
        assert_eq!(out, "  contact:  <redacted:email>  now\n");
        assert_eq!(r.emails, 1);
    }

    /// A bare `@` is a mention or a decorator, not an address.
    #[test]
    fn bare_at_signs_are_not_addresses() {
        for input in ["cc @cesar please", "@decorator\ndef f(): pass", "a @ b"] {
            let (out, r) = redact(input);
            assert_eq!(out, input, "{input}");
            assert_eq!(r.emails, 0);
        }
    }
}
