//! Draft request, streaming and output (plan §12.1).
//!
//! The thread and memory snippets are untrusted data: they are escaped into their blocks, and the
//! draft that comes back is fact-checked before the user sees it. The user's own instruction is
//! the only text that may ask for anything.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tsr_core::Sensitive;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::{ChatModel, ChatRequest, ClientError};
use tsr_infer::prompts::{DRAFT_V1, escape_content, render};

use crate::factcheck::{Flag, check};

/// Plan §12.1: the last ~2 000 characters of the thread around the focus.
pub const THREAD_CHARS: usize = 2_000;
pub const SNIPPET_CHARS: usize = 400;
pub const MAX_INSTRUCTION_CHARS: usize = 300;

pub struct DraftInput<'a> {
    pub app: &'a str,
    pub thread: &'a str,
    /// (item id, text) of retrieved memory, best first.
    pub memory: &'a [(i64, &'a str)],
    pub instruction: &'a str,
    pub style_examples: &'a [&'a str],
}

#[derive(Clone, Debug, Serialize)]
pub struct Draft {
    pub text: String,
    /// Item ids of the memory snippets the model says it used (only ids it was shown).
    pub used: Vec<i64>,
    pub placeholders: Vec<String>,
    pub flags: Vec<Flag>,
    pub first_text_ms: Option<u64>,
    pub total_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error("model: {0}")]
    Model(#[from] ClientError),
    #[error("the model's output was not a valid draft")]
    Invalid,
    #[error("instruction too long")]
    InstructionTooLong,
}

/// The tail of the thread (whole lines where possible), at most `max` characters.
pub fn thread_window(thread: &str, max: usize) -> &str {
    let n = thread.chars().count();
    if n <= max {
        return thread;
    }
    let skip = n - max;
    let byte = thread.char_indices().nth(skip).map(|(b, _)| b).unwrap_or(0);
    let rest = &thread[byte..];
    match rest.find('\n') {
        Some(i) if i + 1 < rest.len() && i < 200 => &rest[i + 1..],
        _ => rest,
    }
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Everything a claim may legitimately come from.
pub fn context_of(input: &DraftInput<'_>) -> String {
    let mut c = String::from(thread_window(input.thread, THREAD_CHARS));
    for (_, t) in input.memory {
        c.push('\n');
        c.push_str(&clip(t, SNIPPET_CHARS));
    }
    c.push('\n');
    c.push_str(input.instruction);
    c
}

pub fn request(input: &DraftInput<'_>, temperature: f32) -> ChatRequest {
    let p = &*DRAFT_V1;
    let system = render(p.system, &[("app_name", &escape_content(input.app))]).expect("template");
    let style = if input.style_examples.is_empty() {
        "(none yet)".to_string()
    } else {
        input
            .style_examples
            .iter()
            .map(|s| escape_content(&clip(s, 600)))
            .collect::<Vec<_>>()
            .join("\n---\n")
    };
    let snippets = if input.memory.is_empty() {
        "(none)".to_string()
    } else {
        input
            .memory
            .iter()
            .enumerate()
            .map(|(n, (_, t))| format!("[{}] {}", n + 1, escape_content(&clip(t, SNIPPET_CHARS))))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let user = render(
        p.user,
        &[
            ("style_examples", &style),
            ("thread", &escape_content(thread_window(input.thread, THREAD_CHARS))),
            ("snippets", &snippets),
            ("instruction", &escape_content(input.instruction)),
        ],
    )
    .expect("template");
    ChatRequest {
        system: Sensitive::new(system),
        user: Sensitive::new(user),
        image_png: None,
        temperature,
        max_tokens: 700,
        json_schema: p.schema.clone(),
        stream: true,
        seed: Some(11),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Out {
    draft: String,
    used: Vec<i64>,
    placeholders: Vec<String>,
}

fn parse(raw: &str, n_snippets: usize, memory: &[(i64, &str)]) -> Result<(String, Vec<i64>, Vec<String>), DraftError> {
    let o: Out = serde_json::from_str(raw.trim()).map_err(|_| DraftError::Invalid)?;
    if o.draft.trim().is_empty() || o.draft.chars().count() > 2_000 || o.placeholders.len() > 20 {
        return Err(DraftError::Invalid);
    }
    // `used` cites snippet numbers 1..=n; anything else is dropped.
    let mut used: Vec<i64> = o
        .used
        .into_iter()
        .filter(|u| *u >= 1 && (*u as usize) <= n_snippets)
        .map(|u| memory[u as usize - 1].0)
        .collect();
    used.dedup();
    Ok((
        o.draft,
        used,
        o.placeholders.into_iter().map(|p| clip(&p, 80)).collect(),
    ))
}

/// Incremental view of the `"draft"` string while the JSON streams in.
#[derive(Default)]
pub struct PartialDraft {
    buf: String,
}

impl PartialDraft {
    /// Feed more output; returns the draft text decoded so far (may be empty).
    pub fn push(&mut self, delta: &str) -> String {
        self.buf.push_str(delta);
        let Some(k) = self.buf.find("\"draft\"") else {
            return String::new();
        };
        let rest = &self.buf[k + 7..];
        let Some(q) = rest.find('"') else { return String::new() };
        if !rest[..q].trim().trim_start_matches(':').trim().is_empty() {
            return String::new();
        }
        let mut out = String::new();
        let mut chars = rest[q + 1..].chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => {}
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if hex.len() < 4 {
                            break; // escape not complete yet
                        }
                        if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            out.push(ch);
                        }
                    }
                    Some(other) => out.push(other),
                    None => break,
                },
                _ => out.push(c),
            }
        }
        out
    }
}

/// Run one draft: stream (calling `on_text` with the draft so far), parse, fact-check. One retry
/// at temperature 0 if the output is invalid.
pub fn run(
    model: &dyn ChatModel,
    input: &DraftInput<'_>,
    cancel: &CancelToken,
    on_text: &mut dyn FnMut(&str),
) -> Result<Draft, DraftError> {
    if input.instruction.chars().count() > MAX_INSTRUCTION_CHARS {
        return Err(DraftError::InstructionTooLong);
    }
    let t0 = Instant::now();
    let mut first: Option<u64> = None;
    let context = context_of(input);
    for temperature in [0.4f32, 0.0] {
        let mut partial = PartialDraft::default();
        let mut shown = 0usize;
        let r = model.chat_streaming(&request(input, temperature), cancel, &mut |d| {
            let text = partial.push(d);
            if text.len() > shown {
                shown = text.len();
                if first.is_none() && !text.trim().is_empty() {
                    first = Some(t0.elapsed().as_millis() as u64);
                }
                on_text(&text);
            }
        })?;
        if let Ok((text, used, placeholders)) = parse(r.content.expose(), input.memory.len(), input.memory) {
            let flags = check(&text, &context);
            return Ok(Draft {
                text,
                used,
                placeholders,
                flags,
                first_text_ms: first,
                total_ms: t0.elapsed().as_millis() as u64,
            });
        }
    }
    Err(DraftError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_draft_decodes_as_it_streams() {
        let mut p = PartialDraft::default();
        assert_eq!(p.push("{\n  \"dra"), "");
        assert_eq!(p.push("ft\": \"Hi Ra"), "Hi Ra");
        assert_eq!(p.push("vi,\\nsent \\u00e9"), "Hi Ravi,\nsent é");
        assert_eq!(p.push("\", \"used\": [1]"), "Hi Ravi,\nsent é");
    }

    #[test]
    fn thread_window_keeps_the_tail_on_a_line_boundary() {
        let t = format!("{}\nlast line", "x".repeat(3000));
        let w = thread_window(&t, 20);
        assert_eq!(w, "last line");
        assert_eq!(thread_window("short", 20), "short");
    }

    #[test]
    fn used_ids_map_to_items_and_invalid_ones_drop() {
        let mem = [(41, "a"), (42, "b")];
        let (_, used, _) = parse(r#"{"draft":"x","used":[2,9,0],"placeholders":[]}"#, 2, &mem).unwrap();
        assert_eq!(used, vec![42]);
        assert!(parse(r#"{"draft":"","used":[],"placeholders":[]}"#, 2, &mem).is_err());
        assert!(parse(r#"{"draft":"x","used":[],"placeholders":[],"send":true}"#, 2, &mem).is_err());
    }
}
