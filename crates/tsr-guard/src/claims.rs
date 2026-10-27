//! Checkable claims in generated text (plan §10.4, §12.2): e-mail addresses, URLs, file names,
//! dates, times, weekdays, relative days, amounts, numbers and capitalised names.
//!
//! A claim is supported when its normalised form occurs in the given context. Normalisation makes
//! "28 Oct" match "28 October", "1,000,000" match "1000000" and "5 pm" match "5pm".
//! `[placeholders]` are never claims. Used by the draft fact check and by Ask's citation check.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Email,
    Url,
    File,
    Time,
    Date,
    Weekday,
    RelativeDay,
    Amount,
    Number,
    Name,
}

/// One claim not found in the context. Offsets are bytes into the checked text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub kind: ClaimKind,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

fn re(p: &str) -> Regex {
    Regex::new(p).expect("factcheck regex")
}

const MONTHS: &str = "jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?";

/// Ordered: earlier patterns win where spans overlap (a date is not also two numbers).
static PATTERNS: LazyLock<Vec<(ClaimKind, Regex)>> = LazyLock::new(|| {
    vec![
        (ClaimKind::Email, re(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+")),
        (ClaimKind::Url, re(r"(?i)\bhttps?://[^\s)\]]+")),
        (
            ClaimKind::File,
            re(
                r"(?i)\b[\w-]+(?:\.[\w-]+)*\.(?:xlsx|xls|csv|docx?|pdf|pptx?|txt|md|png|jpe?g|zip|json|ya?ml|log|py|rs|ts|js|fig|sketch)\b",
            ),
        ),
        (
            ClaimKind::Date,
            re(&format!(
                r"(?i)\b\d{{1,2}}(?:st|nd|rd|th)?\s+(?:{MONTHS})\b(?:\s+\d{{4}})?|\b(?:{MONTHS})\s+\d{{1,2}}(?:st|nd|rd|th)?\b(?:,?\s+\d{{4}})?|\b\d{{4}}-\d{{2}}-\d{{2}}\b|\b\d{{1,2}}/\d{{1,2}}(?:/\d{{2,4}})?\b"
            )),
        ),
        (
            ClaimKind::Time,
            re(r"(?i)\b\d{1,2}(?::\d{2})?\s?(?:am|pm)\b|\b\d{1,2}:\d{2}\b"),
        ),
        (
            ClaimKind::Weekday,
            re(r"(?i)\b(?:mon|tues|wednes|thurs|fri|satur|sun)day\b"),
        ),
        (
            ClaimKind::RelativeDay,
            re(
                r"(?i)\b(?:today|tonight|tomorrow|yesterday|next week|this week|end of (?:the )?(?:day|week|month)|eod|eow)\b",
            ),
        ),
        (
            ClaimKind::Amount,
            re(
                r"(?i)(?:[₹$€£]|\brs\.?|\binr\b|\busd\b|\beur\b)\s?\d[\d,]*(?:\.\d+)?(?:\s?(?:k|m|bn|lakh|lakhs|crore|crores|million|billion))?|\b\d[\d,]*(?:\.\d+)?\s?(?:%|percent|lakh|lakhs|crore|crores|million|billion|rupees|dollars|euros)\b",
            ),
        ),
        (ClaimKind::Number, re(r"\b\d[\d,]*(?:\.\d+)?\b")),
    ]
});

/// Capitalised words that are not names (greetings, sign-offs, pronouns, common openers).
const NOT_NAMES: &[&str] = &[
    "i",
    "hi",
    "hello",
    "hey",
    "dear",
    "thanks",
    "thank",
    "regards",
    "best",
    "cheers",
    "sure",
    "yes",
    "no",
    "ok",
    "okay",
    "please",
    "sorry",
    "great",
    "good",
    "morning",
    "afternoon",
    "evening",
    "noted",
    "done",
    "will",
    "the",
    "a",
    "an",
    "and",
    "but",
    "so",
    "also",
    "just",
    "let",
    "we",
    "you",
    "it",
    "this",
    "that",
    "these",
    "those",
    "here",
    "there",
    "as",
    "if",
    "on",
    "in",
    "for",
    "to",
    "by",
    "at",
    "of",
    "kind",
    "warm",
    "haan",
    "ji",
    "theek",
    "acha",
    "accha",
    "bhai",
    "all",
    "looking",
    "happy",
    "glad",
    "quick",
    "update",
    "fyi",
    "re",
    "fw",
    "fwd",
    "pfa",
];

static NAME: LazyLock<Regex> = LazyLock::new(|| re(r"\b\p{Lu}[\p{L}'’-]+(?:\s+\p{Lu}[\p{L}'’-]+)*"));
pub static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| re(r"\[[^\]\n]{1,80}\]"));

fn month3(m: &str) -> &str {
    &m[..m.len().min(3)]
}

/// Lower-case, unify month names to three letters, drop thousands separators, squeeze spaces,
/// join "5 pm" → "5pm", drop ordinal suffixes.
pub fn normalise(s: &str) -> String {
    static MONTH: LazyLock<Regex> = LazyLock::new(|| re(&format!(r"(?i)\b({MONTHS})\b")));
    static THOUSANDS: LazyLock<Regex> = LazyLock::new(|| re(r"(\d),(\d{3})"));
    static ORDINAL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(\d{1,2})(st|nd|rd|th)\b"));
    static AMPM: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)(\d)\s+(am|pm)\b"));
    static SPACE: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
    let mut t = s.to_lowercase();
    t = MONTH
        .replace_all(&t, |c: &regex::Captures<'_>| month3(&c[1]).to_string())
        .into_owned();
    while THOUSANDS.is_match(&t) {
        t = THOUSANDS.replace_all(&t, "$1$2").into_owned();
    }
    t = ORDINAL.replace_all(&t, "$1").into_owned();
    t = AMPM.replace_all(&t, "$1$2").into_owned();
    t = t.replace(['’', '‘'], "'");
    SPACE.replace_all(&t, " ").into_owned()
}

pub fn char_offsets(s: &str, byte_start: usize, byte_end: usize) -> (usize, usize) {
    (s[..byte_start].chars().count(), s[..byte_end].chars().count())
}

/// Words a name candidate may start with at a sentence start without being checked.
fn sentence_start(draft: &str, byte: usize) -> bool {
    let before = draft[..byte].trim_end_matches([' ', '\t']);
    before.is_empty() || before.ends_with(['.', '!', '?', '\n'])
}

/// Check `draft` against `context` (thread + memory + instruction, joined).
/// Claims in `text` that `context` does not support.
pub fn unsupported(text: &str, context: &str) -> Vec<Claim> {
    let ctx = normalise(context);
    let ctx_words: std::collections::HashSet<String> = ctx
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(str::to_string)
        .collect();
    let mut taken: Vec<(usize, usize)> = PLACEHOLDER.find_iter(text).map(|m| (m.start(), m.end())).collect();
    let mut out = Vec::new();
    let overlaps = |taken: &[(usize, usize)], a: usize, b: usize| taken.iter().any(|&(x, y)| a < y && b > x);
    for (kind, r) in PATTERNS.iter() {
        for m in r.find_iter(text) {
            if overlaps(&taken, m.start(), m.end()) {
                continue;
            }
            taken.push((m.start(), m.end()));
            let n = normalise(m.as_str());
            let n = n.trim_end_matches(['.', ',']);
            // Single digits in lists are noise; longer numbers are claims.
            if *kind == ClaimKind::Number && n.len() < 2 {
                continue;
            }
            if !ctx.contains(n) {
                out.push(Claim {
                    kind: *kind,
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                });
            }
        }
    }
    for m in NAME.find_iter(text) {
        if overlaps(&taken, m.start(), m.end()) {
            continue;
        }
        // Skip leading non-names ("Thanks Ravi" → check "Ravi"); contractions count by their
        // stem ("I've" → "i").
        let words: Vec<&str> = m.as_str().split_whitespace().collect();
        let stem = |w: &str| w.split(['\'', '’']).next().unwrap_or(w).to_lowercase();
        let Some(k) = words.iter().position(|w| !NOT_NAMES.contains(&stem(w).as_str())) else {
            continue;
        };
        if k == 0 && sentence_start(text, m.start()) && words.len() == 1 {
            continue; // a capitalised sentence opener, not a name
        }
        let name = words[k..].join(" ");
        let known = words[k..].iter().all(|w| {
            let w = normalise(w.trim_end_matches(['\'', '’', 's']));
            ctx_words.contains(&w) || ctx.contains(&w)
        });
        if !known {
            let off = m.as_str().find(&name).unwrap_or(0);
            out.push(Claim {
                kind: ClaimKind::Name,
                start: m.start() + off,
                end: m.start() + off + name.len(),
                text: name,
            });
        }
    }
    out.sort_by_key(|c| (c.start, c.end));
    out
}
