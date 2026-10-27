//! Text normalisation with a map back to source positions (plan §8.3 step 2).
//!
//! Folding: Unicode NFKC (per character), lower case, curly quotes → straight, every dash → `-`,
//! whitespace runs → one space, trimmed. Every output character remembers the index of the source
//! character it came from, so a match in normalised text maps back to an exact source span.

use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Normalized {
    pub text: String,
    /// `map[i]` = source char index of normalised char `i`.
    map: Vec<usize>,
    source_chars: usize,
}

fn fold(c: char) -> Option<char> {
    Some(match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{00B4}' | '`' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '\u{00AB}' | '\u{00BB}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{2212}' => '-',
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' | '\u{00AD}' => return None,
        c => c,
    })
}

pub fn normalize(s: &str) -> Normalized {
    let mut text = String::with_capacity(s.len());
    let mut map = Vec::with_capacity(s.len());
    let mut pending_space: Option<usize> = None;
    let mut n = 0;
    for (ci, c) in s.chars().enumerate() {
        n = ci + 1;
        for k in std::iter::once(c).nfkc() {
            for l in k.to_lowercase() {
                let Some(f) = fold(l) else { continue };
                if f.is_whitespace() {
                    if !map.is_empty() && pending_space.is_none() {
                        pending_space = Some(ci);
                    }
                    continue;
                }
                if let Some(sp) = pending_space.take() {
                    text.push(' ');
                    map.push(sp);
                }
                text.push(f);
                map.push(ci);
            }
        }
    }
    Normalized {
        text,
        map,
        source_chars: n,
    }
}

impl Normalized {
    /// Source char span `[start, end)` for the normalised char range `[a, b)`.
    pub fn source_span(&self, a: usize, b: usize) -> Option<(usize, usize)> {
        if a >= b || b > self.map.len() {
            return None;
        }
        Some((self.map[a], (self.map[b - 1] + 1).min(self.source_chars)))
    }

    pub fn chars(&self) -> usize {
        self.map.len()
    }
}

/// Letters, digits and combining marks (Devanagari vowel signs and viramas are marks).
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || unicode_normalization::char::is_combining_mark(c)
}

/// Words of normalised text: letter/digit/mark runs (any script).
pub fn words(s: &str) -> Vec<String> {
    normalize(s)
        .text
        .split(|c: char| !is_word_char(c))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Char index of a byte offset in `s`.
pub fn char_index(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

/// Substring by char indices.
pub fn char_slice(s: &str, start: usize, end: usize) -> String {
    s.chars().skip(start).take(end.saturating_sub(start)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_and_maps_back() {
        let src = "Sure  Ravi,\u{00A0}I\u{2019}ll send — the ＦＩＮＡＬ sheet";
        let n = normalize(src);
        assert_eq!(n.text, "sure ravi, i'll send - the final sheet");
        let a = n.text.find("i'll").unwrap();
        let (s, e) = n.source_span(a, a + 4).unwrap();
        assert_eq!(char_slice(src, s, e), "I\u{2019}ll");
        let a = n.text.find("final").unwrap();
        let (s, e) = n.source_span(a, a + 5).unwrap();
        assert_eq!(char_slice(src, s, e), "ＦＩＮＡＬ");
    }

    #[test]
    fn zero_width_and_soft_hyphen_disappear() {
        assert_eq!(normalize("ig\u{200B}nore pre\u{00AD}vious").text, "ignore previous");
    }

    #[test]
    fn words_split_any_script() {
        assert_eq!(
            words("Kal subah 10 baje! नमस्ते"),
            vec!["kal", "subah", "10", "baje", "नमस्ते"]
        );
    }
}
