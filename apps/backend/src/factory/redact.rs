//! PII redaction for intake text (plan D13, OD-7).
//!
//! Every Gmail, Notion, Drive/Meet transcript, local `.txt` or admin-uploaded
//! transcript item is untrusted, possibly-sensitive text. Before any of it
//! becomes a `TaskSpec` (and therefore before it can ever reach a model), this
//! module strips emails, phone numbers, government ID numbers, payment cards,
//! bank accounts, physical addresses and secrets, replacing each with a
//! numbered placeholder (`[EMAIL_1]`, `[PHONE_1]`, ...). Person names are kept:
//! the factory still needs to know who a task is about.
//!
//! Pure and deterministic: the same text always redacts the same way, and two
//! occurrences of the same value within one call get the same number. Built
//! conservatively — every pattern favors missing an edge case over flagging
//! version numbers, dates, SHAs or UUIDs as PII.

use regex::{Captures, Regex};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Category {
    Secret,
    Card,
    Account,
    Email,
    Id,
    Phone,
    Address,
}

impl Category {
    fn tag(self) -> &'static str {
        match self {
            Category::Secret => "SECRET",
            Category::Card => "CARD",
            Category::Account => "ACCOUNT",
            Category::Email => "EMAIL",
            Category::Id => "ID",
            Category::Phone => "PHONE",
            Category::Address => "ADDRESS",
        }
    }
}

/// Redacts one or more pieces of text, keeping numbering consistent across all
/// of them (the same email in a title and a description gets the same
/// `[EMAIL_n]`). Create one `Redactor` per item.
#[derive(Default)]
pub struct Redactor {
    seen: HashMap<(Category, String), usize>,
    next: HashMap<Category, usize>,
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    fn placeholder(&mut self, category: Category, matched: &str) -> String {
        let key = (category, matched.to_string());
        if let Some(&n) = self.seen.get(&key) {
            return format!("[{}_{n}]", category.tag());
        }
        let counter = self.next.entry(category).or_insert(0);
        *counter += 1;
        let n = *counter;
        self.seen.insert(key, n);
        format!("[{}_{n}]", category.tag())
    }

    /// Redacts one piece of text. Call order matters: secrets and payment
    /// cards are claimed before the looser account/phone/address patterns can
    /// see them, so a card number never also becomes `[ACCOUNT_n]`.
    pub fn apply(&mut self, text: &str) -> String {
        let text = self.redact_secrets(text);
        let text = self.redact_cards(&text);
        let text = self.redact_accounts(&text);
        let text = self.redact_emails(&text);
        let text = self.redact_ids(&text);
        let text = self.redact_phones(&text);
        self.redact_addresses(&text)
    }

    // ------------------------------------------------------------ secrets

    fn redact_secrets(&mut self, text: &str) -> String {
        let text = self.replace_whole(text, private_key_regex(), Category::Secret);
        let text = self.replace_whole(&text, token_shape_regex(), Category::Secret);
        let text = self.replace_whole(&text, jwt_regex(), Category::Secret);
        let text = self.replace_prefixed(&text, bearer_regex(), Category::Secret);
        self.replace_prefixed(&text, secret_assignment_regex(), Category::Secret)
    }

    // ------------------------------------------------------------ cards

    /// A Luhn-valid run of 13-19 digits (optionally grouped with spaces or
    /// dashes). Luhn avoids flagging an arbitrary long number — a database id,
    /// a phone number — as a card.
    fn redact_cards(&mut self, text: &str) -> String {
        let re = card_regex();
        let mut result = String::with_capacity(text.len());
        let mut last = 0;
        for m in re.find_iter(text) {
            result.push_str(&text[last..m.start()]);
            let digits: String = m.as_str().chars().filter(char::is_ascii_digit).collect();
            if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
                result.push_str(&self.placeholder(Category::Card, m.as_str()));
            } else {
                result.push_str(m.as_str());
            }
            last = m.end();
        }
        result.push_str(&text[last..]);
        result
    }

    // ------------------------------------------------------------ accounts

    fn redact_accounts(&mut self, text: &str) -> String {
        let text = self.replace_whole(text, iban_regex(), Category::Account);
        self.replace_prefixed(&text, labelled_account_regex(), Category::Account)
    }

    // ------------------------------------------------------------ emails

    fn redact_emails(&mut self, text: &str) -> String {
        self.replace_whole(text, email_regex(), Category::Email)
    }

    // ------------------------------------------------------------ ids

    fn redact_ids(&mut self, text: &str) -> String {
        let text = self.replace_prefixed(text, cedula_regex(), Category::Id);
        let text = self.replace_prefixed(&text, nit_labelled_regex(), Category::Id);
        let text = self.replace_whole(&text, nit_unlabelled_regex(), Category::Id);
        self.replace_prefixed(&text, passport_regex(), Category::Id)
    }

    // ------------------------------------------------------------ phones

    fn redact_phones(&mut self, text: &str) -> String {
        let text = self.replace_whole(text, phone_intl_regex(), Category::Phone);
        let text = self.replace_whole(&text, phone_co_mobile_regex(), Category::Phone);
        self.replace_whole(&text, phone_co_landline_regex(), Category::Phone)
    }

    // ------------------------------------------------------------ addresses

    fn redact_addresses(&mut self, text: &str) -> String {
        let text = self.replace_whole(text, address_es_regex(), Category::Address);
        self.replace_whole(&text, address_en_regex(), Category::Address)
    }

    // ------------------------------------------------------------ helpers

    /// Replaces every whole match with its placeholder.
    fn replace_whole(&mut self, text: &str, re: &Regex, category: Category) -> String {
        let mut result = String::with_capacity(text.len());
        let mut last = 0;
        for m in re.find_iter(text) {
            result.push_str(&text[last..m.start()]);
            result.push_str(&self.placeholder(category, m.as_str()));
            last = m.end();
        }
        result.push_str(&text[last..]);
        result
    }

    /// Replaces only capture group 2 (the value) with its placeholder, keeping
    /// group 1 (a label such as "cuenta:" or "Bearer ") and an optional group 3
    /// (a trailing quote) exactly as written.
    fn replace_prefixed(&mut self, text: &str, re: &Regex, category: Category) -> String {
        re.replace_all(text, |caps: &Captures<'_>| {
            let prefix = &caps[1];
            let value = &caps[2];
            let suffix = caps.get(3).map_or("", |m| m.as_str());
            format!("{prefix}{}{suffix}", self.placeholder(category, value))
        })
        .into_owned()
    }
}

/// Redacts one piece of text on its own; prefer [`Redactor`] when a title and a
/// description from the same item must share numbering.
pub fn redact(text: &str) -> String {
    Redactor::new().apply(text)
}

fn luhn_valid(digits: &str) -> bool {
    let sum: u32 = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| {
            let d = u32::from(b - b'0');
            if i % 2 == 1 {
                let doubled = d * 2;
                if doubled > 9 {
                    doubled - 9
                } else {
                    doubled
                }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

fn compiled(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("redaction pattern compiles"))
}

macro_rules! regex_fn {
    ($name:ident, $pattern:expr) => {
        fn $name() -> &'static Regex {
            static CELL: OnceLock<Regex> = OnceLock::new();
            compiled(&CELL, $pattern)
        }
    };
}

// ---------------------------------------------------------------- secrets

regex_fn!(
    private_key_regex,
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----"
);
regex_fn!(
    token_shape_regex,
    r"\b(?:ghp|gho|ghs|ghr|github_pat|xox[abprs]|AKIA)[A-Za-z0-9_-]{10,}\b"
);
regex_fn!(
    jwt_regex,
    r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b"
);
regex_fn!(bearer_regex, r"(?i)(Bearer\s+)([A-Za-z0-9\-_.=]{10,})()");
regex_fn!(
    secret_assignment_regex,
    r#"(?i)\b([a-z0-9_.-]*(?:password|passwd|secret|api[_-]?key|token)[a-z0-9_.-]*\s*[:=]\s*['"]?)([A-Za-z0-9_\-./+]{8,})(['"]?)"#
);

// ---------------------------------------------------------------- cards

// First digit plus 12-18 more, each optionally preceded by one space or dash:
// 13-19 digits total. Luhn (above) decides whether it is actually redacted.
regex_fn!(card_regex, r"\b\d(?:[ -]?\d){12,18}\b");

// ---------------------------------------------------------------- accounts

regex_fn!(
    iban_regex,
    r"\b[A-Z]{2}\d{2}(?:[ ]?[A-Za-z0-9]{4}){2,6}(?:[ ]?[A-Za-z0-9]{1,3})?\b"
);
regex_fn!(
    labelled_account_regex,
    r"(?i)\b((?:cuenta|account)(?:\s*(?:no\.?|number|#))?\s*[:#]?\s*)(\d[\d\s-]{6,24}\d)()"
);

// ---------------------------------------------------------------- email

regex_fn!(
    email_regex,
    r"\b[A-Za-z0-9.+_-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"
);

// ---------------------------------------------------------------- ids

regex_fn!(
    cedula_regex,
    r"(?i)\b((?:c[eé]dula(?:\s*de\s*ciudadan[ií]a)?|c\.?c\.?)\s*[:#]?\s*)(\d{1,3}(?:[.,]\d{3}){1,3}|\d{6,10})()"
);
regex_fn!(
    nit_labelled_regex,
    r"(?i)\b(nit\s*[:#]?\s*)(\d{3}(?:[.,]\d{3}){1,2}-?\d)()"
);
regex_fn!(nit_unlabelled_regex, r"\b\d{3}\.\d{3}\.\d{3}-\d\b");
regex_fn!(
    passport_regex,
    r"(?i)\b((?:pasaporte|passport)(?:\s*(?:no\.?|number))?\s*[:#]?\s*)([A-Za-z0-9]{6,9})()"
);

// ---------------------------------------------------------------- phones

regex_fn!(
    phone_intl_regex,
    r"\+\d{1,3}[\s.-]?\(?\d{1,4}\)?(?:[\s.-]?\d{2,4}){2,4}"
);
regex_fn!(
    phone_co_mobile_regex,
    r"\b3\d{2}[\s.-]?\d{3}[\s.-]?\d{4}\b"
);
// Requires the parenthesized area code literally, unlike the mobile pattern's
// leading `3`: a bare leading digit here would also match the tail of any
// unrelated long digit run (nothing then stops it reusing those digits as a
// "phone"), which the `(` anchor rules out.
regex_fn!(
    phone_co_landline_regex,
    r"\(\d{1,3}\)[\s.-]?\d{3}[\s.-]?\d{4}\b"
);

// ---------------------------------------------------------------- addresses

regex_fn!(
    address_es_regex,
    r"(?i)\b(?:calle|cra\.?|carrera|cl\.?|av\.?|avenida|diag\.?|diagonal|transversal|tv\.?)\s+\d{1,4}[a-zA-Z]?(?:\s*bis)?(?:\s*#\s*\d{1,3}[a-zA-Z]?\s*-\s*\d{1,3})?"
);
// A trailing `\b` after an abbreviation's period would need a word character
// right after the period, which there never is ("St. for" has a space on both
// sides of the boundary check) — so the two forms are split: the spelled-out
// word requires `\b` (so "Streets" and "Streetlight" are not addresses), the
// abbreviation's period ends the match on its own.
regex_fn!(
    address_en_regex,
    r"(?i)\b\d{1,6}\s+[A-Za-z][A-Za-z']*(?:\s+[A-Za-z][A-Za-z']*){0,4}\s+(?:(?:Street|Avenue|Road|Boulevard)\b|(?:St|Ave|Rd|Blvd)\.)"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_redacted_and_numbered_consistently() {
        let text = "Contact ana@acme.test or ana@acme.test, also bob@acme.co.uk";
        let out = redact(text);
        assert_eq!(
            out,
            "Contact [EMAIL_1] or [EMAIL_1], also [EMAIL_2]"
        );
    }

    #[test]
    fn numbering_is_shared_across_title_and_description_of_one_item() {
        let mut r = Redactor::new();
        let title = r.apply("Refund for ana@acme.test");
        let description = r.apply("Email ana@acme.test again about this");
        assert_eq!(title, "Refund for [EMAIL_1]");
        assert_eq!(description, "Email [EMAIL_1] again about this");
    }

    #[test]
    fn phones_are_redacted_international_and_colombian() {
        assert_eq!(redact("Call +1 415 555 0132 now"), "Call [PHONE_1] now");
        assert_eq!(redact("Call +57 300 123 4567 now"), "Call [PHONE_1] now");
        assert_eq!(redact("Mi celular es 300 123 4567"), "Mi celular es [PHONE_1]");
        assert_eq!(redact("Fijo: (601) 345 6789"), "Fijo: [PHONE_1]");
    }

    #[test]
    fn colombian_ids_are_redacted_with_their_label_kept() {
        assert_eq!(redact("Cédula: 12.345.678"), "Cédula: [ID_1]");
        assert_eq!(redact("cedula 1023456789"), "cedula [ID_1]");
        assert_eq!(redact("NIT: 900.123.456-7"), "NIT: [ID_1]");
        assert_eq!(redact("RUT 900.123.456-7 adjunto"), "RUT [ID_1] adjunto");
        assert_eq!(redact("Pasaporte: AB123456"), "Pasaporte: [ID_1]");
    }

    #[test]
    fn payment_cards_are_luhn_validated() {
        // A known Luhn-valid test number.
        assert_eq!(redact("Card 4111 1111 1111 1111 expires soon"), "Card [CARD_1] expires soon");
        // Same digits shuffled to break Luhn: left alone (no false positive).
        let invalid = "Card 4111 1111 1111 1112 expires soon";
        assert_eq!(redact(invalid), invalid);
    }

    #[test]
    fn bank_accounts_need_an_iban_shape_or_an_explicit_label() {
        assert_eq!(
            redact("IBAN: DE89 3704 0044 0532 0130 00"),
            "IBAN: [ACCOUNT_1]"
        );
        assert_eq!(
            redact("cuenta: 1234567890123"),
            "cuenta: [ACCOUNT_1]"
        );
        assert_eq!(redact("account no. 00987654321"), "account no. [ACCOUNT_1]");
        // A bare long digit run with no label and no IBAN shape is left alone.
        let plain = "order id 1234567890123";
        assert_eq!(redact(plain), plain);
    }

    #[test]
    fn addresses_are_redacted_best_effort_in_spanish_and_english() {
        assert_eq!(redact("Vive en Calle 12 # 34-56, piso 2"), "Vive en [ADDRESS_1], piso 2");
        assert_eq!(redact("Oficina en Cra 7 # 10-20"), "Oficina en [ADDRESS_1]");
        assert_eq!(redact("Visit us at 123 Main Street today"), "Visit us at [ADDRESS_1] today");
        assert_eq!(redact("See 10 Downing St. for details"), "See [ADDRESS_1] for details");
    }

    #[test]
    fn secrets_tokens_and_keys_are_redacted() {
        assert_eq!(
            redact("token: ghp_abcdefghijklmnopqrstuvwxyz123456"),
            "token: [SECRET_1]"
        );
        assert_eq!(
            redact("Authorization: Bearer sk-abcdefghijklmnopqrstuvwxyz"),
            "Authorization: Bearer [SECRET_1]"
        );
        assert_eq!(
            redact("api_key = \"sh_live_abcdefghijklmnop\""),
            "api_key = \"[SECRET_1]\""
        );
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAK\n-----END RSA PRIVATE KEY-----";
        assert_eq!(redact(pem), "[SECRET_1]");
    }

    #[test]
    fn no_false_positives_on_code_like_text() {
        for clean in [
            "Upgrade to version 1.99.0 of the toolchain",
            "Released on 2026-10-05",
            "Commit fd3fb40a1b2c3d4e5f60718293a4b5c6d7e8f901",
            "Task id 3fa85f64-5717-4562-b3fc-2c963f66afa6",
            "See PR #309 for the risk floor",
            "Run `cargo test --lib -- redact`",
        ] {
            assert_eq!(redact(clean), clean, "{clean}");
        }
    }

    #[test]
    fn person_names_are_kept() {
        assert_eq!(
            redact("Ana Gomez reported the bug, contact ana@acme.test"),
            "Ana Gomez reported the bug, contact [EMAIL_1]"
        );
    }

    #[test]
    fn redaction_is_pure_and_deterministic() {
        let text = "ana@acme.test called 300 123 4567 about cedula 12.345.678";
        assert_eq!(redact(text), redact(text));
    }

    #[test]
    fn common_real_world_formats_are_redacted() {
        let text = "Escríbele a ana.perez+ventas@acme.co o llama al +57 300 123 4567 (o 300-123-4567). \
                    Cédula 1.023.456.789, NIT 900.123.456-7. Tarjeta 4111 1111 1111 1111. \
                    IBAN ES91 2100 0418 4502 0005 1332. Vive en Calle 12 # 34-56. token=ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        let out = redact(text);
        for leaked in [
            "ana.perez", "300 123 4567", "300-123-4567", "1.023.456.789", "900.123.456",
            "4111 1111", "ES91", "Calle 12 # 34-56", "ghp_abcdefghij",
        ] {
            assert!(!out.contains(leaked), "{leaked} leaked: {out}");
        }
        // Names and ordinary numbers stay.
        assert!(out.contains("Escríbele"));
        assert!(redact("Release v2.3.1 on 2026-10-05, PR #309, commit fd3fb40").contains("v2.3.1"));
    }
}
