//! Pure helpers for the `security_dast` autonomous-agent template (Phase 2: active DAST).
//!
//! No I/O and no process spawning here. The worker calls these to: validate the
//! authorized target URL/host from a `web_application` target, build the nuclei argv
//! with fixed flags (target injected ONLY from the authorized target), enforce the
//! per-finding host scope guard, and map nuclei JSONL output into canonical findings
//! with request/response evidence.
//!
//! Active DAST sends real attack traffic, so the security-critical invariants live
//! here where they are exhaustively unit-testable:
//!   1. the scan URL comes only from an authorized target, never free-form run input;
//!   2. only `http`/`https` with a real host, no embedded credentials;
//!   3. findings whose host is not the authorized host are dropped (scope guard).
//!
//! Contract: `openspec/changes/security-dast-template/contracts.md`.

use anyhow::{bail, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const NUCLEI: &str = "nuclei";

/// Only these programs may be spawned by the DAST runner. Separate from every other
/// runner's allowlist so widening one never widens another.
pub const DAST_PROGRAM_ALLOWLIST: [&str; 1] = [NUCLEI];

/// nuclei severity filter values accepted from config.
pub const NUCLEI_SEVERITIES: [&str; 5] = ["info", "low", "medium", "high", "critical"];

pub const MAX_DAST_INVOCATIONS: usize = 8;

/// Nuclei templates baked into the runtime image at build time (see
/// apps/backend/Dockerfile). Passed via `-t` with update checks disabled so runs
/// never download templates or phone home at run time.
pub const NUCLEI_TEMPLATES_DIR: &str = "/opt/nuclei-home/nuclei-templates";

/// High-signal template tags run by default. The full ~13.6k-template set is far too
/// slow to even load per target (a run appears to hang for many minutes); this curated
/// set (CVEs, misconfigurations, exposures, default logins, exposed panels) filtered by
/// severity loads and runs in a couple of minutes while keeping the actionable coverage.
pub const NUCLEI_TAGS: &str = "cve,misconfig,exposure,default-login,exposed-panel";

/// Nuclei template concurrency — the full set is otherwise slow against one target.
pub const NUCLEI_CONCURRENCY: u32 = 50;

const US: char = '\u{1f}';

pub fn is_allowlisted_program(program: &str) -> bool {
    DAST_PROGRAM_ALLOWLIST.contains(&program)
}

// ── Authorized target URL / host ─────────────────────────────────────────────

/// Extract and validate the scan URL from a `web_application` target's `config`.
/// Returns `(authorized_host_lowercased, url)`. The URL is the ONLY place the scan
/// target may come from — never free-form run input.
pub fn authorized_target_url(target_config: &Value) -> Result<(String, String)> {
    let raw = target_config
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("target_url_missing"))?;
    parse_http_url(raw)
}

/// Parse an http(s) URL into `(host_lowercased, trimmed_url)`. Rejects any other
/// scheme, embedded credentials (`user:pass@`), and a missing/invalid host.
pub fn parse_http_url(raw: &str) -> Result<(String, String)> {
    let raw = raw.trim();
    // Scheme match is case-insensitive (HTTPS://), but the URL keeps its original
    // case for the scanner; only the extracted host is lowercased.
    let lower = raw.to_ascii_lowercase();
    let scheme_len = if lower.starts_with("https://") {
        "https://".len()
    } else if lower.starts_with("http://") {
        "http://".len()
    } else {
        bail!("target_url_scheme_invalid")
    };
    let rest = &raw[scheme_len..];
    // Authority is everything up to the first path/query/fragment delimiter.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        bail!("target_url_host_missing")
    }
    // No embedded credentials — they must never ride inside the scanned URL.
    if authority.contains('@') {
        bail!("target_url_userinfo_forbidden")
    }
    let host = authority.split(':').next().unwrap_or("");
    if host.is_empty() {
        bail!("target_url_host_missing")
    }
    // Conservative host charset (hostnames and IPv4). IPv6 literals are not
    // supported in v1 and are rejected rather than mis-parsed.
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    {
        bail!("target_url_host_invalid")
    }
    Ok((host.to_ascii_lowercase(), raw.to_string()))
}

/// Host of an arbitrary URL, lowercased, for scope comparisons. None if unparseable.
pub fn host_of(url: &str) -> Option<String> {
    parse_http_url(url).ok().map(|(host, _)| host)
}

/// A candidate URL is in scope iff its host equals the authorized host. Anything the
/// scanner surfaces on a different host (e.g. via a redirect) is dropped.
pub fn in_scope(candidate_url: &str, authorized_host: &str) -> bool {
    host_of(candidate_url).is_some_and(|h| h == authorized_host.to_ascii_lowercase())
}

// ── nuclei argv (fixed flags; only validated slots injected) ─────────────────

pub fn validate_severity_filter(severity: &str) -> Result<()> {
    // Comma-separated subset of the known severities.
    for part in severity.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if !NUCLEI_SEVERITIES.contains(&part) {
            bail!("invalid_severity_filter")
        }
    }
    Ok(())
}

/// Build the nuclei argv. `-jsonl` streams results to stdout. `-no-interactsh`
/// disables the out-of-band callback server (no external infra), `-disable-update-check`
/// stops phone-home. The target URL is the caller's authorized URL and is the only
/// injected value besides the validated severity/rate/timeout numbers.
pub fn build_nuclei_argv(
    target_url: &str,
    severity: &str,
    rate_limit: u32,
    timeout_secs: u32,
) -> Result<Vec<String>> {
    // The URL must re-validate here so a bad value can never reach argv.
    parse_http_url(target_url)?;
    validate_severity_filter(severity)?;
    if rate_limit == 0 || timeout_secs == 0 {
        bail!("invalid_rate_or_timeout")
    }
    Ok(vec![
        NUCLEI.into(),
        "-u".into(),
        target_url.into(),
        "-jsonl".into(),
        "-silent".into(),
        "-no-interactsh".into(),
        "-disable-update-check".into(),
        // Traffic-level scope guard: never follow a redirect, so an attacker-influenced
        // 3xx from the target cannot make nuclei send probes to another host (blind SSRF
        // to internal endpoints). This makes the host guarantee hold at the request layer,
        // not just when recording findings.
        "-disable-redirects".into(),
        "-rate-limit".into(),
        rate_limit.to_string(),
        "-timeout".into(),
        timeout_secs.to_string(),
        "-severity".into(),
        severity.into(),
        // Run the templates baked into the image; no runtime download.
        "-t".into(),
        NUCLEI_TEMPLATES_DIR.into(),
        // Scope to a fast, high-signal tag set (the full template set is too slow to
        // load per target) and raise template concurrency.
        "-tags".into(),
        NUCLEI_TAGS.into(),
        "-c".into(),
        NUCLEI_CONCURRENCY.to_string(),
    ])
}

// ── Severity + fingerprint ───────────────────────────────────────────────────

pub fn normalize_severity(native: &str) -> &'static str {
    match native.trim().to_ascii_lowercase().as_str() {
        "critical" => "critical",
        "high" => "high",
        "medium" => "medium",
        "low" => "low",
        "info" | "informational" | "unknown" | "" => "info",
        _ => "medium",
    }
}

fn sha256_hex(parts: &[&str]) -> String {
    hex::encode(Sha256::digest(parts.join(&US.to_string()).as_bytes()))
}

pub fn fingerprint_dast(template_id: &str, matched_at: &str) -> String {
    sha256_hex(&["dast", template_id, matched_at])
}

// ── nuclei JSONL → canonical findings ────────────────────────────────────────

fn str_at<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or("")
}

fn join_str_array(v: &Value, ptr: &str) -> Vec<String> {
    v.pointer(ptr)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Map nuclei `-jsonl` output (one JSON object per line) to canonical findings.
/// Findings whose `matched-at`/`host` is outside the authorized host are DROPPED
/// (scope guard). Malformed lines are skipped; the function never panics.
pub fn map_nuclei_jsonl(bytes: &[u8], authorized_host: &str) -> Vec<Value> {
    let text = String::from_utf8_lossy(bytes);
    let mut findings = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let template_id = str_at(&v, "/template-id");
        if template_id.is_empty() {
            continue;
        }
        let matched_at = {
            let m = str_at(&v, "/matched-at");
            if m.is_empty() {
                str_at(&v, "/host")
            } else {
                m
            }
        };
        // Scope guard: only keep findings on the authorized host.
        let host = str_at(&v, "/host");
        let scope_ref = if !matched_at.is_empty() {
            matched_at
        } else {
            host
        };
        if !in_scope(scope_ref, authorized_host) {
            continue;
        }
        let name = str_at(&v, "/info/name");
        let severity = normalize_severity(str_at(&v, "/info/severity"));
        let title = if name.is_empty() {
            template_id.to_string()
        } else {
            name.to_string()
        };
        findings.push(json!({
            "title": title,
            "severity": severity,
            "summary": format!("{template_id} matched at {matched_at}"),
            "fingerprint": fingerprint_dast(template_id, matched_at),
            "evidence": {
                "kind": "dast",
                "engine": "nuclei",
                "template_id": template_id,
                "matched_at": matched_at,
                "type": str_at(&v, "/type"),
                "request": str_at(&v, "/request"),
                "response": str_at(&v, "/response"),
                "curl_command": str_at(&v, "/curl-command"),
                "description": str_at(&v, "/info/description"),
                // Proposed remediation (nuclei templates often carry one; the triage
                // agent enriches it when absent) plus the documented references.
                "remediation": str_at(&v, "/info/remediation"),
                "cwe": join_str_array(&v, "/info/classification/cwe-id"),
                "cve": join_str_array(&v, "/info/classification/cve-id"),
                "reference": join_str_array(&v, "/info/reference")
            }
        }));
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_allowlist_is_closed_to_nuclei() {
        assert!(is_allowlisted_program("nuclei"));
        for bad in ["sqlmap", "zap", "semgrep", "bash", "sh", "curl", "nmap", "ffuf"] {
            assert!(!is_allowlisted_program(bad), "{bad} must not be allowlisted");
        }
    }

    #[test]
    fn url_parsing_enforces_scheme_host_and_no_credentials() {
        assert_eq!(
            parse_http_url("https://staging.example.com/app").unwrap(),
            ("staging.example.com".into(), "https://staging.example.com/app".into())
        );
        assert_eq!(
            parse_http_url("http://10.0.0.5:8080").unwrap().0,
            "10.0.0.5"
        );
        assert_eq!(parse_http_url("HTTPS://Up.Example.COM").unwrap().0, "up.example.com".to_string().to_ascii_lowercase());
        for bad in [
            "ftp://example.com",
            "file:///etc/passwd",
            "example.com",           // no scheme
            "https://",              // no host
            "https://user:pass@evil.com", // embedded creds
            "https:// space.com",    // invalid host char
            "javascript:alert(1)",
        ] {
            assert!(parse_http_url(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn authorized_target_url_reads_from_config() {
        let cfg = json!({"url": "https://staging.example.com"});
        assert_eq!(authorized_target_url(&cfg).unwrap().0, "staging.example.com");
        assert!(authorized_target_url(&json!({})).is_err());
        assert!(authorized_target_url(&json!({"url": "ssh://x"})).is_err());
    }

    #[test]
    fn scope_guard_matches_only_authorized_host() {
        assert!(in_scope("https://staging.example.com/a?b=1", "staging.example.com"));
        assert!(in_scope("http://staging.example.com:8080/x", "staging.example.com"));
        assert!(!in_scope("https://evil.com/staging.example.com", "staging.example.com"));
        assert!(!in_scope("https://sub.staging.example.com", "staging.example.com"));
        assert!(!in_scope("not-a-url", "staging.example.com"));
    }

    #[test]
    fn nuclei_argv_is_fixed_and_rejects_bad_input() {
        let argv = build_nuclei_argv("https://staging.example.com", "medium,high,critical", 20, 10).unwrap();
        assert_eq!(argv[0], "nuclei");
        assert!(argv.contains(&"-jsonl".to_string()));
        assert!(argv.contains(&"-no-interactsh".to_string()));
        // redirects are disabled so scope holds at the traffic layer (blind-SSRF guard)
        assert!(argv.contains(&"-disable-redirects".to_string()));
        // baked templates dir passed via -t (no runtime download) + scoped tags
        assert!(argv.contains(&"-t".to_string()));
        assert!(argv.contains(&NUCLEI_TEMPLATES_DIR.to_string()));
        assert!(argv.contains(&"-tags".to_string()));
        assert!(argv.contains(&NUCLEI_TAGS.to_string()));
        // target is present and exactly the authorized url
        let i = argv.iter().position(|a| a == "-u").unwrap();
        assert_eq!(argv[i + 1], "https://staging.example.com");
        // bad inputs never reach argv
        assert!(build_nuclei_argv("ftp://x", "high", 20, 10).is_err());
        assert!(build_nuclei_argv("https://x.com", "bogus", 20, 10).is_err());
        assert!(build_nuclei_argv("https://x.com", "high", 0, 10).is_err());
    }

    #[test]
    fn map_nuclei_jsonl_maps_in_scope_and_drops_out_of_scope() {
        let in_scope_line = json!({
            "template-id": "CVE-2021-1234",
            "info": {"name": "Example RCE", "severity": "critical", "description": "d",
                     "reference": ["https://x"], "classification": {"cwe-id": ["CWE-77"], "cve-id": ["CVE-2021-1234"]}},
            "type": "http",
            "host": "staging.example.com",
            "matched-at": "https://staging.example.com/vuln?x=1",
            "request": "GET /vuln?x=1 HTTP/1.1",
            "response": "HTTP/1.1 200 OK",
            "curl-command": "curl ..."
        });
        let out_of_scope_line = json!({
            "template-id": "CVE-9999",
            "info": {"name": "Elsewhere", "severity": "high"},
            "host": "other.com",
            "matched-at": "https://other.com/x"
        });
        let bytes = format!("{in_scope_line}\n\ngarbage-not-json\n{out_of_scope_line}\n");
        let findings = map_nuclei_jsonl(bytes.as_bytes(), "staging.example.com");
        assert_eq!(findings.len(), 1, "only the in-scope finding survives");
        let f = &findings[0];
        assert_eq!(f["title"], "Example RCE");
        assert_eq!(f["severity"], "critical");
        assert_eq!(f["evidence"]["template_id"], "CVE-2021-1234");
        assert_eq!(f["evidence"]["request"], "GET /vuln?x=1 HTTP/1.1");
        assert_eq!(
            f["fingerprint"],
            fingerprint_dast("CVE-2021-1234", "https://staging.example.com/vuln?x=1")
        );
        // Empty / malformed input never panics.
        assert!(map_nuclei_jsonl(b"", "staging.example.com").is_empty());
        assert!(map_nuclei_jsonl(b"{}\n", "staging.example.com").is_empty());
    }

    #[test]
    fn severity_normalization_and_fingerprint_stability() {
        assert_eq!(normalize_severity("CRITICAL"), "critical");
        assert_eq!(normalize_severity("unknown"), "info");
        assert_eq!(normalize_severity(""), "info");
        assert_eq!(normalize_severity("weird"), "medium");
        let a = fingerprint_dast("t1", "https://h/x");
        assert_eq!(a, fingerprint_dast("t1", "https://h/x"));
        assert_ne!(a, fingerprint_dast("t1", "https://h/y"));
    }
}
