//! Pure helpers for the AI Content Manager's post imagery.
//!
//! No I/O and no process spawning live here. The worker calls these functions to
//! validate the agent's design system, compose a brand-consistent prompt, build
//! the image-generator argv, and parse the generator's JSON result. Keeping it
//! pure makes it exhaustively unit-testable and — as with `security_scan` — keeps
//! the program allowlist in one reviewable place, separate from the runner that
//! actually spawns a process.
//!
//! Why the worker generates the images rather than the agent: the content agent
//! runs with `allowedTools = Read,Skill,mcp__…` and no Bash, and the package
//! manager allowlist used by the test runner deliberately does not include image
//! tooling. So the agent only *describes* the image it wants (`image_prompt`) and
//! the worker produces it under a fixed argv, exactly like the security scanners.

use anyhow::{bail, Result};
use serde_json::Value;

/// The only program this module's runner may spawn. Deliberately separate from
/// both the package-manager allowlist (`run_allowlisted_commands`) and the
/// security-scanner allowlist, so widening one never widens another.
pub const HIGGSFIELD: &str = "higgsfield";
pub const IMAGE_PROGRAM_ALLOWLIST: [&str; 1] = [HIGGSFIELD];

/// Higgsfield job type used for post imagery. `higgsfield model get gpt_image_2`
/// reports `prompt` as the only required param, plus the enums mirrored below.
pub const DEFAULT_JOB_TYPE: &str = "gpt_image_2";

/// Accepted `--aspect-ratio` values for `gpt_image_2`.
pub const ASPECT_RATIOS: [&str; 9] = [
    "auto", "1:1", "4:3", "3:4", "16:9", "21:9", "9:16", "3:2", "2:3",
];

/// LinkedIn renders a square well in both the feed and on mobile, so it is the
/// default when the design system does not pin one.
pub const DEFAULT_ASPECT_RATIO: &str = "1:1";

/// Hard cap on images per post. LinkedIn's multi-image post accepts 2..=20, but
/// each image is a paid generation and a carousel beyond a handful reads as
/// filler, so the template caps it far lower than the API does.
pub const MAX_IMAGES_PER_POST: usize = 4;

/// Hard cap on generations per run, so a misconfigured `posts_per_run` ×
/// `per_post` cannot fan out into an unbounded spend.
pub const MAX_IMAGE_JOBS_PER_RUN: usize = 12;

/// Wall-clock ceiling handed to `higgsfield generate create --wait`.
pub const WAIT_TIMEOUT: &str = "10m";

// ── Program allowlist ────────────────────────────────────────────────────────

pub fn is_allowlisted_program(program: &str) -> bool {
    IMAGE_PROGRAM_ALLOWLIST.contains(&program)
}

// ── Design system ────────────────────────────────────────────────────────────

/// The brand contract captured once when the agent is created, and applied
/// unchanged to every image the agent ever generates. This is the whole point of
/// asking for it at creation time: a per-post prompt drifts run to run, a fixed
/// prefix does not.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DesignSystem {
    pub palette: Vec<String>,
    pub typography: String,
    pub visual_style: String,
    pub imagery_rules: String,
    pub avoid: String,
    pub aspect_ratio: String,
    pub logo_url: String,
}

/// A hex colour, `#RGB` or `#RRGGBB`. Anything else is rejected rather than
/// silently passed to the model, where it would read as prose and skew the image.
pub fn is_hex_colour(value: &str) -> bool {
    let Some(body) = value.strip_prefix('#') else {
        return false;
    };
    matches!(body.len(), 3 | 6) && body.chars().all(|c| c.is_ascii_hexdigit())
}

/// `http(s)` URL with a host. Used for the optional logo reference; the worker
/// downloads it and passes the local file to the generator.
pub fn is_http_url(value: &str) -> bool {
    reqwest::Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| !host.is_empty())
}

fn text_field(value: &Value, key: &str, limit: usize) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or_default()
        .chars()
        .take(limit)
        .collect()
}

/// Read the `design_system` object out of an agent config. Unknown and malformed
/// entries are dropped rather than erroring: validation at create time is what
/// rejects a bad design system, and a run must not die because one colour is
/// malformed.
pub fn design_system_from_config(config: &Value) -> DesignSystem {
    let Some(source) = config.get("design_system") else {
        return DesignSystem::default();
    };
    let palette = source
        .get("palette")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| is_hex_colour(value))
                .take(8)
                .map(|value| value.to_uppercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let aspect_ratio = source
        .get("aspect_ratio")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| ASPECT_RATIOS.contains(value))
        .unwrap_or(DEFAULT_ASPECT_RATIO)
        .to_string();
    let logo_url = source
        .get("logo_url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| is_http_url(value))
        .unwrap_or_default()
        .to_string();
    DesignSystem {
        palette,
        typography: text_field(source, "typography", 200),
        visual_style: text_field(source, "visual_style", 400),
        imagery_rules: text_field(source, "imagery_rules", 400),
        avoid: text_field(source, "avoid", 300),
        aspect_ratio,
        logo_url,
    }
}

/// A design system is usable for generation once it says something about how the
/// brand looks. Colours alone, or a style sentence alone, are enough; nothing at
/// all is not — an empty system produces a different-looking image every run,
/// which is precisely the failure this feature exists to prevent.
pub fn design_system_is_usable(system: &DesignSystem) -> bool {
    !system.palette.is_empty()
        || !system.visual_style.is_empty()
        || !system.imagery_rules.is_empty()
        || !system.typography.is_empty()
}

// ── Prompt composition ───────────────────────────────────────────────────────

/// Constraints applied to every generated post image regardless of brand.
///
/// Text is excluded on purpose: image models garble words, and a LinkedIn post
/// already carries its copy as real text, so rendered lettering only adds a
/// failure mode. `avoid` in the design system extends this, never replaces it.
const UNIVERSAL_CONSTRAINTS: &str = "no text, no words, no letters, no numbers, no watermarks, no signatures, no user-interface chrome";

/// Build the final generator prompt: the brand contract first and identical on
/// every call, the post's own angle last.
///
/// The order matters. Brand lines lead so they anchor the composition, and the
/// per-post request is scoped to the subject rather than the look — which is why
/// the agent is told to describe *what* the image shows, not how it is styled.
pub fn compose_image_prompt(system: &DesignSystem, post_prompt: &str) -> String {
    let subject: String = post_prompt.trim().chars().take(1200).collect();
    let mut lines = Vec::new();
    lines.push("Brand image for a LinkedIn post. Follow the brand system exactly; it is identical for every image in this brand.".to_string());
    if !system.visual_style.is_empty() {
        lines.push(format!("Visual style: {}", system.visual_style));
    }
    if !system.palette.is_empty() {
        lines.push(format!(
            "Colour palette (use only these): {}",
            system.palette.join(", ")
        ));
    }
    if !system.typography.is_empty() {
        // Kept even though lettering is forbidden: typography still describes the
        // brand's geometry and weight, which shapes the forms in the image.
        lines.push(format!(
            "Typographic character of the brand (as visual feeling only, never rendered as letters): {}",
            system.typography
        ));
    }
    if !system.imagery_rules.is_empty() {
        lines.push(format!("Imagery rules: {}", system.imagery_rules));
    }
    lines.push(format!("Subject of this image: {subject}"));
    let avoid = if system.avoid.is_empty() {
        UNIVERSAL_CONSTRAINTS.to_string()
    } else {
        format!("{UNIVERSAL_CONSTRAINTS}, {}", system.avoid)
    };
    lines.push(format!("Avoid: {avoid}"));
    lines.join("\n")
}

// ── Argv builder ─────────────────────────────────────────────────────────────

/// Build the `higgsfield generate create` argv. Only validated values reach the
/// command line, and every flag is fixed here rather than taken from config, so
/// agent- or user-supplied text can never introduce a new flag.
///
/// `--wait` makes the CLI block until the job finishes and emit the terminal job
/// JSON, which avoids a create/poll pair and keeps the runner a single spawn.
pub fn build_higgsfield_argv(
    prompt: &str,
    aspect_ratio: &str,
    reference_paths: &[String],
) -> Result<Vec<String>> {
    if prompt.trim().is_empty() {
        bail!("empty_image_prompt")
    }
    if !ASPECT_RATIOS.contains(&aspect_ratio) {
        bail!("invalid_aspect_ratio")
    }
    let mut argv = vec![
        HIGGSFIELD.to_string(),
        "generate".to_string(),
        "create".to_string(),
        DEFAULT_JOB_TYPE.to_string(),
        "--prompt".to_string(),
        prompt.to_string(),
        "--aspect-ratio".to_string(),
        aspect_ratio.to_string(),
    ];
    // A leading dash would be read as a flag; such a path cannot come from the
    // worker's own temp dir, so it is a bug or an attack and is rejected.
    for path in reference_paths.iter().take(4) {
        if path.starts_with('-') || path.trim().is_empty() {
            bail!("invalid_reference_path")
        }
        argv.push("--image-references".to_string());
        argv.push(path.clone());
    }
    argv.push("--wait".to_string());
    argv.push("--wait-timeout".to_string());
    argv.push(WAIT_TIMEOUT.to_string());
    argv.push("--json".to_string());
    Ok(argv)
}

// ── Result parsing ───────────────────────────────────────────────────────────

/// Pull the finished image URL out of `higgsfield … --wait --json` output.
///
/// The CLI prints progress before the JSON payload and may emit either a single
/// job object or an array of them, so this scans for the last balanced JSON value
/// in the stream and reads `result_url` from a completed job. A job that finished
/// in any state other than `completed` is an error, not an empty result — a silent
/// empty would publish the post with no image and no explanation.
pub fn parse_higgsfield_result(stdout: &str) -> Result<String> {
    let value = last_json_value(stdout).ok_or_else(|| anyhow::anyhow!("image_result_not_json"))?;
    let jobs: Vec<&Value> = match &value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    let mut last_status = String::new();
    for job in jobs {
        let status = job
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if status == "completed" {
            if let Some(url) = job
                .get("result_url")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|value| is_http_url(value))
            {
                return Ok(url.to_string());
            }
        }
        if !status.is_empty() {
            last_status = status.to_string();
        }
    }
    if last_status.is_empty() {
        bail!("image_result_missing_url")
    }
    bail!("image_job_{last_status}")
}

/// Find the last *top-level* JSON object or array in mixed CLI output.
///
/// Scanning forward and jumping past each value that parses is what makes it
/// top-level: searching backwards from the last brace would instead find the
/// innermost nested object (Higgsfield echoes the request `params` inside the
/// job), which parses fine and carries no `result_url`. Tracking string and
/// escape state means a brace inside an echoed prompt cannot end a value early.
fn last_json_value(text: &str) -> Option<Value> {
    let mut result = None;
    let mut index = 0usize;
    while index < text.len() {
        let Some(offset) = text[index..].find(['{', '[']) else {
            break;
        };
        let start = index + offset;
        if let Some(end) = balanced_end(text, start) {
            if let Ok(value) = serde_json::from_str::<Value>(&text[start..=end]) {
                if value.is_object() || value.is_array() {
                    result = Some(value);
                    // Skip the whole value so its nested objects are not
                    // mistaken for later top-level payloads.
                    index = end + 1;
                    continue;
                }
            }
        }
        index = start + 1;
    }
    result
}

fn balanced_end(text: &str, start: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in text.char_indices().skip_while(|(i, _)| *i < start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

// ── Per-post plan ────────────────────────────────────────────────────────────

/// How many images to generate for one post, clamped to the template's caps and
/// to what is still left of the run's budget.
pub fn images_for_post(configured: usize, remaining_budget: usize) -> usize {
    configured.clamp(0, MAX_IMAGES_PER_POST).min(remaining_budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_higgsfield_is_allowlisted() {
        assert!(is_allowlisted_program(HIGGSFIELD));
        for program in ["codex", "sh", "bash", "curl", "npm", "semgrep"] {
            assert!(
                !is_allowlisted_program(program),
                "{program} must not be spawnable by the image runner"
            );
        }
    }

    #[test]
    fn hex_colours_are_validated() {
        for value in ["#fff", "#0B0B0F", "#ABCDEF"] {
            assert!(is_hex_colour(value), "{value} should be valid");
        }
        for value in ["fff", "#gg0000", "#0B0B0", "", "#", "rgb(0,0,0)"] {
            assert!(!is_hex_colour(value), "{value} should be rejected");
        }
    }

    #[test]
    fn design_system_drops_malformed_entries_instead_of_failing() {
        let config = json!({"design_system":{
            "palette":["#0B0B0F","not-a-colour","#3b82f6"],
            "visual_style":"editorial dark-tech",
            "aspect_ratio":"banana",
            "logo_url":"javascript:alert(1)"
        }});
        let system = design_system_from_config(&config);
        assert_eq!(system.palette, vec!["#0B0B0F", "#3B82F6"]);
        assert_eq!(system.visual_style, "editorial dark-tech");
        // An unknown ratio falls back to the default rather than reaching the CLI.
        assert_eq!(system.aspect_ratio, DEFAULT_ASPECT_RATIO);
        // A non-http scheme is not a logo the worker will fetch.
        assert_eq!(system.logo_url, "");
    }

    #[test]
    fn a_missing_design_system_is_not_usable() {
        assert!(!design_system_is_usable(&DesignSystem::default()));
        assert!(design_system_is_usable(&DesignSystem {
            palette: vec!["#000000".into()],
            ..Default::default()
        }));
        assert!(design_system_is_usable(&DesignSystem {
            visual_style: "minimal".into(),
            ..Default::default()
        }));
    }

    #[test]
    fn the_brand_prefix_is_identical_across_posts() {
        let system = DesignSystem {
            palette: vec!["#0B0B0F".into(), "#3B82F6".into()],
            visual_style: "editorial dark-tech".into(),
            imagery_rules: "abstract renders, no stock people".into(),
            ..Default::default()
        };
        let first = compose_image_prompt(&system, "a memory graph forming");
        let second = compose_image_prompt(&system, "an agent handing off work");
        let prefix_of = |text: &str| {
            text.lines()
                .take_while(|line| !line.starts_with("Subject of this image:"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            prefix_of(&first),
            prefix_of(&second),
            "the brand contract must not vary between posts"
        );
        assert!(first.contains("a memory graph forming"));
        assert!(second.contains("an agent handing off work"));
    }

    #[test]
    fn every_prompt_forbids_rendered_text_and_keeps_brand_avoids() {
        let system = DesignSystem {
            avoid: "people, gradients".into(),
            ..Default::default()
        };
        let prompt = compose_image_prompt(&system, "a subject");
        assert!(prompt.contains("no text"));
        assert!(prompt.contains("no watermarks"));
        // The brand's own avoid list extends the universal one rather than
        // replacing it.
        assert!(prompt.contains("people, gradients"));
    }

    #[test]
    fn argv_pins_every_flag_and_rejects_bad_input() {
        let argv = build_higgsfield_argv("a prompt", "1:1", &["/tmp/logo.png".into()]).unwrap();
        assert_eq!(argv[0], HIGGSFIELD);
        assert_eq!(argv[3], DEFAULT_JOB_TYPE);
        assert!(argv.contains(&"--wait".to_string()));
        assert!(argv.contains(&"--json".to_string()));
        assert!(argv.contains(&"/tmp/logo.png".to_string()));
        assert_eq!(
            build_higgsfield_argv("", "1:1", &[]).unwrap_err().to_string(),
            "empty_image_prompt"
        );
        assert_eq!(
            build_higgsfield_argv("p", "5:4", &[]).unwrap_err().to_string(),
            "invalid_aspect_ratio"
        );
        // A dash-leading path would be parsed as a flag by the CLI.
        assert_eq!(
            build_higgsfield_argv("p", "1:1", &["--json".into()])
                .unwrap_err()
                .to_string(),
            "invalid_reference_path"
        );
    }

    #[test]
    fn result_url_is_read_from_the_completed_job() {
        let stdout = "submitting job...\nwaiting\n{\"id\":\"abc\",\"status\":\"completed\",\"result_url\":\"https://cdn.example.com/a.png\"}\n";
        assert_eq!(
            parse_higgsfield_result(stdout).unwrap(),
            "https://cdn.example.com/a.png"
        );
    }

    #[test]
    fn an_array_payload_and_a_braced_prompt_still_parse() {
        let stdout = r#"progress
[{"status":"completed","params":{"prompt":"a {weird} prompt with \"quotes\""},"result_url":"https://cdn.example.com/b.png"}]"#;
        assert_eq!(
            parse_higgsfield_result(stdout).unwrap(),
            "https://cdn.example.com/b.png"
        );
    }

    #[test]
    fn a_failed_job_is_an_error_not_an_empty_result() {
        let stdout = r#"{"id":"abc","status":"failed"}"#;
        assert_eq!(
            parse_higgsfield_result(stdout).unwrap_err().to_string(),
            "image_job_failed"
        );
        assert_eq!(
            parse_higgsfield_result("no json here")
                .unwrap_err()
                .to_string(),
            "image_result_not_json"
        );
    }

    #[test]
    fn image_count_respects_both_the_cap_and_the_remaining_budget() {
        assert_eq!(images_for_post(1, 12), 1);
        assert_eq!(images_for_post(99, 12), MAX_IMAGES_PER_POST);
        assert_eq!(images_for_post(4, 2), 2);
        assert_eq!(images_for_post(4, 0), 0);
    }
}
