//! Redaction v1 (plan §6.3).
//!
//! Runs on acquired text before it enters a session buffer, on model output before storage or
//! display, and on OCR lines for VLM masking. Each detector returns byte spans; overlapping spans
//! are merged; each span becomes `[REDACTED:<kind>]`. Only per-kind counts leave this module —
//! the matched values are never returned, stored or logged.
//!
//! Numbers that could be anything (16 digits, 12 digits) are only redacted when a checksum agrees:
//! Luhn plus a card-network prefix for cards, Verhoeff for Aadhaar. Keys and tokens are matched by
//! their published prefixes; unprefixed secrets only next to words like "key", "token", "secret".
//! Phone numbers and e-mail addresses are kept by default (people resolution needs them) and can be
//! redacted with [`Options`].

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretKind {
    PrivateKey,
    Jwt,
    ApiKey,
    Card,
    Aadhaar,
    Pan,
    /// Passport or voter ID (EPIC) number.
    GovId,
    BankAccount,
    Ifsc,
    Upi,
    Otp,
    Password,
    GenericSecret,
    Phone,
    Email,
    Person,
    Address,
    DateOfBirth,
}

impl SecretKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SecretKind::PrivateKey => "private_key",
            SecretKind::Jwt => "jwt",
            SecretKind::ApiKey => "api_key",
            SecretKind::Card => "card",
            SecretKind::Aadhaar => "aadhaar",
            SecretKind::Pan => "pan",
            SecretKind::GovId => "gov_id",
            SecretKind::BankAccount => "account",
            SecretKind::Ifsc => "ifsc",
            SecretKind::Upi => "upi",
            SecretKind::Otp => "otp",
            SecretKind::Password => "password",
            SecretKind::GenericSecret => "secret",
            SecretKind::Phone => "phone",
            SecretKind::Email => "email",
            SecretKind::Person => "person",
            SecretKind::Address => "address",
            SecretKind::DateOfBirth => "dob",
        }
    }
}

/// Policy version stored with every redacted output (facts store `redact_ver`). Bump when the
/// detectors change so stored rows can be re-checked.
/// v3: Robust PII masking (names with pseudonyms, addresses, DOB, typed token formats).
pub const REDACT_VER: &str = "v3";

/// Token formatting style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TokenFormat {
    /// Classic prefix tokens, e.g. `[REDACTED:card]` (default for backwards compatibility).
    #[default]
    RedactedPrefix,
    /// Typed standard placeholders, e.g. `[PERSON_01]`, `[EMAIL]`, `[PHONE]`, `[CARD]`.
    TypedPlaceholder,
}

/// Which optional PII categories to redact. Secrets are always redacted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub redact_phone: bool,
    pub redact_email: bool,
    pub redact_names: bool,
    pub redact_addresses: bool,
    pub redact_dob: bool,
    pub token_format: TokenFormat,
}

impl Options {
    /// Full PII masking covering all supported privacy categories with typed placeholders.
    pub fn full_pii() -> Self {
        Options {
            redact_phone: true,
            redact_email: true,
            redact_names: true,
            redact_addresses: true,
            redact_dob: true,
            token_format: TokenFormat::TypedPlaceholder,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: SecretKind,
}

/// Redacted text plus how many of each kind were removed. `Debug` shows counts only.
#[derive(Clone, PartialEq, Eq)]
pub struct Redacted {
    pub text: String,
    pub counts: BTreeMap<SecretKind, u32>,
}

impl Redacted {
    pub fn total(&self) -> u32 {
        self.counts.values().sum()
    }
}

impl std::fmt::Debug for Redacted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redacted")
            .field("chars", &self.text.chars().count())
            .field("counts", &self.counts)
            .finish()
    }
}

fn re(p: &str) -> Regex {
    Regex::new(p).expect("redaction regex")
}

static PEM: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?(?:-----END [A-Z0-9 ]*PRIVATE KEY-----|\z)"));
static JWT: LazyLock<Regex> = LazyLock::new(|| re(r"\beyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}"));
static API_KEYS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",                    // AWS access key id
        r"\bgh[pousr]_[A-Za-z0-9]{36,255}\b",                // GitHub tokens
        r"\bgithub_pat_[A-Za-z0-9_]{22,255}\b",              // GitHub fine-grained PAT
        r"\bxox[abprs]-[A-Za-z0-9-]{10,}",                   // Slack
        r"\bsk-(?:ant-[a-z0-9]+-|proj-)?[A-Za-z0-9_-]{20,}", // OpenAI / Anthropic style
        r"\bAIza[0-9A-Za-z_-]{35}",                          // Google API key
        r"\b(?:sk|rk)_(?:live|test)_[0-9A-Za-z]{16,}",       // Stripe secret / restricted
        r"\bglpat-[0-9A-Za-z_-]{20,}",                       // GitLab PAT
        r"\bnpm_[A-Za-z0-9]{36}\b",                          // npm
    ]
    .iter()
    .map(|p| re(p))
    .collect()
});
static GENERIC: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)\b(?:api[_-]?key|access[_-]?key|secret[_-]?key|client[_-]?secret|secret|token|bearer|authorization|private[_-]?key)\b[^\S\n]*(?:[:=]|=>|is)?[^\S\n]*["']?([A-Za-z0-9+/_\-.=]{20,})"#,
    )
});
static PASSWORD: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)(?:\b(?:password|passwd|pwd|pass ?code|pin)\b|पासवर्ड)[^\S\n]*(?:[:=]|is|hai|h)[^\S\n]*["']?([^\s"']{3,64})"#,
    )
});
static OTP_AFTER: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(?:\b(?:otp|one[- ]time (?:password|code|pin)|verification code|security code|login code|sign[- ]in code|auth(?:entication)? code|code is|code:)|ओटीपी|सत्यापन कोड)\D{0,40}?\b(\d{4,8})\b",
    )
});
static OTP_BEFORE: LazyLock<Regex> = LazyLock::new(|| {
    // Up to two words between, e.g. "is your Google verification code".
    re(
        r"(?i)\b(\d{4,8})\b\s*(?:is|hai|as)\s+(?:your|ur|the|aapka|tumhara)\s+(?:[\p{L}\d]+\s+){0,2}(?:otp|code|verification|one[- ]time|login|sign[- ]in)",
    )
});
static AADHAAR: LazyLock<Regex> = LazyLock::new(|| re(r"\b[2-9]\d{3}[ -]?\d{4}[ -]?\d{4}\b"));
static AADHAAR_WORD: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(?:aadhaa?r|uid|uidai|e-?kyc)\b|आधार"));
static CARD: LazyLock<Regex> = LazyLock::new(|| re(r"\b\d(?:[ -]?\d){12,18}\b"));
/// Aadhaar Virtual ID: 16 digits (Verhoeff), only after a VID keyword.
static AADHAAR_VID: LazyLock<Regex> = LazyLock::new(|| re(r"\b[2-9]\d{3}[ -]?\d{4}[ -]?\d{4}[ -]?\d{4}\b"));
static VID_WORD: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(?:vid|virtual\s+id)\b"));
/// Indian passport (letter + 7 digits) and voter ID / EPIC (3 letters + 7 digits), only right
/// after their keyword: on their own they look like product codes and ticket ids.
static PASSPORT: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(?:passport|pp)\b[^\S\n]*(?:no\.?|number|num)?[^\S\n]*[:#\-]?[^\S\n]*([A-Z][1-9]\d{6})\b")
});
static VOTER_ID: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:voter[^\S\n]*id|epic|election[^\S\n]*card)\b(?:[^\S\n]*card)?[^\S\n]*(?:no\.?|number|num)?[^\S\n]*[:#\-]?[^\S\n]*([A-Z]{3}\d{7})\b",
    )
});
static PAN: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Z]{3}[PCHFATBLJG][A-Z]\d{4}[A-Z]\b"));
static IFSC: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Z]{4}0[A-Z0-9]{6}\b"));
static ACCOUNT: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:a/c|acct|account|acc|khata)\.?[^\S\n]*(?:no\.?|number|num|#|sankhya)?[^\S\n]*[:\-]?[^\S\n]*(\d(?:[ -]?\d){8,17})\b",
    )
});
static UPI: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b[a-z0-9][a-z0-9._-]{1,255}@(?:okaxis|okhdfcbank|okicici|oksbi|ybl|ibl|axl|paytm|ptyes|ptaxis|pthdfc|ptsbi|upi|apl|yapl|axisbank|axisb|icici|sbi|hdfcbank|kotak|kmbl|yesbank|idfcbank|idfcfirst|fbl|federal|airtel|freecharge|jupiteraxis|slc|indus|pnb|barodampay|boi|cnrb|ibkl|rbl|aubank|dbs|hsbc|citi|abfspay|waaxis|wahdfcbank|waicici|wasbi|naviaxis|superyes|ikwik|mbk|postbank)\b",
    )
});
static PHONE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?:\+91[\s-]?|\b0)?\b[6-9]\d{4}[\s-]?\d{5}\b|\+\d{1,3}[\s-]?(?:\(\d{1,4}\)[\s-]?)?\d{2,4}[\s-]?\d{3,4}(?:[\s-]?\d{3,4})?\b")
});
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}\b"));
static NAME_HONORIFIC: LazyLock<Regex> = LazyLock::new(|| {
    re(r"\b(?:Mr\.?|Mrs\.?|Ms\.?|Miss|Dr\.?|Prof\.?|Shri|Smt\.?|Sir|Madam)\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,2})\b")
});
static NAME_CONTEXT: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i:\b(?:my\s+name\s+is|contact|meet\s+with|attended\s+by|assigned\s+to|reported\s+by|name\s*[:=]))\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,2})\b")
});
static NAME_GREETING: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i:\b(?:Dear|Hi|Hello))\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]+)?)\b")
});
static ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(?:address\s*[:=]|lives?\s+at|residing\s+at)\s*([0-9A-Za-z\s,.-]{5,60}?(?:Street|St\.?|Avenue|Ave\.?|Road|Rd\.?|Lane|Ln\.?|Boulevard|Blvd\.?|Drive|Dr\.?|Marg|Nagar|Sector|Block|Apartment|Apt\.?|Suite|Ste\.?|Floor|Building|Bldg\.?)(?:\s*,?\s*[A-Za-z\s]+,?\s*(?:\b\d{6}\b|\b\d{5}(?:-\d{4})?\b))?)\b")
});
static POSTAL_CODE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(?:pin\s*code|pincode|postal\s*code|zip\s*code)\s*[:#-]?\s*(\d{6}|\d{5}(?:-\d{4})?)\b")
});
static DOB: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(?:dob|date\s+of\s+birth|born\s+(?:on)?|d\.o\.b\.?|birth\s*date)\b[^\S\n]*(?:[:=]|is)?\s*(\d{1,2}[-/. ]\d{1,2}[-/. ]\d{2,4}|\d{4}[-/. ]\d{1,2}[-/. ]\d{1,2}|\d{1,2}\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{2,4})\b")
});
static IBAN: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(?:iban\s*[:#-]?)?\s*([A-Z]{2}\d{2}[A-Z0-9]{4}\d{7}(?:[A-Z0-9]{1,16})?)\b")
});

fn digits(s: &str) -> Vec<u8> {
    s.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect()
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn luhn_ok(d: &[u8]) -> bool {
    let sum: u32 = d
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &x)| {
            let x = x as u32;
            if i % 2 == 1 {
                if x * 2 > 9 { x * 2 - 9 } else { x * 2 }
            } else {
                x
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Card-network prefix and length sanity (Visa, Mastercard, Amex, Diners, Discover, JCB,
/// UnionPay, RuPay, Maestro).
fn card_prefix_ok(d: &[u8]) -> bool {
    let n = d.len();
    let p = |k: usize| d.iter().take(k).fold(0u32, |a, &x| a * 10 + x as u32);
    match d.first() {
        Some(4) => matches!(n, 13 | 16 | 19),
        Some(5) => (51..=55).contains(&p(2)) && n == 16 || p(2) == 50 || (56..=58).contains(&p(2)),
        Some(2) => (2221..=2720).contains(&p(4)) && n == 16,
        Some(3) => {
            (matches!(p(2), 34 | 37) && n == 15)
                || matches!(p(2), 36 | 38 | 39)
                || (300..=305).contains(&p(3))
                || (3528..=3589).contains(&p(4))
        }
        Some(6) => matches!(p(2), 60 | 62 | 64 | 65) || p(4) == 6011 || (63..=69).contains(&p(2)),
        Some(8) => matches!(p(2), 81 | 82) && n == 16,
        _ => false,
    }
}

const VD: [[u8; 10]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    [1, 2, 3, 4, 0, 6, 7, 8, 9, 5],
    [2, 3, 4, 0, 1, 7, 8, 9, 5, 6],
    [3, 4, 0, 1, 2, 8, 9, 5, 6, 7],
    [4, 0, 1, 2, 3, 9, 5, 6, 7, 8],
    [5, 9, 8, 7, 6, 0, 4, 3, 2, 1],
    [6, 5, 9, 8, 7, 1, 0, 4, 3, 2],
    [7, 6, 5, 9, 8, 2, 1, 0, 4, 3],
    [8, 7, 6, 5, 9, 3, 2, 1, 0, 4],
    [9, 8, 7, 6, 5, 4, 3, 2, 1, 0],
];
const VP: [[u8; 10]; 8] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    [1, 5, 7, 6, 2, 8, 3, 0, 9, 4],
    [5, 8, 0, 3, 7, 9, 6, 1, 4, 2],
    [8, 9, 1, 6, 0, 4, 3, 5, 2, 7],
    [9, 4, 5, 3, 1, 2, 6, 8, 7, 0],
    [4, 2, 8, 6, 5, 7, 3, 9, 0, 1],
    [2, 7, 9, 3, 8, 0, 6, 4, 1, 5],
    [7, 0, 4, 6, 9, 1, 3, 2, 5, 8],
];

/// Verhoeff checksum (Aadhaar's check digit).
pub fn verhoeff_ok(d: &[u8]) -> bool {
    let mut c = 0u8;
    for (i, &x) in d.iter().rev().enumerate() {
        c = VD[c as usize][VP[i % 8][x as usize] as usize];
    }
    c == 0
}

/// Shannon entropy in bits per character.
pub fn entropy(s: &str) -> f64 {
    let mut counts = [0u32; 256];
    for b in s.bytes() {
        counts[b as usize] += 1;
    }
    let n = s.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| -(c as f64 / n) * (c as f64 / n).log2())
        .sum()
}

fn looks_like_secret(s: &str) -> bool {
    let has_digit = s.bytes().any(|b| b.is_ascii_digit());
    let has_alpha = s.bytes().any(|b| b.is_ascii_alphabetic());
    // Hex keys top out at 4.0 bits/char (a 32-char hex token scores ~3.6), so the plan's 4.0
    // threshold would miss them; mixed letters+digits and >= 3.0 bits/char instead.
    has_digit && has_alpha && entropy(s) >= 3.0
}

/// All secret spans in `text`, merged and sorted.
pub fn find(text: &str, opts: Options) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut push = |start: usize, end: usize, kind: SecretKind| spans.push(Span { start, end, kind });

    for m in PEM.find_iter(text) {
        push(m.start(), m.end(), SecretKind::PrivateKey);
    }
    for m in JWT.find_iter(text) {
        push(m.start(), m.end(), SecretKind::Jwt);
    }
    for r in API_KEYS.iter() {
        for m in r.find_iter(text) {
            push(m.start(), m.end(), SecretKind::ApiKey);
        }
    }
    for c in GENERIC.captures_iter(text) {
        let g = c.get(1).unwrap();
        if looks_like_secret(g.as_str()) {
            push(g.start(), g.end(), SecretKind::GenericSecret);
        }
    }
    for c in PASSWORD.captures_iter(text) {
        let g = c.get(1).unwrap();
        let v = g.as_str();
        // Placeholders and already-redacted values are not passwords.
        if !v.starts_with("[REDACTED") && !v.chars().all(|ch| matches!(ch, '*' | '•' | '●' | '.' | 'x' | 'X')) {
            push(g.start(), g.end(), SecretKind::Password);
        }
    }
    for r in [&*OTP_AFTER, &*OTP_BEFORE] {
        for c in r.captures_iter(text) {
            let g = c.get(1).unwrap();
            push(g.start(), g.end(), SecretKind::Otp);
        }
    }
    for c in ACCOUNT.captures_iter(text) {
        let g = c.get(1).unwrap();
        push(g.start(), g.end(), SecretKind::BankAccount);
    }
    for m in CARD.find_iter(text) {
        let d = digits(m.as_str());
        if (13..=19).contains(&d.len()) && luhn_ok(&d) && card_prefix_ok(&d) {
            push(m.start(), m.end(), SecretKind::Card);
        }
    }
    for m in AADHAAR.find_iter(text) {
        let d = digits(m.as_str());
        // One random 12-digit number in ten passes Verhoeff, and order/tracking ids are often
        // 12 digits. The 4-4-4 grouping is Aadhaar's own format; a bare run of 12 digits also
        // needs an Aadhaar keyword within 40 bytes before it.
        let grouped = m.as_str().len() > 12;
        let context = &text[floor_char(text, m.start().saturating_sub(40))..m.start()];
        if d.len() == 12 && verhoeff_ok(&d) && (grouped || AADHAAR_WORD.is_match(context)) {
            push(m.start(), m.end(), SecretKind::Aadhaar);
        }
    }
    for m in AADHAAR_VID.find_iter(text) {
        let d = digits(m.as_str());
        let context = &text[floor_char(text, m.start().saturating_sub(40))..m.start()];
        if d.len() == 16 && verhoeff_ok(&d) && VID_WORD.is_match(context) {
            push(m.start(), m.end(), SecretKind::Aadhaar);
        }
    }
    for c in PASSPORT.captures_iter(text) {
        if let Some(v) = c.get(1) {
            push(v.start(), v.end(), SecretKind::GovId);
        }
    }
    for c in VOTER_ID.captures_iter(text) {
        if let Some(v) = c.get(1) {
            push(v.start(), v.end(), SecretKind::GovId);
        }
    }
    for m in PAN.find_iter(text) {
        push(m.start(), m.end(), SecretKind::Pan);
    }
    for m in IFSC.find_iter(text) {
        push(m.start(), m.end(), SecretKind::Ifsc);
    }
    for m in UPI.find_iter(text) {
        // `name@sbi.co.in` is an e-mail address, not a UPI ID.
        let rest = &text[m.end()..];
        if !(rest.starts_with('.') && rest[1..].starts_with(|c: char| c.is_ascii_alphanumeric())) {
            push(m.start(), m.end(), SecretKind::Upi);
        }
    }
    if opts.redact_email {
        for m in EMAIL.find_iter(text) {
            push(m.start(), m.end(), SecretKind::Email);
        }
    }
    if opts.redact_phone {
        for m in PHONE.find_iter(text) {
            push(m.start(), m.end(), SecretKind::Phone);
        }
    }
    if opts.redact_names {
        let mut discovered_names = std::collections::BTreeSet::new();
        for c in NAME_HONORIFIC.captures_iter(text) {
            if let Some(m) = c.get(1) {
                push(m.start(), m.end(), SecretKind::Person);
                discovered_names.insert(m.as_str());
            }
        }
        for c in NAME_CONTEXT.captures_iter(text) {
            if let Some(m) = c.get(1) {
                push(m.start(), m.end(), SecretKind::Person);
                discovered_names.insert(m.as_str());
            }
        }
        for c in NAME_GREETING.captures_iter(text) {
            if let Some(m) = c.get(1) {
                push(m.start(), m.end(), SecretKind::Person);
                discovered_names.insert(m.as_str());
            }
        }
        for name in discovered_names {
            if let Ok(r) = Regex::new(&format!(r"\b{}\b", regex::escape(name))) {
                for m in r.find_iter(text) {
                    push(m.start(), m.end(), SecretKind::Person);
                }
            }
        }
    }
    if opts.redact_addresses {
        for c in ADDRESS.captures_iter(text) {
            if let Some(m) = c.get(1) {
                push(m.start(), m.end(), SecretKind::Address);
            }
        }
        for m in POSTAL_CODE.find_iter(text) {
            push(m.start(), m.end(), SecretKind::Address);
        }
    }
    if opts.redact_dob {
        for c in DOB.captures_iter(text) {
            if let Some(m) = c.get(1) {
                push(m.start(), m.end(), SecretKind::DateOfBirth);
            }
        }
    }
    for c in IBAN.captures_iter(text) {
        if let Some(m) = c.get(1) {
            let digits_only: Vec<u8> = digits(m.as_str());
            if digits_only.len() >= 10 {
                push(m.start(), m.end(), SecretKind::BankAccount);
            }
        }
    }

    // Merge overlaps; the earlier-listed (more specific) kind wins.
    spans.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end), s.kind));
    let mut merged: Vec<Span> = Vec::new();
    for s in spans {
        match merged.last_mut() {
            Some(last) if s.start < last.end => {
                last.end = last.end.max(s.end);
                last.kind = last.kind.min(s.kind);
            }
            _ => merged.push(s),
        }
    }
    merged
}

/// Convenience function applying full PII detection and masking with typed standard tokens.
pub fn mask_pii(text: &str) -> Redacted {
    redact(text, Options::full_pii())
}

pub fn redact(text: &str, opts: Options) -> Redacted {
    // One pass can reveal another secret: replacing the first of two codes after "OTP" brings
    // the second into the keyword's window (found by tests/robustness.rs). Repeat until
    // nothing is found; each pass only shrinks secret-like spans, so a few passes suffice.
    let mut r = redact_once(text, opts);
    for _ in 0..4 {
        if find(&r.text, opts).is_empty() {
            break;
        }
        let next = redact_once(&r.text, opts);
        for (k, n) in next.counts {
            *r.counts.entry(k).or_insert(0) += n;
        }
        r.text = next.text;
    }
    r
}

fn redact_once(text: &str, opts: Options) -> Redacted {
    let spans = find(text, opts);
    let mut out = String::with_capacity(text.len());
    let mut counts = BTreeMap::new();
    let mut pos = 0;
    let mut person_map: BTreeMap<String, usize> = BTreeMap::new();
    let mut next_person_id = 1usize;

    for s in &spans {
        out.push_str(&text[pos..s.start]);
        let raw_val = &text[s.start..s.end];
        let placeholder = match opts.token_format {
            TokenFormat::TypedPlaceholder => match s.kind {
                SecretKind::Person => {
                    let norm = raw_val.trim().to_lowercase();
                    let id = *person_map.entry(norm).or_insert_with(|| {
                        let id = next_person_id;
                        next_person_id += 1;
                        id
                    });
                    format!("[PERSON_{:02}]", id)
                }
                SecretKind::Email => "[EMAIL]".to_string(),
                SecretKind::Phone => "[PHONE]".to_string(),
                SecretKind::Aadhaar => "[AADHAAR]".to_string(),
                SecretKind::Pan => "[PAN]".to_string(),
                SecretKind::Card => "[CARD]".to_string(),
                SecretKind::GovId => "[GOV_ID]".to_string(),
                SecretKind::BankAccount => "[BANK_ACCOUNT]".to_string(),
                SecretKind::Ifsc => "[IFSC]".to_string(),
                SecretKind::Upi => "[UPI]".to_string(),
                SecretKind::Otp => "[OTP]".to_string(),
                SecretKind::Password => "[PASSWORD]".to_string(),
                SecretKind::ApiKey => "[API_KEY]".to_string(),
                SecretKind::PrivateKey => "[PRIVATE_KEY]".to_string(),
                SecretKind::Jwt => "[JWT]".to_string(),
                SecretKind::GenericSecret => "[SECRET]".to_string(),
                SecretKind::Address => "[ADDRESS]".to_string(),
                SecretKind::DateOfBirth => "[DATE_OF_BIRTH]".to_string(),
            },
            TokenFormat::RedactedPrefix => {
                if s.kind == SecretKind::Person {
                    let norm = raw_val.trim().to_lowercase();
                    let id = *person_map.entry(norm).or_insert_with(|| {
                        let id = next_person_id;
                        next_person_id += 1;
                        id
                    });
                    format!("[REDACTED:person_{:02}]", id)
                } else {
                    format!("[REDACTED:{}]", s.kind.as_str())
                }
            }
        };
        out.push_str(&placeholder);
        *counts.entry(s.kind).or_insert(0) += 1;
        pos = s.end;
    }
    out.push_str(&text[pos..]);
    Redacted { text: out, counts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(t: &str) -> String {
        redact(t, Options::default()).text
    }

    #[test]
    fn checksums() {
        assert!(luhn_ok(&digits("4111 1111 1111 1111")));
        assert!(!luhn_ok(&digits("4111 1111 1111 1112")));
        // Published Verhoeff examples: 236 -> check digit 3, 12345 -> check digit 1.
        assert!(verhoeff_ok(&digits("2363")));
        assert!(!verhoeff_ok(&digits("2364")));
        assert!(verhoeff_ok(&digits("123451")));
        assert!(!verhoeff_ok(&digits("123452")));
    }

    #[test]
    fn cards_need_luhn_and_a_network_prefix() {
        assert_eq!(
            r("Card 4111 1111 1111 1111 exp 12/29"),
            "Card [REDACTED:card] exp 12/29"
        );
        assert_eq!(
            r("order 4111 1111 1111 1112"),
            "order 4111 1111 1111 1112",
            "Luhn fails"
        );
        assert_eq!(
            r("ref 1111 1111 1111 1117"),
            "ref 1111 1111 1111 1117",
            "no network starts with 1"
        );
        assert_eq!(r("amex 3782-822463-10005"), "amex [REDACTED:card]");
    }

    #[test]
    fn otp_in_english_and_hinglish() {
        assert_eq!(
            r("Your OTP is 482913. Do not share."),
            "Your OTP is [REDACTED:otp]. Do not share."
        );
        assert_eq!(
            r("483920 is your verification code"),
            "[REDACTED:otp] is your verification code"
        );
        assert_eq!(
            r("OTP 7731 hai, kisi ko mat batana"),
            "OTP [REDACTED:otp] hai, kisi ko mat batana"
        );
        assert_eq!(
            r("OTP will arrive in 30 seconds"),
            "OTP will arrive in 30 seconds",
            "2-digit numbers are not OTPs"
        );
    }

    #[test]
    fn upi_vs_email() {
        assert_eq!(r("pay asha.verma@okaxis today"), "pay [REDACTED:upi] today");
        assert_eq!(
            r("mail ravi@sbi.co.in"),
            "mail ravi@sbi.co.in",
            "email domain, not a UPI handle"
        );
        assert_eq!(
            redact(
                "mail ravi@example.com",
                Options {
                    redact_email: true,
                    ..Default::default()
                }
            )
            .text,
            "mail [REDACTED:email]"
        );
    }

    #[test]
    fn passwords_and_generic_secrets() {
        assert_eq!(r("password: Hunter2!x"), "password: [REDACTED:password]");
        assert_eq!(r("Password: ********"), "Password: ********");
        assert_eq!(r("token=9f86d081884c7d659a2feaa0c55ad015"), "token=[REDACTED:secret]");
        assert_eq!(
            r("the token bucket algorithm limits requests"),
            "the token bucket algorithm limits requests"
        );
    }

    /// Not a fuzzer (that is Stage 7), but a cheap guard: random mixes of digits, separators,
    /// keywords and multi-byte text must never panic and must redact idempotently.
    #[test]
    fn random_inputs_never_panic_and_are_idempotent() {
        let parts = [
            "4",
            "1",
            "0",
            " ",
            "-",
            "OTP ",
            "is your code ",
            "password: ",
            "token=",
            "आधार ",
            "ओटीपी ",
            "₹",
            "😀",
            "@okaxis",
            "AKIA",
            "eyJ",
            ".",
            "\n",
            "a/c no ",
            "abcdef",
            "Z",
            "हिन्दी",
            "x".repeat(25).leak(),
        ];
        let mut seed: u64 = 0x5eed;
        for _ in 0..3_000 {
            let mut s = String::new();
            for _ in 0..(seed % 40) {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                s.push_str(parts[(seed >> 33) as usize % parts.len()]);
            }
            let once = r(&s);
            assert_eq!(r(&once), once, "not idempotent for {s:?}");
        }
    }

    #[test]
    fn redaction_is_idempotent_and_debug_hides_text() {
        let once = r("OTP 482913, card 4111111111111111, key AKIAIOSFODNN7EXAMPLE");
        assert_eq!(r(&once), once);
        let d = format!("{:?}", redact("password: Hunter2!x", Options::default()));
        assert!(!d.contains("Hunter2"), "{d}");
    }

    #[test]
    fn full_pii_masking_and_pseudonyms() {
        let text = "Please contact Rahul at rahul@example.com before the meeting. If Rahul is busy, meet with Priya at priya@corp.net.";
        let res = mask_pii(text);
        assert_eq!(
            res.text,
            "Please contact [PERSON_01] at [EMAIL] before the meeting. If [PERSON_01] is busy, meet with [PERSON_02] at [EMAIL]."
        );
    }

    #[test]
    fn pii_dob_address_and_phone() {
        let text = "Patient Dr. Ramesh Gupta, DOB: 14/07/1985, phone +91-9876543210, address: Flat 402 Sector 15 Nagar, pincode: 560001";
        let res = mask_pii(text);
        assert!(res.text.contains("[PERSON_01]"));
        assert!(res.text.contains("[DATE_OF_BIRTH]"));
        assert!(res.text.contains("[PHONE]"));
        assert!(res.text.contains("[ADDRESS]"));
        assert!(!res.text.contains("Ramesh"));
        assert!(!res.text.contains("14/07/1985"));
        assert!(!res.text.contains("9876543210"));
    }

    #[test]
    fn typed_token_formats() {
        let text = "Call +1-555-123-4567, email user@domain.com, card 4111 1111 1111 1111, password: Secret123Password";
        let res = mask_pii(text);
        assert!(res.text.contains("[PHONE]"));
        assert!(res.text.contains("[EMAIL]"));
        assert!(res.text.contains("[CARD]"));
        assert!(res.text.contains("[PASSWORD]"));
    }
}
