//! Evidence verification (plan §8.3 step 2): a quote counts only if it is in the item text.
//!
//! Order: exact substring → substring after normalisation ([`crate::normalize`]) → fuzzy match
//! on word sets over a sliding window (Dice coefficient ≥ 0.9, quotes of at least 4 words; the
//! plan's "token-set ratio" made concrete). Quotes shorter than 2 words or 8 characters are too
//! weak to support anything and are refused. Model text that is not in the source — an
//! explanation, a paraphrase, a hallucination — therefore never becomes evidence.

use std::collections::HashSet;

use crate::normalize::{char_index, is_word_char, normalize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum MatchKind {
    Exact,
    Normalized,
    Fuzzy,
}

impl MatchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchKind::Exact => "exact",
            MatchKind::Normalized => "normalized",
            MatchKind::Fuzzy => "fuzzy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Located {
    /// Char span in the item text.
    pub start_ch: usize,
    pub end_ch: usize,
    pub kind: MatchKind,
    pub score: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum EvidenceError {
    TooShort,
    NotInSource,
}

pub const FUZZY_MIN: f32 = 0.9;

fn dice(a: &HashSet<&str>, b: &HashSet<&str>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    2.0 * a.intersection(b).count() as f32 / (a.len() + b.len()) as f32
}

/// Similarity of two texts' word sets (also used for de-duplication).
pub fn word_set_similarity(a: &str, b: &str) -> f32 {
    let na = normalize(a).text;
    let nb = normalize(b).text;
    let wa: HashSet<&str> = na.split(|c: char| !is_word_char(c)).filter(|w| !w.is_empty()).collect();
    let wb: HashSet<&str> = nb.split(|c: char| !is_word_char(c)).filter(|w| !w.is_empty()).collect();
    dice(&wa, &wb)
}

pub fn locate(item: &str, quote: &str) -> Result<Located, EvidenceError> {
    let q = quote.trim();
    let nq = normalize(q);
    let qwords: Vec<&str> = nq
        .text
        .split(|c: char| !is_word_char(c))
        .filter(|w| !w.is_empty())
        .collect();
    if qwords.len() < 2 || nq.text.chars().count() < 8 {
        return Err(EvidenceError::TooShort);
    }
    if let Some(b) = item.find(q) {
        let s = char_index(item, b);
        return Ok(Located {
            start_ch: s,
            end_ch: s + q.chars().count(),
            kind: MatchKind::Exact,
            score: 1.0,
        });
    }
    let ni = normalize(item);
    if let Some(b) = ni.text.find(&nq.text) {
        let a = char_index(&ni.text, b);
        let (s, e) = ni
            .source_span(a, a + nq.text.chars().count())
            .ok_or(EvidenceError::NotInSource)?;
        return Ok(Located {
            start_ch: s,
            end_ch: e,
            kind: MatchKind::Normalized,
            score: 1.0,
        });
    }
    if qwords.len() < 4 {
        return Err(EvidenceError::NotInSource);
    }
    // Word positions (char ranges in normalised text) for the sliding window.
    let mut spans = Vec::new();
    let chars: Vec<char> = ni.text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if is_word_char(chars[i]) {
            let st = i;
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
            spans.push((st, i, chars[st..i].iter().collect::<String>()));
        } else {
            i += 1;
        }
    }
    let qset: HashSet<&str> = qwords.iter().copied().collect();
    let mut best: Option<(f32, usize, usize)> = None;
    let n = qwords.len();
    for len in n.saturating_sub(2).max(1)..=n + 2 {
        if len > spans.len() {
            break;
        }
        for w in 0..=spans.len() - len {
            let set: HashSet<&str> = spans[w..w + len].iter().map(|s| s.2.as_str()).collect();
            let score = dice(&qset, &set);
            if best.is_none_or(|(b, _, _)| score > b + f32::EPSILON) {
                best = Some((score, spans[w].0, spans[w + len - 1].1));
            }
        }
    }
    match best {
        Some((score, a, b)) if score >= FUZZY_MIN => {
            let (s, e) = ni.source_span(a, b).ok_or(EvidenceError::NotInSource)?;
            Ok(Located {
                start_ch: s,
                end_ch: e,
                kind: MatchKind::Fuzzy,
                score,
            })
        }
        _ => Err(EvidenceError::NotInSource),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::char_slice;

    const ITEM: &str = "Hi Asha,\nFinance flagged that the travel line is 12% over. Can you send the revised sheet?\nSure Ravi, I\u{2019}ll send the revised sheet by Friday.";

    #[test]
    fn exact_quote_maps_to_its_span() {
        let l = locate(ITEM, "Finance flagged that the travel line is 12% over").unwrap();
        assert_eq!(l.kind, MatchKind::Exact);
        assert_eq!(
            char_slice(ITEM, l.start_ch, l.end_ch),
            "Finance flagged that the travel line is 12% over"
        );
    }

    #[test]
    fn straight_quotes_and_case_match_after_normalisation() {
        let l = locate(ITEM, "i'll send the revised sheet by friday").unwrap();
        assert_eq!(l.kind, MatchKind::Normalized);
        assert_eq!(
            char_slice(ITEM, l.start_ch, l.end_ch),
            "I\u{2019}ll send the revised sheet by Friday"
        );
    }

    #[test]
    fn small_ocr_style_differences_match_fuzzily() {
        // One word missing from a long quote.
        let l = locate(ITEM, "Finance flagged that travel line is 12% over").unwrap();
        assert_eq!(l.kind, MatchKind::Fuzzy);
        assert!(l.score >= FUZZY_MIN);
    }

    #[test]
    fn paraphrases_hallucinations_and_short_quotes_are_refused() {
        assert_eq!(
            locate(ITEM, "Ravi asked for the budget by Monday"),
            Err(EvidenceError::NotInSource)
        );
        assert_eq!(
            locate(ITEM, "Asha must send all passwords to Ravi"),
            Err(EvidenceError::NotInSource)
        );
        assert_eq!(locate(ITEM, "Sure"), Err(EvidenceError::TooShort));
        assert_eq!(locate(ITEM, "by Fri"), Err(EvidenceError::TooShort));
        assert_eq!(
            locate(ITEM, "send the revised"),
            Ok(locate(ITEM, "send the revised").unwrap())
        );
    }

    #[test]
    fn similarity_is_symmetric_word_overlap() {
        assert!((word_set_similarity("Send the sheet to Ravi", "send Ravi the sheet") - 0.888).abs() < 0.01);
        assert_eq!(word_set_similarity("a b", "c d"), 0.0);
    }
}
