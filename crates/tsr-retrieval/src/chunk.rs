//! Deterministic, provenance-preserving chunking (plan §8.1 step 4, §9.4).
//!
//! Chunks are windows of at most [`MAX_WORDS`] words (≈ 512 tokens for E5's tokenizer on
//! English; fewer for scripts that tokenise finer) overlapping by [`OVERLAP_WORDS`], cut at
//! paragraph or sentence ends when one is close. Each chunk is a *span* of the item text
//! (`start_ch..end_ch`), so a retrieved chunk always leads back to the exact source text.
//! The configuration is versioned ([`CHUNKER_VER`]) and stored with every vector.

pub const CHUNKER_VER: &str = "w380o48-v1";
pub const MAX_WORDS: usize = 380;
pub const OVERLAP_WORDS: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub ord: u32,
    pub start_ch: usize,
    pub end_ch: usize,
}

/// Word spans (char offsets) and whether the word ends a sentence/paragraph.
fn word_spans(text: &str) -> Vec<(usize, usize, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let st = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let last = chars[i - 1];
        let para = chars[i..]
            .iter()
            .take_while(|c| c.is_whitespace())
            .filter(|&&c| c == '\n')
            .count()
            > 0;
        out.push((st, i, para || matches!(last, '.' | '!' | '?' | '।')));
    }
    out
}

pub fn chunk(text: &str) -> Vec<Chunk> {
    chunk_with(text, MAX_WORDS, OVERLAP_WORDS)
}

pub fn chunk_with(text: &str, max: usize, overlap: usize) -> Vec<Chunk> {
    assert!(max > overlap * 2, "overlap too large");
    let w = word_spans(text);
    let mut out = Vec::new();
    let mut start = 0;
    while start < w.len() {
        let mut end = (start + max).min(w.len());
        if end < w.len() {
            // Prefer a boundary in the last quarter of the window.
            if let Some(b) = (start + max * 3 / 4..end).rev().find(|&k| w[k].2) {
                end = b + 1;
            }
        }
        out.push(Chunk {
            ord: out.len() as u32,
            start_ch: w[start].0,
            end_ch: w[end - 1].1,
        });
        if end == w.len() {
            break;
        }
        start = end.saturating_sub(overlap).max(start + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_chunk_covering_it() {
        let t = "  Hello there.\nSecond line  ";
        let c = chunk(t);
        assert_eq!(c.len(), 1);
        let s: String = t
            .chars()
            .skip(c[0].start_ch)
            .take(c[0].end_ch - c[0].start_ch)
            .collect();
        assert_eq!(s, "Hello there.\nSecond line");
        assert!(chunk("   ").is_empty());
    }

    #[test]
    fn long_text_overlaps_and_prefers_sentence_ends_deterministically() {
        let t: String = (0..1000)
            .map(|i| {
                if i % 10 == 9 {
                    format!("w{i}. ")
                } else {
                    format!("w{i} ")
                }
            })
            .collect();
        let a = chunk_with(&t, 100, 10);
        assert_eq!(a, chunk_with(&t, 100, 10), "deterministic");
        for pair in a.windows(2) {
            assert!(pair[1].start_ch < pair[0].end_ch, "overlap");
            assert!(pair[1].ord == pair[0].ord + 1);
        }
        let first: String = t.chars().take(a[0].end_ch).collect();
        assert!(first.ends_with('.'), "cut at a sentence end: {first:?}");
        assert_eq!(a.last().unwrap().end_ch, t.trim_end().chars().count());
    }
}
