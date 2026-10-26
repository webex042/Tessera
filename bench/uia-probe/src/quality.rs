//! Text quality score, 0..1 (plan §5.4 "Quality score").
//!
//! A weighted mix of: printable-character ratio, share of word-like tokens (2-24 letters,
//! any script), and a penalty for repeated garbage. For OCR text an extra factor checks that
//! the number of lines is plausible for the image height. Weights are a first guess; Stage 0
//! fixture runs tune them, and the final version moves to tsr-capture in Stage 1.

pub fn score(text: &str, image_height_px: Option<u32>) -> f32 {
    let total = text.chars().count();
    if total == 0 {
        return 0.0;
    }
    let printable = text.chars().filter(|c| !c.is_control() || c.is_whitespace()).count();
    let printable_ratio = printable as f32 / total as f32;

    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        return 0.0;
    }
    let wordlike = tokens.iter().filter(|t| is_wordlike(t)).count();
    let wordlike_share = wordlike as f32 / tokens.len() as f32;

    let garbage = tokens.iter().filter(|t| is_garbage(t)).count();
    let garbage_share = garbage as f32 / tokens.len() as f32;

    let mut s = 0.3 * printable_ratio + 0.5 * wordlike_share + 0.2 * (1.0 - garbage_share);

    if let Some(h) = image_height_px.filter(|h| *h > 0) {
        let lines = text.lines().filter(|l| !l.trim().is_empty()).count() as f32;
        // Readable UI text is roughly 12-60 px per line; outside ~2x that range, distrust.
        let per_line = h as f32 / lines.max(1.0);
        if !(6.0..=400.0).contains(&per_line) {
            s *= 0.6;
        }
    }
    s.clamp(0.0, 1.0)
}

fn is_wordlike(token: &str) -> bool {
    let core: String = token.chars().filter(|c| !c.is_ascii_punctuation()).collect();
    let n = core.chars().count();
    (2..=24).contains(&n) && core.chars().all(char::is_alphabetic)
}

fn is_garbage(token: &str) -> bool {
    let has_alnum = token.chars().any(char::is_alphanumeric);
    let mut chars = token.chars();
    let first = chars.next();
    let repeated = token.chars().count() >= 4 && first.is_some_and(|f| token.chars().all(|c| c == f));
    !has_alnum || repeated
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn prose_scores_high() {
        let s = score("Please send the revised budget sheet to Ravi by Friday evening.", None);
        assert!(s > 0.8, "{s}");
    }

    #[test]
    fn hindi_and_hinglish_count_as_words() {
        let s = score("कल सुबह मीटिंग है, please confirm karna", None);
        assert!(s > 0.7, "{s}");
    }

    #[test]
    fn ocr_noise_scores_low() {
        let s = score("|||| ~~ ;;;; ___ ### ||| %%%% ==== ::::", Some(400));
        assert!(s < 0.4, "{s}");
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(score("   ", None), 0.0);
    }
}
