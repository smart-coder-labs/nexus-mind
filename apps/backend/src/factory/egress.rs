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
}

impl Upstream {
    pub fn host(self) -> &'static str {
        match self {
            Upstream::Anthropic => "api.anthropic.com",
            Upstream::Nexusmind => "api.nexusmind.smartcoderlabs.com",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunToken {
    pub run_id: String,
    pub expires_unix: i64,
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

/// Signs `v1.<run_id>.<expires_unix>.<hex hmac>`. `run_id` must be
/// `[A-Za-z0-9-]{1,64}` so the dotted layout stays unambiguous.
pub fn sign_run_token(key: &[u8], run_id: &str, expires_unix: i64) -> Result<String, TokenError> {
    if !valid_run_id(run_id) {
        return Err(TokenError::Malformed);
    }
    let payload = format!("v1.{run_id}.{expires_unix}");
    Ok(format!("{payload}.{}", hex::encode(mac(key, &payload))))
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
    let mut parts = token.split('.');
    let (Some("v1"), Some(run_id), Some(expires), Some(signature), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(TokenError::Malformed);
    };
    let expires_unix: i64 = expires.parse().map_err(|_| TokenError::Malformed)?;
    let signature = hex::decode(signature).map_err(|_| TokenError::Malformed)?;
    if !valid_run_id(run_id) {
        return Err(TokenError::Malformed);
    }
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(format!("v1.{run_id}.{expires_unix}").as_bytes());
    // Constant-time comparison.
    mac.verify_slice(&signature)
        .map_err(|_| TokenError::BadSignature)?;
    if now_unix > expires_unix {
        return Err(TokenError::Expired);
    }
    Ok(RunToken {
        run_id: run_id.to_string(),
        expires_unix,
    })
}

/// Classifies one request. `target` is the HTTP request target (origin-form for
/// reverse routes, `host:port` authority-form for CONNECT).
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
        _ => return deny("unknown_route", Some(&run)),
    };
    let path_and_query = if tail.starts_with('/') {
        tail.to_string()
    } else {
        format!("/{tail}")
    };
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
        sign_run_token(KEY, run, expires).unwrap()
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
                run_id: "run-1".into(),
                expires_unix: NOW + 60
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
            "v1",
            "v2.run-1.1.00",
            "v1.run-1.notanumber.00",
            "v1.run-1.1.zz",
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
                sign_run_token(KEY, bad, NOW),
                Err(TokenError::Malformed),
                "{bad:?}"
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
                    run_id: "run-1".into(),
                    expires_unix: NOW + 60
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
                    run_id: "run-1".into(),
                    expires_unix: NOW + 60
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
