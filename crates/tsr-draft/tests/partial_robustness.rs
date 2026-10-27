//! Randomised robustness of the streaming draft view (Stage 7 task 3; not coverage-guided):
//! after every chunk the visible text is a prefix of the final draft, and at the end it equals it.

use tsr_draft::draft::PartialDraft;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const PIECES: &[&str] = &["Hi", " Ravi", ",", "\n", " \"quoted\"", " back\\slash", " é", " ü", " 🙂", " {x}", " [y]", "\t", " \u{2028}"];

#[test]
fn visible_text_is_always_a_prefix_and_ends_complete() {
    let mut r = Rng(0xd4af7_2026);
    for _ in 0..3_000 {
        let text: String = (0..1 + r.below(10)).map(|_| PIECES[r.below(PIECES.len())]).collect();
        let json = serde_json::json!({"draft": text, "used": [], "placeholders": []}).to_string();
        let chars: Vec<char> = json.chars().collect();
        let mut p = PartialDraft::default();
        let mut last = String::new();
        let mut i = 0;
        while i < chars.len() {
            let k = (1 + r.below(9)).min(chars.len() - i);
            let piece: String = chars[i..i + k].iter().collect();
            last = p.push(&piece);
            assert!(text.starts_with(&last), "{last:?} is not a prefix of {text:?}");
            i += k;
        }
        assert_eq!(last, text);
    }
}
