//! Randomised robustness of the streaming Ask parser (Stage 7 task 3; not coverage-guided):
//! random sentences (quotes, escapes, braces, brackets, Unicode) serialised as the model would,
//! split into random chunks, must stream out exactly the sentences a full parse gives, once each.

use tsr_retrieval::ask::SentenceStream;

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

const PIECES: &[&str] = &["Ravi", " asked", " {for}", " [the]", " \"sheet\"", " \\ back", " é", " 🙂", ".", ",", " ]}", " {[", "\n", "\t"];

#[test]
fn chunking_never_changes_the_streamed_sentences() {
    let mut r = Rng(0xa5c_2026_1027);
    for _ in 0..3_000 {
        let n = r.below(5);
        let sentences: Vec<(String, Vec<i64>)> = (0..n)
            .map(|_| {
                let t: String = (0..1 + r.below(8)).map(|_| PIECES[r.below(PIECES.len())]).collect();
                let cites: Vec<i64> = (0..r.below(3)).map(|_| r.below(9) as i64).collect();
                (t, cites)
            })
            .collect();
        let json = serde_json::json!({
            "sentences": sentences.iter().map(|(t, c)| serde_json::json!({"text": t, "cites": c})).collect::<Vec<_>>(),
            "insufficient": false,
        })
        .to_string();
        let mut st = SentenceStream::default();
        let mut got = Vec::new();
        let chars: Vec<char> = json.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let k = (1 + r.below(12)).min(chars.len() - i);
            let piece: String = chars[i..i + k].iter().collect();
            got.extend(st.push(&piece));
            i += k;
        }
        assert_eq!(got, sentences, "{json}");
    }
}
