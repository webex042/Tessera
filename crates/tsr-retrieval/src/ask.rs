//! Ask with sources (plan §10.4): snippets, the request, streaming sentences, citation checks.
//!
//! Every sentence the model writes is verified by code as soon as it is complete:
//! 1. every cited snippet number exists (otherwise it is a *fabricated citation*);
//! 2. every claim in the sentence (number, date, time, amount, e-mail, URL, capitalised name)
//!    appears in at least one cited snippet;
//! 3. content-word overlap with the cited snippets is at least 0.35.
//!
//! Unverified sentences are kept but marked (the UI dims them). If nothing verifies, the answer is
//! "I couldn't find that in your memory" with the top three snippets.

use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tsr_core::Sensitive;
use tsr_guard::claims;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::{ChatModel, ChatRequest, ClientError};
use tsr_infer::prompts::{ASK_V2, escape_content, render};

pub const MAX_SNIPPETS: usize = 8;
/// ≈ 1 500 tokens of snippets in total (plan §10.3 step 5).
pub const SNIPPET_BUDGET_CHARS: usize = 6_000;
pub const SNIPPET_CHARS: usize = 700;
pub const MIN_OVERLAP: f64 = 0.35;
pub const MAX_QUESTION_CHARS: usize = 300;

#[derive(Clone, Debug, Serialize)]
pub struct Snippet {
    /// 1-based number shown to the model.
    pub n: usize,
    pub item_id: i64,
    pub source_id: i64,
    pub started_at: i64,
    pub app: String,
    pub title: Option<String>,
    #[serde(skip)]
    pub text: Sensitive<String>,
    /// The trimmed text, for the UI's hover preview (already redacted at capture).
    pub preview: String,
}

/// Window of `text` of at most `max` characters around the first query-term hit.
fn window(text: &str, terms: &[String], max: usize) -> String {
    let low = text.to_lowercase();
    let at = terms
        .iter()
        .filter(|t| t.len() >= 3)
        .filter_map(|t| low.find(t.as_str()))
        .min()
        .unwrap_or(0);
    let chars: Vec<char> = text.chars().collect();
    let pos = text[..at].chars().count();
    let start = pos.saturating_sub(max / 3);
    let end = (start + max).min(chars.len());
    let start = end.saturating_sub(max).min(start);
    chars[start..end].iter().collect()
}

/// Content words (lower case, no stop words, ≥ 3 letters or any number).
fn words(s: &str) -> HashSet<String> {
    const STOP: [&str; 40] = [
        "the", "and", "for", "with", "that", "this", "from", "have", "has", "had", "was", "were", "are", "you", "your",
        "about", "what", "when", "did", "does", "will", "would", "can", "could", "they", "them", "their", "there",
        "which", "who", "whom", "how", "not", "but", "also", "into", "onto", "then", "than", "she",
    ];
    claims::normalise(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| (w.len() >= 3 || w.chars().all(|c| c.is_ascii_digit())) && !w.is_empty() && !STOP.contains(w))
        .map(str::to_string)
        .collect()
}

/// Build the numbered snippets from ranked items (best first): one per source, within budget.
pub fn snippets(
    conn: &rusqlite::Connection,
    ranked: &[i64],
    question: &str,
) -> Result<Vec<Snippet>, tsr_store::StoreError> {
    let terms: Vec<String> = words(question).into_iter().collect();
    let mut out: Vec<Snippet> = Vec::new();
    let mut seen_sources = HashSet::new();
    let mut used = 0usize;
    for id in ranked {
        if out.len() == MAX_SNIPPETS {
            break;
        }
        let Some(it) = tsr_store::items::get_item(conn, *id)? else {
            continue;
        };
        if !seen_sources.insert(it.source_id) {
            continue;
        }
        let src = tsr_store::items::get_source(conn, it.source_id)?;
        let raw_w = window(it.text.expose(), &terms, SNIPPET_CHARS);
        let w = tsr_guard::redact::mask_pii(&raw_w).text;
        if used + w.len() > SNIPPET_BUDGET_CHARS && !out.is_empty() {
            break;
        }
        used += w.len();
        out.push(Snippet {
            n: out.len() + 1,
            item_id: *id,
            source_id: it.source_id,
            started_at: it.started_at,
            app: src
                .as_ref()
                .map(|s| s.app_name.clone().unwrap_or_else(|| s.app_exe.clone()))
                .unwrap_or_default(),
            title: src.and_then(|s| s.window_title.map(|t| t.expose().clone())),
            preview: w.clone(),
            text: Sensitive::new(w),
        });
    }
    Ok(out)
}

pub fn request(
    question: &str,
    snippets: &[Snippet],
    local_date: &str,
    weekday: &str,
    label: &dyn Fn(i64) -> String,
) -> ChatRequest {
    let p = &*ASK_V2;
    let lines = snippets
        .iter()
        .map(|s| {
            format!(
                "[{}] {} {}{} — {}",
                s.n,
                label(s.started_at),
                escape_content(&s.app),
                s.title
                    .as_ref()
                    .map(|t| format!(" \"{}\"", escape_content(t)))
                    .unwrap_or_default(),
                escape_content(s.text.expose())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let user = render(
        p.user,
        &[
            ("local_date", local_date),
            ("weekday", weekday),
            ("snippets", if lines.is_empty() { "(none)" } else { &lines }),
            ("question", &escape_content(question)),
        ],
    )
    .expect("template");
    ChatRequest {
        system: Sensitive::new(p.system.to_string()),
        user: Sensitive::new(user),
        image_png: None,
        temperature: 0.1,
        max_tokens: 700,
        json_schema: p.schema.clone(),
        stream: true,
        seed: Some(5),
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AnswerSentence {
    pub text: String,
    /// Valid snippet numbers cited.
    pub cites: Vec<usize>,
    /// The items behind those citations (for source chips).
    pub item_ids: Vec<i64>,
    pub verified: bool,
    /// Cited numbers that do not exist.
    pub fabricated: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Answer {
    pub sentences: Vec<AnswerSentence>,
    pub insufficient: bool,
    /// Nothing verified: show "I couldn't find that" and these snippet numbers.
    pub not_found: bool,
    pub fallback: Vec<usize>,
    pub first_sentence_ms: Option<u64>,
    pub total_ms: u64,
}

/// Verify one sentence against the snippets it cites.
pub fn verify(text: &str, cites: &[usize], snippets: &[Snippet]) -> AnswerSentence {
    let mut valid = Vec::new();
    let mut fabricated = 0;
    for c in cites {
        if *c >= 1 && *c <= snippets.len() {
            if !valid.contains(c) {
                valid.push(*c);
            }
        } else {
            fabricated += 1;
        }
    }
    let cited: Vec<&str> = valid.iter().map(|c| snippets[c - 1].text.expose().as_str()).collect();
    let ctx = cited.join("\n");
    let mut verified = !cited.is_empty() && fabricated == 0 && claims::unsupported(text, &ctx).is_empty();
    if verified {
        let w = words(text);
        if !w.is_empty() {
            let cw = words(&ctx);
            let hits = w.iter().filter(|x| cw.contains(*x)).count();
            verified = hits as f64 / w.len() as f64 >= MIN_OVERLAP;
        }
    }
    let safe_text = tsr_guard::redact::mask_pii(text).text;
    AnswerSentence {
        text: safe_text,
        item_ids: valid.iter().map(|c| snippets[c - 1].item_id).collect(),
        cites: valid,
        verified,
        fabricated,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSentence {
    text: String,
    cites: Vec<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnswer {
    sentences: Vec<RawSentence>,
    insufficient: bool,
}

/// Picks complete sentence objects out of the streaming JSON as they close.
#[derive(Default)]
pub struct SentenceStream {
    buf: String,
    emitted: usize,
}

impl SentenceStream {
    /// Feed output; returns the sentence objects completed since the last call.
    pub fn push(&mut self, delta: &str) -> Vec<(String, Vec<i64>)> {
        self.buf.push_str(delta);
        let Some(k) = self.buf.find("\"sentences\"") else {
            return Vec::new();
        };
        let Some(open) = self.buf[k..].find('[') else {
            return Vec::new();
        };
        let body = &self.buf[k + open + 1..];
        let mut out = Vec::new();
        let (mut depth, mut in_str, mut esc, mut start, mut count) = (0i32, false, false, None, 0usize);
        for (i, c) in body.char_indices() {
            if in_str {
                match (esc, c) {
                    (true, _) => esc = false,
                    (false, '\\') => esc = true,
                    (false, '"') => in_str = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_str = true,
                '{' => {
                    if depth == 0 {
                        start = Some(i);
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0
                        && let Some(s) = start.take()
                    {
                        count += 1;
                        if count > self.emitted
                            && let Ok(r) = serde_json::from_str::<RawSentence>(&body[s..=i])
                        {
                            out.push((r.text, r.cites));
                        }
                    }
                }
                ']' if depth == 0 => break,
                _ => {}
            }
        }
        self.emitted += out.len();
        out
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AskError {
    #[error("model: {0}")]
    Model(#[from] ClientError),
    #[error("the model's answer was not valid")]
    Invalid,
}

/// Ask the model and verify every sentence (calling `on_sentence` as each one completes).
pub fn answer(
    model: &dyn ChatModel,
    req: &ChatRequest,
    snippets: &[Snippet],
    cancel: &CancelToken,
    on_sentence: &mut dyn FnMut(&AnswerSentence),
) -> Result<Answer, AskError> {
    let t0 = Instant::now();
    let mut first = None;
    let mut stream = SentenceStream::default();
    let r = model.chat_streaming(req, cancel, &mut |d| {
        for (text, cites) in stream.push(d) {
            if first.is_none() {
                first = Some(t0.elapsed().as_millis() as u64);
            }
            let cites: Vec<usize> = cites.into_iter().map(|c| c.max(0) as usize).collect();
            on_sentence(&verify(&text, &cites, snippets));
        }
    })?;
    let raw: RawAnswer = serde_json::from_str(r.content.expose().trim()).map_err(|_| AskError::Invalid)?;
    let sentences: Vec<AnswerSentence> = raw
        .sentences
        .iter()
        .take(8)
        .map(|s| {
            let cites: Vec<usize> = s.cites.iter().map(|c| (*c).max(0) as usize).collect();
            verify(&s.text, &cites, snippets)
        })
        .collect();
    let not_found = !sentences.iter().any(|s| s.verified);
    Ok(Answer {
        fallback: if not_found {
            snippets.iter().take(3).map(|s| s.n).collect()
        } else {
            Vec::new()
        },
        not_found,
        insufficient: raw.insufficient,
        sentences,
        first_sentence_ms: first,
        total_ms: t0.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snip(n: usize, text: &str) -> Snippet {
        Snippet {
            n,
            item_id: 100 + n as i64,
            source_id: n as i64,
            started_at: 0,
            app: "Outlook".into(),
            title: None,
            text: Sensitive::new(text.into()),
            preview: text.into(),
        }
    }

    #[test]
    fn sentences_verify_only_against_what_they_cite() {
        let s = [
            snip(1, "Ravi: can you send the revised budget sheet by Friday?"),
            snip(2, "Lena moved the design review to Thursday 3pm."),
        ];
        let ok = verify("You promised Ravi the revised budget sheet by Friday.", &[1], &s);
        assert!(ok.verified && ok.fabricated == 0 && ok.item_ids == vec![101]);
        let wrong_source = verify("You promised Ravi the revised budget sheet by Friday.", &[2], &s);
        assert!(!wrong_source.verified, "claims must be in the cited snippet");
        let invented = verify("The design review is on Thursday at 5pm.", &[2], &s);
        assert!(!invented.verified, "5pm is not in the source");
        let fabricated = verify("The review moved.", &[2, 7], &s);
        assert_eq!(fabricated.fabricated, 1);
        assert!(!fabricated.verified);
        assert!(
            !verify("Unrelated words entirely here.", &[1], &s).verified,
            "overlap below 0.35"
        );
    }

    #[test]
    fn sentences_stream_out_as_their_objects_close() {
        let mut st = SentenceStream::default();
        assert!(
            st.push("{\"sentences\": [{\"text\": \"Ravi asked for the sh")
                .is_empty()
        );
        let a = st.push("eet {by Friday}.\", \"cites\": [1]}, {\"text\": \"Second");
        assert_eq!(a, vec![("Ravi asked for the sheet {by Friday}.".to_string(), vec![1])]);
        let b = st.push(".\", \"cites\": [2]}], \"insufficient\": false}");
        assert_eq!(b, vec![("Second.".to_string(), vec![2])]);
        assert!(st.push("").is_empty(), "no repeats");
    }
}
