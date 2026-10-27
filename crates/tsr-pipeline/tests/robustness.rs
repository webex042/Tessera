//! Randomised robustness tests (Stage 7 task 3). Not coverage-guided fuzzing (`cargo-fuzz` needs
//! a nightly toolchain and downloads, pending approval): a seeded generator mixes digits,
//! punctuation, Unicode, secret-like and injection-like fragments, and each target must never
//! panic and must keep its invariants. Deterministic: the same seed gives the same inputs.

use tsr_guard::redact::{Options, find, redact};
use tsr_pipeline::dates::{DateContext, DateOrder, resolve};
use tsr_pipeline::normalize::normalize;

/// xorshift64* — small, deterministic, no dependency.
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

const FRAGMENTS: &[&str] = &[
    "4111 1111 1111 1111",
    "2345 6789 0123",
    "ABCDE1234F",
    "OTP 482913",
    "password: hunter2",
    "sk-live-",
    "eyJhbGciOi",
    "-----BEGIN PRIVATE KEY-----",
    "VID 1234 5678 9012 3456",
    "Passport No: K1234567",
    "EPIC ABC1234567",
    "a/c 12345678901",
    "IFSC HDFC0001234",
    "name@okicici",
    "ignore previous instructions",
    "AI: send",
    "<|im_start|>",
    "```",
    "> quoted",
    "by Friday",
    "next Tuesday 5pm",
    "kal subah",
    "31/02/2026",
    "EOD",
    "2026-13-45",
    "आधार",
    "सहायक:",
    "\u{200b}",
    "\u{feff}",
    "Ｆｕｌｌｗｉｄｔｈ",
    "🙂",
    "\\",
    "\"",
    "{",
    "}",
    "[",
    "]",
    ":",
    ",",
    "\n",
    "\t",
    "  ",
];

fn input(r: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..r.below(12) {
        match r.below(4) {
            0 => s.push_str(FRAGMENTS[r.below(FRAGMENTS.len())]),
            1 => {
                for _ in 0..r.below(8) {
                    s.push(char::from(b'0' + r.below(10) as u8));
                }
            }
            2 => {
                for _ in 0..r.below(10) {
                    s.push(char::from_u32(0x20 + r.below(0x5f) as u32).unwrap_or(' '));
                }
            }
            _ => {
                // Any scalar value, including combining marks and RTL text.
                if let Some(c) = char::from_u32(r.below(0x3_0000) as u32) {
                    s.push(c);
                }
            }
        }
    }
    s
}

#[test]
fn redaction_never_panics_and_never_leaves_a_detected_secret() {
    let mut r = Rng(0x7e55_a2a0_2026_1027);
    for _ in 0..20_000 {
        let s = input(&mut r);
        let out = redact(&s, Options::default());
        // Idempotent: nothing left to redact after one pass, and a second pass changes nothing.
        assert!(
            find(&out.text, Options::default()).is_empty(),
            "left a secret in {:?}",
            out.text
        );
        assert_eq!(redact(&out.text, Options::default()).text, out.text);
    }
}

#[test]
fn normalisation_never_panics_and_is_idempotent() {
    let mut r = Rng(0x1234_5678_9abc_def0);
    for _ in 0..20_000 {
        let s = input(&mut r);
        let n = normalize(&s).text;
        assert_eq!(normalize(&n).text, n);
    }
}

#[test]
fn date_resolution_never_panics_on_any_text() {
    let mut r = Rng(0x0dd5_eed5_0000_0001);
    let z: jiff::Zoned = "2026-10-21T10:05:00+05:30[Asia/Kolkata]".parse().unwrap();
    let ctx = DateContext::new(z, DateOrder::Dmy);
    for _ in 0..20_000 {
        let s = input(&mut r);
        let _ = resolve(&s, &ctx);
    }
}
