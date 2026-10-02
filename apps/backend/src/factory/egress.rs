//! Pure decision core of the sandbox egress proxy (factory F1, design §1).
//!
//! A sandbox pod holds no credential. Its only secret is a short-lived, HMAC-signed
//! run token. Every request is classified here as a reverse route (the proxy
//! injects the real credential), an allow-listed tunnel (no credential, public
//! registries only) or a denial. The network side lives in the
//! `factory_egress_proxy` binary; everything that decides lives here, so it can be
//! tested without sockets.

/// Which upstream a reverse route targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upstream {
    Anthropic,
    Nexusmind,
    /// Private npm packages (GitHub Packages), read-only, with the org's token.
    GithubPackages,
}

impl Upstream {
    pub fn host(self) -> &'static str {
        match self {
            Upstream::Anthropic => "api.anthropic.com",
            Upstream::Nexusmind => "api.nexusmind.smartcoderlabs.com",
            Upstream::GithubPackages => "npm.pkg.github.com",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunToken {
    /// Selects which organization's NexusMind bot key the proxy injects.
    pub org_id: String,
    pub run_id: String,
    pub expires_unix: i64,
    /// Extra hosts this run may tunnel to (v3 tokens: the agent's targets).
    pub hosts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenError {
    Malformed,
    BadSignature,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Reverse {
        run: RunToken,
        upstream: Upstream,
        /// Path and query forwarded upstream, always starting with `/`.
        path_and_query: String,
    },
    Tunnel {
        run: RunToken,
        host: String,
        port: u16,
    },
    Deny {
        reason: &'static str,
        /// Known only when the token verified; never the token itself.
        run_id: Option<String>,
    },
}

/// Signs `v2.<org_id>.<run_id>.<expires_unix>.<hex hmac>`. Both ids must be
/// `[A-Za-z0-9-]{1,64}` so the dotted layout stays unambiguous.
pub fn sign_run_token(
    key: &[u8],
    org_id: &str,
    run_id: &str,
    expires_unix: i64,
) -> Result<String, TokenError> {
    if !valid_run_id(org_id) || !valid_run_id(run_id) {
        return Err(TokenError::Malformed);
    }
    let payload = format!("v2.{org_id}.{run_id}.{expires_unix}");
    Ok(format!("{payload}.{}", hex::encode(mac(key, &payload))))
}

/// Signs a v3 token: like v2 plus `hosts`, extra tunnel destinations for this
/// run only (the agent's target hosts). Hosts must be lowercase DNS names; IP
/// literals are refused so no token can ever name a metadata or node address.
pub fn sign_run_token_with_hosts(
    key: &[u8],
    org_id: &str,
    run_id: &str,
    expires_unix: i64,
    hosts: &[String],
) -> Result<String, TokenError> {
    if !valid_run_id(org_id)
        || !valid_run_id(run_id)
        || hosts.is_empty()
        || hosts.len() > MAX_TOKEN_HOSTS
        || !hosts.iter().all(|host| valid_host(host))
    {
        return Err(TokenError::Malformed);
    }
    use base64::Engine;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hosts.join(","));
    let payload = format!("v3.{org_id}.{run_id}.{expires_unix}.{encoded}");
    Ok(format!("{payload}.{}", hex::encode(mac(key, &payload))))
}

/// Whether a tunnel may connect to `ip`. A name is checked by where it resolves,
/// not by its spelling: `metadata.google.internal`, `*.nip.io` or a rebinding
/// domain can all point inside. Only globally routable addresses pass.
pub fn is_public_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            // IPv4-mapped and NAT64 addresses are judged by the IPv4 inside.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let segments = v6.segments();
            if segments[0] == 0x64 && segments[1] == 0xff9b {
                return false;
            }
            !(v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                // fc00::/7 unique local, fe80::/10 link local, 2001:db8::/32 documentation.
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x2001 && segments[1] == 0x0db8))
        }
    }
}

fn is_public_v4(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        // 100.64.0.0/10 carrier-grade NAT (also some cloud internal ranges).
        || (a == 100 && (64..=127).contains(&b))
        // 198.18.0.0/15 benchmarking, 192.0.0.0/24 protocol assignments, 240/4 reserved.
        || (a == 198 && (b == 18 || b == 19))
        || (a == 192 && b == 0 && c == 0)
        || a >= 240)
}

/// Most target hosts one token may carry.
const MAX_TOKEN_HOSTS: usize = 16;

/// A lowercase DNS name with at least one dot and no IP literal.
pub(crate) fn valid_host(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    (1..=253).contains(&host.len())
        && labels.len() >= 2
        && labels.iter().all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        // All-numeric labels throughout would be an IPv4 literal.
        && !labels.iter().all(|label| label.bytes().all(|b| b.is_ascii_digit()))
}

fn valid_run_id(run_id: &str) -> bool {
    (1..=64).contains(&run_id.len())
        && run_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn mac(key: &[u8], payload: &str) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Verifies signature (constant time) and expiry.
pub fn verify_run_token(key: &[u8], token: &str, now_unix: i64) -> Result<RunToken, TokenError> {
    let parts: Vec<&str> = token.split('.').collect();
    let (payload_len, hosts) = match parts.as_slice() {
        ["v2", _, _, _, _] => (4, Vec::new()),
        ["v3", _, _, _, encoded, _] => {
            use base64::Engine;
            let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| TokenError::Malformed)?;
            let joined = String::from_utf8(decoded).map_err(|_| TokenError::Malformed)?;
            let hosts: Vec<String> = joined.split(',').map(str::to_string).collect();
            if hosts.len() > MAX_TOKEN_HOSTS || !hosts.iter().all(|host| valid_host(host)) {
                return Err(TokenError::Malformed);
            }
            (5, hosts)
        }
        _ => return Err(TokenError::Malformed),
    };
    let (org_id, run_id) = (parts[1], parts[2]);
    let expires_unix: i64 = parts[3].parse().map_err(|_| TokenError::Malformed)?;
    let signature = hex::decode(parts[payload_len]).map_err(|_| TokenError::Malformed)?;
    if !valid_run_id(org_id) || !valid_run_id(run_id) {
        return Err(TokenError::Malformed);
    }
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(parts[..payload_len].join(".").as_bytes());
    // Constant-time comparison.
    mac.verify_slice(&signature)
        .map_err(|_| TokenError::BadSignature)?;
    if now_unix > expires_unix {
        return Err(TokenError::Expired);
    }
    Ok(RunToken {
        org_id: org_id.to_string(),
        run_id: run_id.to_string(),
        expires_unix,
        hosts,
    })
}

/// Classifies one request. `target` is the HTTP request target (origin-form for
/// reverse routes, `host:port` authority-form for CONNECT).
/// Marks a run token that may only open registry tunnels, never reach a
/// credentialed upstream: the token handed to code under test (a UUID run id never
/// ends with it).
pub const REGISTRY_ONLY_SUFFIX: &str = "-registry";

/// The run id of the registry-only token for `run_id`.
pub fn registry_only_run_id(run_id: &str) -> String {
    format!("{run_id}{REGISTRY_ONLY_SUFFIX}")
}

/// The npm scope a GitHub Packages request is for (`/@scope%2fname`,
/// `/@scope/name`, `/download/@scope/...`), lowercased. `None` for anything else:
/// the proxy only serves packages of the org's allowed scopes.
pub fn github_packages_scope(path_and_query: &str) -> Option<String> {
    let path = path_and_query.split('?').next().unwrap_or("");
    let rest = path
        .strip_prefix("/download/")
        .or_else(|| path.strip_prefix('/'))?;
    let decoded = rest.replacen("%2f", "/", 1).replacen("%2F", "/", 1);
    let scope = decoded.strip_prefix('@')?.split('/').next()?;
    let valid = !scope.is_empty()
        && scope
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    valid.then(|| format!("@{}", scope.to_ascii_lowercase()))
}

/// Exact paths the Anthropic route may reach.
const ANTHROPIC_PATHS: &[&str] = &["/v1/messages", "/v1/messages/count_tokens"];

pub fn decide(
    key: &[u8],
    method: &str,
    target: &str,
    proxy_authorization: Option<&str>,
    tunnel_allowlist: &[&str],
    now_unix: i64,
) -> Decision {
    let deny = |reason, run_id: Option<&RunToken>| Decision::Deny {
        reason,
        run_id: run_id.map(|run| run.run_id.clone()),
    };
    if method.eq_ignore_ascii_case("CONNECT") {
        let Some(header) = proxy_authorization else {
            return deny("no_run_token", None);
        };
        // Credentials were presented: anything that is not a valid run token is bad.
        let Some(token) = basic_run_token(header) else {
            return deny("bad_run_token", None);
        };
        let Ok(run) = verify_run_token(key, &token, now_unix) else {
            return deny("bad_run_token", None);
        };
        let Some((host, port)) = target.rsplit_once(':') else {
            return deny("bad_connect_target", Some(&run));
        };
        let host = host.to_ascii_lowercase();
        if !tunnel_allowlist
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&host))
            && !run.hosts.contains(&host)
        {
            return deny("host_not_allowlisted", Some(&run));
        }
        return match port.parse::<u16>() {
            Ok(443) => Decision::Tunnel {
                run,
                host,
                port: 443,
            },
            _ => deny("port_not_allowed", Some(&run)),
        };
    }
    if !target.starts_with('/') {
        return deny("forward_proxy_not_supported", None);
    }
    let Some(rest) = target.strip_prefix("/r/") else {
        return deny("no_run_token", None);
    };
    let (token, rest) = rest.split_once('/').unwrap_or((rest, ""));
    let Ok(run) = verify_run_token(key, token, now_unix) else {
        return deny("bad_run_token", None);
    };
    let (route, tail) = match rest.find(['/', '?']) {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let upstream = match route {
        "anthropic" => Upstream::Anthropic,
        "nexusmind" => Upstream::Nexusmind,
        "ghpkg" => Upstream::GithubPackages,
        _ => return deny("unknown_route", Some(&run)),
    };
    // Code under test holds a registry-only token: it may install packages
    // (including private ones, read-only) but never reach Claude or NexusMind.
    if run.run_id.ends_with(REGISTRY_ONLY_SUFFIX) && upstream != Upstream::GithubPackages {
        return deny("token_scope", Some(&run));
    }
    if upstream == Upstream::GithubPackages
        && !(method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD"))
    {
        return deny("method_not_allowed", Some(&run));
    }
    let path_and_query = if tail.starts_with('/') {
        tail.to_string()
    } else {
        format!("/{tail}")
    };
    // The Anthropic credential is an account-wide subscription token: only the
    // inference endpoints the CLI needs may use it. NexusMind needs no list here —
    // the org's bot key is already bounded by the `factory-bot` role.
    let path = path_and_query.split('?').next().unwrap_or("");
    if upstream == Upstream::Anthropic && !ANTHROPIC_PATHS.contains(&path) {
        return deny("path_not_allowed", Some(&run));
    }
    Decision::Reverse {
        run,
        upstream,
        path_and_query,
    }
}

/// The token from `Proxy-Authorization: Basic base64("run:<token>")`.
fn basic_run_token(header: &str) -> Option<String> {
    use base64::Engine;
    // Auth schemes are case-insensitive (RFC 9110 §11.1).
    let (scheme, encoded) = header.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    decoded.strip_prefix("run:").map(str::to_string)
}

/// Request headers to forward upstream: credentials and hop-by-hop headers are
/// always removed, so a sandbox can never choose its own identity.
pub fn scrub_request_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    const DROP: [&str; 11] = [
        "authorization",
        "x-api-key",
        "cookie",
        "proxy-authorization",
        "host",
        "connection",
        "keep-alive",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ];
    headers
        .iter()
        .filter(|(name, _)| !DROP.iter().any(|drop| name.eq_ignore_ascii_case(drop)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"test-signing-key-32-bytes-long!!";
    const NOW: i64 = 1_790_000_000;
    const ALLOW: &[&str] = &["registry.npmjs.org", "pypi.org", "files.pythonhosted.org"];

    fn token(run: &str, expires: i64) -> String {
        sign_run_token(KEY, "org-1", run, expires).unwrap()
    }

    fn basic(token: &str) -> String {
        use base64::Engine;
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("run:{token}"))
        )
    }

    #[test]
    fn a_signed_token_verifies_until_it_expires() {
        let t = token("run-1", NOW + 60);
        assert_eq!(
            verify_run_token(KEY, &t, NOW),
            Ok(RunToken {
                org_id: "org-1".into(),
                run_id: "run-1".into(),
                expires_unix: NOW + 60,
                hosts: vec![],
            })
        );
        assert_eq!(
            verify_run_token(KEY, &t, NOW + 61),
            Err(TokenError::Expired)
        );
    }

    #[test]
    fn a_tampered_or_foreign_token_is_rejected() {
        let t = token("run-1", NOW + 60);
        let other_run = t.replacen("run-1", "run-2", 1);
        let other_org = t.replacen("org-1", "org-2", 1);
        assert_eq!(
            verify_run_token(KEY, &other_org, NOW),
            Err(TokenError::BadSignature)
        );
        assert_eq!(
            verify_run_token(KEY, &other_run, NOW),
            Err(TokenError::BadSignature)
        );
        let later = t.replacen(&(NOW + 60).to_string(), &(NOW + 9999).to_string(), 1);
        assert_eq!(
            verify_run_token(KEY, &later, NOW),
            Err(TokenError::BadSignature)
        );
        assert_eq!(
            verify_run_token(b"another-key", &t, NOW),
            Err(TokenError::BadSignature)
        );
        for bad in [
            "",
            "v2",
            "v1.run-1.1.00",
            "v2.org-1.run-1.notanumber.00",
            "v2.org-1.run-1.1.zz",
        ] {
            assert_eq!(
                verify_run_token(KEY, bad, NOW),
                Err(TokenError::Malformed),
                "{bad}"
            );
        }
    }

    #[test]
    fn run_ids_that_would_break_the_layout_cannot_be_signed() {
        for bad in ["", "a.b", "run/1", &"x".repeat(65)] {
            assert_eq!(
                sign_run_token(KEY, "org-1", bad, NOW),
                Err(TokenError::Malformed),
                "{bad:?}"
            );
            assert_eq!(
                sign_run_token(KEY, bad, "run-1", NOW),
                Err(TokenError::Malformed),
                "org {bad:?}"
            );
        }
    }

    #[test]
    fn reverse_routes_carry_the_token_in_the_path() {
        let t = token("run-1", NOW + 60);
        assert_eq!(
            decide(
                KEY,
                "POST",
                &format!("/r/{t}/anthropic/v1/messages?beta=true"),
                None,
                ALLOW,
                NOW
            ),
            Decision::Reverse {
                run: RunToken {
                    org_id: "org-1".into(),
                    run_id: "run-1".into(),
                    expires_unix: NOW + 60,
                    hosts: vec![],
                },
                upstream: Upstream::Anthropic,
                path_and_query: "/v1/messages?beta=true".into(),
            }
        );
        match decide(KEY, "GET", &format!("/r/{t}/nexusmind"), None, ALLOW, NOW) {
            Decision::Reverse {
                upstream: Upstream::Nexusmind,
                path_and_query,
                ..
            } => {
                assert_eq!(path_and_query, "/")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn reverse_routes_without_a_valid_token_are_denied() {
        let expired = token("run-1", NOW - 1);
        for (target, reason) in [
            ("/v1/messages".to_string(), "no_run_token"),
            (
                "/r/garbage/anthropic/v1/messages".to_string(),
                "bad_run_token",
            ),
            (
                format!("/r/{expired}/anthropic/v1/messages"),
                "bad_run_token",
            ),
        ] {
            assert_eq!(
                decide(KEY, "POST", &target, None, ALLOW, NOW),
                Decision::Deny {
                    reason,
                    run_id: None
                },
                "{target}"
            );
        }
    }

    #[test]
    fn unknown_upstreams_and_forward_proxy_requests_are_denied() {
        let t = token("run-1", NOW + 60);
        assert_eq!(
            decide(
                KEY,
                "GET",
                &format!("/r/{t}/github/repos"),
                None,
                ALLOW,
                NOW
            ),
            Decision::Deny {
                reason: "unknown_route",
                run_id: Some("run-1".into())
            }
        );
        // Plain-HTTP forward proxying (absolute-form) would bypass injection rules.
        assert_eq!(
            decide(KEY, "GET", "http://evil.example/steal", None, ALLOW, NOW),
            Decision::Deny {
                reason: "forward_proxy_not_supported",
                run_id: None
            }
        );
    }

    #[test]
    fn tunnels_need_a_valid_token_an_allowlisted_host_and_port_443() {
        let t = token("run-1", NOW + 60);
        assert_eq!(
            decide(
                KEY,
                "CONNECT",
                "registry.npmjs.org:443",
                Some(&basic(&t)),
                ALLOW,
                NOW
            ),
            Decision::Tunnel {
                run: RunToken {
                    org_id: "org-1".into(),
                    run_id: "run-1".into(),
                    expires_unix: NOW + 60,
                    hosts: vec![],
                },
                host: "registry.npmjs.org".into(),
                port: 443,
            }
        );
        let denied =
            |target: &str, auth: Option<&str>| decide(KEY, "CONNECT", target, auth, ALLOW, NOW);
        assert_eq!(
            denied("api.anthropic.com:443", Some(&basic(&t))),
            Decision::Deny {
                reason: "host_not_allowlisted",
                run_id: Some("run-1".into())
            }
        );
        assert_eq!(
            denied("registry.npmjs.org:22", Some(&basic(&t))),
            Decision::Deny {
                reason: "port_not_allowed",
                run_id: Some("run-1".into())
            }
        );
        assert_eq!(
            denied("registry.npmjs.org:443", None),
            Decision::Deny {
                reason: "no_run_token",
                run_id: None
            }
        );
        assert_eq!(
            denied("registry.npmjs.org:443", Some("Basic bm90LWEtdG9rZW4=")),
            Decision::Deny {
                reason: "bad_run_token",
                run_id: None
            }
        );
        // Suffix tricks must not match the allowlist.
        assert_eq!(
            denied("registry.npmjs.org.evil.example:443", Some(&basic(&t))),
            Decision::Deny {
                reason: "host_not_allowlisted",
                run_id: Some("run-1".into())
            }
        );
    }

    #[test]
    fn the_basic_scheme_is_case_insensitive() {
        let t = token("run-1", NOW + 60);
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(format!("run:{t}"));
        for scheme in ["basic", "BASIC", "Basic"] {
            assert!(
                matches!(
                    decide(
                        KEY,
                        "CONNECT",
                        "pypi.org:443",
                        Some(&format!("{scheme} {b64}")),
                        ALLOW,
                        NOW
                    ),
                    Decision::Tunnel { .. }
                ),
                "{scheme}"
            );
        }
    }

    #[test]
    fn a_registry_only_token_opens_tunnels_but_never_reaches_upstreams() {
        let registry = token(&registry_only_run_id("run-1"), NOW + 60);
        for route in ["anthropic/v1/messages", "nexusmind/v1/context"] {
            assert_eq!(
                decide(
                    KEY,
                    "POST",
                    &format!("/r/{registry}/{route}"),
                    None,
                    ALLOW,
                    NOW
                ),
                Decision::Deny {
                    reason: "token_scope",
                    run_id: Some("run-1-registry".into())
                },
                "{route}"
            );
        }
        use base64::Engine;
        let basic = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("run:{registry}"))
        );
        assert!(matches!(
            decide(
                KEY,
                "CONNECT",
                "registry.npmjs.org:443",
                Some(&basic),
                ALLOW,
                NOW
            ),
            Decision::Tunnel { .. }
        ));
    }

    fn connect(token: &str, target: &str) -> Decision {
        use base64::Engine;
        let basic = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("run:{token}"))
        );
        decide(KEY, "CONNECT", target, Some(&basic), ALLOW, NOW)
    }

    #[test]
    fn v3_tokens_open_tunnels_to_their_own_hosts_only() {
        let hosts = vec!["app.acme.test".to_string(), "staging.acme.test".to_string()];
        let v3 = sign_run_token_with_hosts(KEY, "org-1", "run-1", NOW + 60, &hosts).unwrap();
        let run = verify_run_token(KEY, &v3, NOW).unwrap();
        assert_eq!(run.hosts, hosts);
        assert!(matches!(
            connect(&v3, "app.acme.test:443"),
            Decision::Tunnel { .. }
        ));
        assert!(matches!(
            connect(&v3, "APP.ACME.TEST:443"),
            Decision::Tunnel { .. }
        ));
        assert!(matches!(
            connect(&v3, "registry.npmjs.org:443"),
            Decision::Tunnel { .. }
        ));
        assert!(matches!(
            connect(&v3, "other.acme.test:443"),
            Decision::Deny {
                reason: "host_not_allowlisted",
                ..
            }
        ));
        // A v2 token (no hosts) cannot reach the targets.
        assert!(matches!(
            connect(&token("run-1", NOW + 60), "app.acme.test:443"),
            Decision::Deny {
                reason: "host_not_allowlisted",
                ..
            }
        ));
        // Reverse routes still work with v3.
        assert!(matches!(
            decide(
                KEY,
                "POST",
                &format!("/r/{v3}/anthropic/v1/messages"),
                None,
                ALLOW,
                NOW
            ),
            Decision::Reverse { .. }
        ));
    }

    #[test]
    fn signed_hosts_cannot_be_altered_and_must_be_dns_names() {
        let v3 =
            sign_run_token_with_hosts(KEY, "org-1", "run-1", NOW + 60, &["app.acme.test".into()])
                .unwrap();
        let mut parts: Vec<&str> = v3.split('.').collect();
        let forged_hosts = {
            use base64::Engine;
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("evil.example")
        };
        parts[4] = &forged_hosts;
        assert_eq!(
            verify_run_token(KEY, &parts.join("."), NOW),
            Err(TokenError::BadSignature)
        );
        for bad in [
            "169.254.169.254",
            "10.0.0.1",
            "[::1]",
            "Evil.Example",
            "a..b",
            "*.acme.test",
            "host:443",
            "",
        ] {
            assert_eq!(
                sign_run_token_with_hosts(KEY, "org-1", "run-1", NOW + 60, &[bad.to_string()]),
                Err(TokenError::Malformed),
                "{bad}"
            );
        }
        let many: Vec<String> = (0..17).map(|i| format!("h{i}.acme.test")).collect();
        assert_eq!(
            sign_run_token_with_hosts(KEY, "org-1", "run-1", NOW + 60, &many),
            Err(TokenError::Malformed)
        );
    }

    #[test]
    fn only_globally_routable_addresses_are_tunnel_destinations() {
        let ip = |text: &str| text.parse::<std::net::IpAddr>().unwrap();
        for public in ["104.18.32.7", "140.82.112.3", "2606:4700::6810:84e5"] {
            assert!(is_public_ip(ip(public)), "{public}");
        }
        for internal in [
            "127.0.0.1",
            "10.43.0.1",
            "10.42.3.9",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "::ffff:169.254.169.254",
            "::ffff:10.0.0.1",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(!is_public_ip(ip(internal)), "{internal}");
        }
    }

    #[test]
    fn github_packages_paths_name_their_scope() {
        for (path, scope) in [
            ("/@kasymir%2fui-commons", Some("@kasymir")),
            ("/@Kasymir%2Fui-commons", Some("@kasymir")),
            ("/@kasymir/ui-commons", Some("@kasymir")),
            ("/download/@xell-shop/ui/1.0.0/abc123", Some("@xell-shop")),
            ("/@kasymir%2fui?write=true", Some("@kasymir")),
            ("/", None),
            ("/-/whoami", None),
            ("/unscoped-package", None),
            ("/download/unscoped/1.0.0/x", None),
        ] {
            assert_eq!(github_packages_scope(path).as_deref(), scope, "{path}");
        }
    }

    #[test]
    fn github_packages_is_a_read_only_registry_route_open_to_registry_tokens() {
        let registry = token(&registry_only_run_id("run-1"), NOW + 60);
        let agent = token("run-1", NOW + 60);
        for t in [&registry, &agent] {
            for method in ["GET", "HEAD"] {
                assert!(
                    matches!(
                        decide(
                            KEY,
                            method,
                            &format!("/r/{t}/ghpkg/@acme%2fui"),
                            None,
                            ALLOW,
                            NOW
                        ),
                        Decision::Reverse {
                            upstream: Upstream::GithubPackages,
                            ..
                        }
                    ),
                    "{method}"
                );
            }
        }
        for method in ["PUT", "POST", "DELETE", "PATCH"] {
            assert_eq!(
                decide(
                    KEY,
                    method,
                    &format!("/r/{registry}/ghpkg/@acme%2fui"),
                    None,
                    ALLOW,
                    NOW
                ),
                Decision::Deny {
                    reason: "method_not_allowed",
                    run_id: Some("run-1-registry".into())
                },
                "{method}"
            );
        }
        assert_eq!(Upstream::GithubPackages.host(), "npm.pkg.github.com");
    }

    #[test]
    fn the_anthropic_route_only_reaches_inference_endpoints() {
        let t = token("run-1", NOW + 60);
        for allowed in [
            "/v1/messages",
            "/v1/messages?beta=true",
            "/v1/messages/count_tokens?beta=true",
        ] {
            assert!(
                matches!(
                    decide(
                        KEY,
                        "POST",
                        &format!("/r/{t}/anthropic{allowed}"),
                        None,
                        ALLOW,
                        NOW
                    ),
                    Decision::Reverse {
                        upstream: Upstream::Anthropic,
                        ..
                    }
                ),
                "{allowed}"
            );
        }
        for denied in [
            "/v1/organizations/keys",
            "/api/oauth/profile",
            "/v1/messages/../oauth",
            "/v1/messagesX",
            "/",
        ] {
            assert_eq!(
                decide(
                    KEY,
                    "POST",
                    &format!("/r/{t}/anthropic{denied}"),
                    None,
                    ALLOW,
                    NOW
                ),
                Decision::Deny {
                    reason: "path_not_allowed",
                    run_id: Some("run-1".into())
                },
                "{denied}"
            );
        }
    }

    #[test]
    fn credentials_and_hop_by_hop_headers_never_go_upstream() {
        let headers: Vec<(String, String)> = [
            ("Authorization", "Bearer placeholder"),
            ("x-api-key", "k"),
            ("Cookie", "c"),
            ("Proxy-Authorization", "p"),
            ("Host", "proxy"),
            ("Connection", "keep-alive"),
            ("Keep-Alive", "1"),
            ("TE", "trailers"),
            ("Trailer", "x"),
            ("Transfer-Encoding", "chunked"),
            ("Upgrade", "h2c"),
            ("anthropic-beta", "prompt-caching"),
            ("content-type", "application/json"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(
            scrub_request_headers(&headers),
            vec![
                ("anthropic-beta".to_string(), "prompt-caching".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ]
        );
    }
}
