//! Pure helpers for the `security_scan` autonomous-agent template (Phase 1: SAST + SCA).
//!
//! No I/O and no process spawning live here. The worker calls these functions to
//! validate/build scanner argv, normalize scanner-native severities, compute stable
//! dedupe fingerprints, and map raw scanner JSON into the canonical finding shape.
//! Keeping it pure makes it exhaustively unit-testable and isolates the security-
//! sensitive contract from the process-spawning runner.
//!
//! Contract source of truth:
//! `openspec/changes/security-scanner-templates/contracts.md` (C2/C3/C4).

use anyhow::{bail, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const SEMGREP: &str = "semgrep";
pub const OSV_SCANNER: &str = "osv-scanner";

/// Only these programs may be spawned by the security scanner runner. This is
/// intentionally separate from the package-manager allowlist used by the test
/// runner (`run_allowlisted_commands`), so widening one never widens the other.
/// v1 SCA is osv-scanner only: it reads npm/cargo/pip/etc. lockfiles natively, so
/// a per-ecosystem tool like `npm audit` is deferred (see contracts.md C3).
pub const SCANNER_PROGRAM_ALLOWLIST: [&str; 2] = [SEMGREP, OSV_SCANNER];

/// Bundled Semgrep rulesets accepted without treating the value as a path.
pub const SEMGREP_RULESET_ENUM: [&str; 3] = ["auto", "p/ci", "p/owasp-top-ten"];

/// Hard cap on scanner invocations per run (SAST + SCA + fallbacks).
pub const MAX_SCANNER_INVOCATIONS: usize = 4;

/// Unit separator used to join fingerprint parts so a value containing a common
/// delimiter can never collide with a different (field, value) split.
const US: char = '\u{1f}';

/// Canonical severities, lowest → highest.
pub const CANONICAL_SEVERITIES: [&str; 5] = ["info", "low", "medium", "high", "critical"];

// ── Program allowlist ────────────────────────────────────────────────────────

pub fn is_allowlisted_program(program: &str) -> bool {
    SCANNER_PROGRAM_ALLOWLIST.contains(&program)
}

// ── Argv builders (fixed flags; only validated slots are injected) ───────────

/// A Semgrep ruleset is valid iff it is a bundled name OR a checkout-relative
/// YAML path with no traversal, no absolute root, and a conservative charset.
pub fn validate_semgrep_ruleset(ruleset: &str) -> Result<()> {
    if SEMGREP_RULESET_ENUM.contains(&ruleset) {
        return Ok(());
    }
    if ruleset.is_empty() || ruleset.starts_with('/') || ruleset.contains("..") {
        bail!("invalid_ruleset")
    }
    let charset_ok = ruleset
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));
    let yaml_ext = ruleset.ends_with(".yml") || ruleset.ends_with(".yaml");
    if !charset_ok || !yaml_ext {
        bail!("invalid_ruleset")
    }
    Ok(())
}

/// Semgrep argv. `--json` with no `--output` writes the report to stdout, which the
/// runner captures. `<SCAN_ROOT>` is always last and host-controlled.
pub fn build_semgrep_argv(ruleset: &str, scan_root: &str, timeout_secs: u32) -> Result<Vec<String>> {
    validate_semgrep_ruleset(ruleset)?;
    Ok(vec![
        SEMGREP.into(),
        "--json".into(),
        "--quiet".into(),
        "--timeout".into(),
        timeout_secs.to_string(),
        "--config".into(),
        ruleset.into(),
        scan_root.into(),
    ])
}

/// osv-scanner argv. `--format json` writes the report to stdout.
pub fn build_osv_argv(scan_root: &str) -> Vec<String> {
    vec![
        OSV_SCANNER.into(),
        "--format".into(),
        "json".into(),
        "--recursive".into(),
        scan_root.into(),
    ]
}

// ── Severity normalization (C4) ──────────────────────────────────────────────

pub fn normalize_semgrep_severity(native: &str, impact_high: bool) -> &'static str {
    if impact_high {
        return "critical";
    }
    match native.to_ascii_uppercase().as_str() {
        "INFO" => "info",
        "WARNING" => "medium",
        "ERROR" => "high",
        _ => "medium",
    }
}

pub fn normalize_cvss_severity(score: f64) -> &'static str {
    if score >= 9.0 {
        "critical"
    } else if score >= 7.0 {
        "high"
    } else if score >= 4.0 {
        "medium"
    } else {
        "low"
    }
}

pub fn normalize_text_severity(text: &str) -> &'static str {
    match text.trim().to_ascii_lowercase().as_str() {
        "critical" => "critical",
        "high" => "high",
        "moderate" | "medium" => "medium",
        "low" => "low",
        "info" | "informational" | "none" => "info",
        _ => "medium",
    }
}

// ── Fingerprints (C2) ────────────────────────────────────────────────────────

fn sha256_hex(parts: &[&str]) -> String {
    let joined = parts.join(&US.to_string());
    hex::encode(Sha256::digest(joined.as_bytes()))
}

pub fn fingerprint_sast(rule_id: &str, path: &str, start_line: i64) -> String {
    sha256_hex(&["sast", rule_id, path, &start_line.to_string()])
}

pub fn fingerprint_sca(ecosystem: &str, package: &str, advisory_id: &str) -> String {
    sha256_hex(&["sca", ecosystem, package, advisory_id])
}

// ── Scanner JSON → canonical findings (C1) ───────────────────────────────────

fn str_at<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or("")
}

/// Map Semgrep `--json` output (`{"results":[...]}`) to canonical findings.
/// Malformed or empty input degrades to zero findings; it never panics.
pub fn map_semgrep_json(raw: &Value) -> Vec<Value> {
    let Some(results) = raw.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    results
        .iter()
        .map(|r| {
            let rule_id = str_at(r, "/check_id");
            let path = str_at(r, "/path");
            let start_line = r
                .pointer("/start/line")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let end_line = r.pointer("/end/line").and_then(Value::as_i64).unwrap_or(start_line);
            let message = str_at(r, "/extra/message");
            let native_sev = str_at(r, "/extra/severity");
            let impact_high = r
                .pointer("/extra/metadata/impact")
                .and_then(Value::as_str)
                .map(|s| s.eq_ignore_ascii_case("HIGH"))
                .unwrap_or(false);
            let severity = normalize_semgrep_severity(native_sev, impact_high);
            let snippet = str_at(r, "/extra/lines");
            let cwe = r
                .pointer("/extra/metadata/cwe")
                .cloned()
                .unwrap_or(json!([]));
            let references = r
                .pointer("/extra/metadata/references")
                .cloned()
                .unwrap_or(json!([]));
            let title = if message.is_empty() {
                rule_id.to_string()
            } else {
                message.to_string()
            };
            json!({
                "title": title,
                "severity": severity,
                "summary": format!("{rule_id} at {path}:{start_line}"),
                "fingerprint": fingerprint_sast(rule_id, path, start_line),
                "evidence": {
                    "kind": "sast",
                    "engine": "semgrep",
                    "rule_id": rule_id,
                    "path": path,
                    "start_line": start_line,
                    "end_line": end_line,
                    "snippet": snippet,
                    "message": message,
                    // Semgrep's autofix when the rule ships one; the triage agent
                    // proposes a concrete fix when this is empty.
                    "remediation": str_at(r, "/extra/fix"),
                    "cwe": cwe,
                    "references": references
                }
            })
        })
        .collect()
}

/// Best-effort numeric CVSS from an osv `severity` array whose `score` may be a
/// bare number or a CVSS vector string. Returns None when no number is present.
fn osv_numeric_cvss(vuln: &Value) -> Option<f64> {
    let sev = vuln.get("severity").and_then(Value::as_array)?;
    for entry in sev {
        if let Some(score) = entry.get("score") {
            if let Some(n) = score.as_f64() {
                return Some(n);
            }
            if let Some(s) = score.as_str() {
                if let Ok(n) = s.parse::<f64>() {
                    return Some(n);
                }
            }
        }
        if let Some(n) = entry.get("base_score").and_then(Value::as_f64) {
            return Some(n);
        }
    }
    None
}

fn osv_fixed_version(vuln: &Value) -> String {
    let Some(affected) = vuln.get("affected").and_then(Value::as_array) else {
        return String::new();
    };
    for a in affected {
        if let Some(ranges) = a.get("ranges").and_then(Value::as_array) {
            for range in ranges {
                if let Some(events) = range.get("events").and_then(Value::as_array) {
                    for ev in events {
                        if let Some(fixed) = ev.get("fixed").and_then(Value::as_str) {
                            return fixed.to_string();
                        }
                    }
                }
            }
        }
    }
    String::new()
}

/// Map osv-scanner `--format json` output to canonical findings.
pub fn map_osv_json(raw: &Value) -> Vec<Value> {
    let Some(results) = raw.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for result in results {
        let manifest_path = str_at(result, "/source/path");
        let Some(packages) = result.get("packages").and_then(Value::as_array) else {
            continue;
        };
        for pkg in packages {
            let name = str_at(pkg, "/package/name");
            let ecosystem = str_at(pkg, "/package/ecosystem");
            let installed = str_at(pkg, "/package/version");
            let Some(vulns) = pkg.get("vulnerabilities").and_then(Value::as_array) else {
                continue;
            };
            for vuln in vulns {
                let advisory_id = vuln.get("id").and_then(Value::as_str).unwrap_or("");
                let severity = match osv_numeric_cvss(vuln) {
                    Some(score) => normalize_cvss_severity(score),
                    None => normalize_text_severity(str_at(vuln, "/database_specific/severity")),
                };
                let fixed_version = osv_fixed_version(vuln);
                let remediation = if fixed_version.is_empty() {
                    format!("No fixed version published; evaluate replacing or mitigating {name}")
                } else {
                    format!("Upgrade {name} to {fixed_version} or later")
                };
                let title = format!("{advisory_id} in {name}");
                findings.push(json!({
                    "title": title,
                    "severity": severity,
                    "summary": format!("{name}@{installed} affected by {advisory_id}"),
                    "fingerprint": fingerprint_sca(ecosystem, name, advisory_id),
                    "evidence": {
                        "kind": "sca",
                        "engine": "osv-scanner",
                        "ecosystem": ecosystem,
                        "package": name,
                        "installed_version": installed,
                        "advisory_id": advisory_id,
                        "fixed_version": fixed_version,
                        "remediation": remediation,
                        "manifest_path": manifest_path
                    }
                }));
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_allowlist_is_closed() {
        assert!(is_allowlisted_program("semgrep"));
        assert!(is_allowlisted_program("osv-scanner"));
        for bad in ["nuclei", "sqlmap", "bash", "sh", "curl", "rm", "npx", "npm", "cargo"] {
            assert!(!is_allowlisted_program(bad), "{bad} must not be allowlisted");
        }
    }

    #[test]
    fn semgrep_ruleset_validation_blocks_traversal_and_junk() {
        for ok in ["auto", "p/ci", "p/owasp-top-ten", "rules/custom.yml", "a/b-c_d.yaml"] {
            assert!(validate_semgrep_ruleset(ok).is_ok(), "{ok} should pass");
        }
        for bad in [
            "",
            "/etc/passwd",
            "../secrets.yml",
            "rules/../../x.yml",
            "rules/custom.txt",
            "rules/custom",
            "rules/$(whoami).yml",
        ] {
            assert!(validate_semgrep_ruleset(bad).is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn semgrep_argv_is_fixed_and_injects_only_validated_slots() {
        let argv = build_semgrep_argv("p/ci", ".", 30).unwrap();
        assert_eq!(argv[0], "semgrep");
        assert!(argv.contains(&"--json".to_string()));
        assert!(!argv.iter().any(|a| a == "--output")); // report goes to stdout
        assert_eq!(argv.last().unwrap(), "."); // scan root is always last
        // A bad ruleset never reaches argv.
        assert!(build_semgrep_argv("../evil.yml", ".", 30).is_err());
    }

    #[test]
    fn osv_argv_is_fixed_json_recursive() {
        let argv = build_osv_argv(".");
        assert_eq!(argv[0], "osv-scanner");
        assert!(argv.contains(&"--recursive".to_string()));
        assert_eq!(argv.last().unwrap(), ".");
    }

    #[test]
    fn severity_normalization_matches_contract() {
        assert_eq!(normalize_semgrep_severity("ERROR", false), "high");
        assert_eq!(normalize_semgrep_severity("WARNING", false), "medium");
        assert_eq!(normalize_semgrep_severity("INFO", false), "info");
        assert_eq!(normalize_semgrep_severity("INFO", true), "critical");
        assert_eq!(normalize_semgrep_severity("weird", false), "medium");

        assert_eq!(normalize_cvss_severity(9.8), "critical");
        assert_eq!(normalize_cvss_severity(7.0), "high");
        assert_eq!(normalize_cvss_severity(4.0), "medium");
        assert_eq!(normalize_cvss_severity(0.0), "low");

        assert_eq!(normalize_text_severity("MODERATE"), "medium");
        assert_eq!(normalize_text_severity("critical"), "critical");
        assert_eq!(normalize_text_severity("bogus"), "medium");
    }

    #[test]
    fn fingerprints_are_stable_and_source_scoped() {
        let a = fingerprint_sast("rules.xss", "src/app.js", 42);
        let b = fingerprint_sast("rules.xss", "src/app.js", 42);
        assert_eq!(a, b, "same input must yield same fingerprint");
        assert_ne!(a, fingerprint_sast("rules.xss", "src/app.js", 43));
        // A SAST and SCA fingerprint of superficially similar values never collide.
        assert_ne!(
            fingerprint_sast("x", "y", 1),
            fingerprint_sca("x", "y", "1")
        );
    }

    #[test]
    fn map_semgrep_json_extracts_findings_and_degrades_safely() {
        let raw = json!({
            "results": [{
                "check_id": "rules.sqli",
                "path": "src/db.js",
                "start": {"line": 10},
                "end": {"line": 12},
                "extra": {
                    "message": "SQL injection",
                    "severity": "ERROR",
                    "lines": "db.query(userInput)",
                    "metadata": {"cwe": ["CWE-89"], "impact": "HIGH", "references": ["https://x"]}
                }
            }]
        });
        let findings = map_semgrep_json(&raw);
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f["title"], "SQL injection");
        assert_eq!(f["severity"], "critical"); // impact HIGH upgrades ERROR
        assert_eq!(f["evidence"]["rule_id"], "rules.sqli");
        assert_eq!(f["evidence"]["path"], "src/db.js");
        assert_eq!(
            f["fingerprint"],
            fingerprint_sast("rules.sqli", "src/db.js", 10)
        );

        // Malformed input never panics and yields nothing.
        assert!(map_semgrep_json(&json!({})).is_empty());
        assert!(map_semgrep_json(&json!({"results": "nope"})).is_empty());
        assert!(map_semgrep_json(&json!(null)).is_empty());
    }

    #[test]
    fn map_osv_json_extracts_findings_with_fixed_version() {
        let raw = json!({
            "results": [{
                "source": {"path": "package-lock.json"},
                "packages": [{
                    "package": {"name": "lodash", "version": "4.17.11", "ecosystem": "npm"},
                    "vulnerabilities": [{
                        "id": "GHSA-jf85-cpcp-j695",
                        "severity": [{"type": "CVSS_V3", "score": 9.1}],
                        "database_specific": {"severity": "CRITICAL"},
                        "affected": [{"ranges": [{"events": [{"introduced": "0"}, {"fixed": "4.17.12"}]}]}]
                    }]
                }]
            }]
        });
        let findings = map_osv_json(&raw);
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f["severity"], "critical");
        assert_eq!(f["evidence"]["package"], "lodash");
        assert_eq!(f["evidence"]["fixed_version"], "4.17.12");
        assert_eq!(
            f["fingerprint"],
            fingerprint_sca("npm", "lodash", "GHSA-jf85-cpcp-j695")
        );

        // Falls back to text severity when no numeric CVSS is present.
        let text_only = json!({
            "results": [{
                "source": {"path": "Cargo.lock"},
                "packages": [{
                    "package": {"name": "foo", "version": "0.1.0", "ecosystem": "crates.io"},
                    "vulnerabilities": [{
                        "id": "RUSTSEC-2020-0001",
                        "database_specific": {"severity": "moderate"}
                    }]
                }]
            }]
        });
        let f2 = map_osv_json(&text_only);
        assert_eq!(f2.len(), 1);
        assert_eq!(f2[0]["severity"], "medium");
        assert_eq!(f2[0]["evidence"]["fixed_version"], "");

        assert!(map_osv_json(&json!({})).is_empty());
    }
}
