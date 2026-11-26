//! Local-first secret & PII redactor applied before persisting episodic memory
//! or sending payloads to any external model or NexusMind server.

use regex::{Captures, Regex};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Category {
    Secret,
    Card,
    Email,
}

impl Category {
    fn tag(self) -> &'static str {
        match self {
            Self::Secret => "SECRET",
            Self::Card => "CARD",
            Self::Email => "EMAIL",
        }
    }
}

#[derive(Default)]
pub struct LocalRedactor {
    seen: HashMap<(Category, String), usize>,
    next: HashMap<Category, usize>,
}

impl LocalRedactor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn redact(&mut self, text: &str) -> String {
        let t = self.replace_whole(text, private_key_re(), Category::Secret);
        let t = self.replace_whole(&t, token_re(), Category::Secret);
        let t = self.replace_whole(&t, jwt_re(), Category::Secret);
        let t = self.replace_prefixed(&t, bearer_re(), Category::Secret);
        let t = self.replace_prefixed(&t, env_secret_re(), Category::Secret);
        let t = self.redact_cards(&t);
        self.replace_whole(&t, email_re(), Category::Email)
    }

    fn placeholder(&mut self, cat: Category, matched: &str) -> String {
        let key = (cat, matched.to_string());
        if let Some(&n) = self.seen.get(&key) {
            return format!("[{}_{n}]", cat.tag());
        }
        let n = self.next.entry(cat).or_insert(0);
        *n += 1;
        let idx = *n;
        self.seen.insert(key, idx);
        format!("[{}_{idx}]", cat.tag())
    }

    fn replace_whole(&mut self, text: &str, re: &Regex, cat: Category) -> String {
        re.replace_all(text, |caps: &Captures| self.placeholder(cat, &caps[0]))
            .into_owned()
    }

    fn replace_prefixed(&mut self, text: &str, re: &Regex, cat: Category) -> String {
        re.replace_all(text, |caps: &Captures| {
            let value = &caps[2];
            if value.starts_with('[') && value.ends_with(']') {
                return caps[0].to_string();
            }
            format!("{}{}", &caps[1], self.placeholder(cat, value))
        })
        .into_owned()
    }

    fn redact_cards(&mut self, text: &str) -> String {
        let re = card_re();
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for m in re.find_iter(text) {
            out.push_str(&text[last..m.start()]);
            let digits: String = m.as_str().chars().filter(char::is_ascii_digit).collect();
            if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
                out.push_str(&self.placeholder(Category::Card, m.as_str()));
            } else {
                out.push_str(m.as_str());
            }
            last = m.end();
        }
        out.push_str(&text[last..]);
        out
    }
}

fn luhn_valid(digits: &str) -> bool {
    let mut sum = 0_u32;
    for (idx, ch) in digits.chars().rev().enumerate() {
        let mut d = match ch.to_digit(10) {
            Some(v) => v,
            None => return false,
        };
        if idx % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum.is_multiple_of(10)
}

fn private_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----")
            .unwrap()
    })
}

fn token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:sk-[A-Za-z0-9_\-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|AIza[A-Za-z0-9_\-]{30,}|nm_[A-Za-z0-9_\-]{12,}|xox[baprs]-[A-Za-z0-9\-]{16,})\b")
            .unwrap()
    })
}

fn jwt_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\beyJ[A-Za-z0-9_\-]{10,}\.eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\b")
            .unwrap()
    })
}

fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(Bearer\s+)([A-Za-z0-9._\-]{16,})").unwrap())
}

fn env_secret_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)((?:API_KEY|SECRET|PASSWORD|TOKEN|PASSWD)\s*[=:]\s*)([^\s'\x22]{8,})")
            .unwrap()
    })
}

fn card_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:\d[ -]*?){13,19}\b").unwrap())
}

fn email_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_tokens_and_env_secrets_deterministically() {
        let mut r = LocalRedactor::new();
        let input = "export NEXUSMIND_API_KEY=nm_live_1234567890abcdef and sk-ant-api03-abcdef1234567890 for user@example.com";
        let out = r.redact(input);
        assert!(!out.contains("nm_live_1234567890abcdef"));
        assert!(!out.contains("sk-ant-api03-abcdef1234567890"));
        assert!(!out.contains("user@example.com"));
        assert!(out.contains("[SECRET_1]"));
        assert!(out.contains("[EMAIL_1]"));
    }
}
