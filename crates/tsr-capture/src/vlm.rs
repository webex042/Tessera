//! Rung 3: a vision-language model on a **masked** window frame (plan §5.4 rung 3, §16.2).
//!
//! Used only when OCR text is poor ([`ocr_quality`] < 0.55), for visual apps, or when the user
//! asks about the screen. Steps, all in memory:
//!
//! 1. [`mask_plan`] — rectangles to black out: every OCR word inside a redaction hit (cards,
//!    keys, OTPs, …), every word of a line that looks like a password prompt, plus caller-supplied
//!    regions (UIA password fields, the notch, user-excluded areas). The matched secret values are
//!    kept (zeroized on drop) only to check the model's answer for leaks.
//! 2. [`apply_masks`] then [`verify_masked`] — every pixel inside every rectangle must be black
//!    *before* anything is encoded; masking is never left to the model.
//! 3. [`downscale`] to ≤ 1 280 px on the long side, [`encode_png`] (no disk, no external crate).
//! 4. The request carries only the image and a fixed instruction — no window title, no OCR text,
//!    no file name — so masked content cannot be rebuilt from text sent alongside it.
//! 5. [`parse_transcript`] — strict: transcription lines (observed) and at most one final
//!    `VISUAL:` line (interpretation, kept apart). Any masked value in the answer discards the
//!    whole answer; then the normal redaction pass runs again. The result is treated like OCR
//!    text (sessions, extraction with evidence checks and the injection guard).
//!
//! If no compatible VLM is available (no vision projector file, or the backend cannot load it),
//! rung 3 is skipped and the OCR text is used as is.

use tsr_core::Sensitive;
use tsr_guard::redact::{Options, find, redact};
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::{ChatModel, ChatRequest, ClientError};
use tsr_infer::prompts::VLM_V1;
use zeroize::{Zeroize, Zeroizing};

pub const MAX_LONG_SIDE: u32 = 1280;
pub const QUALITY_THRESHOLD: f32 = 0.55;
pub const MAX_TRANSCRIPT_CHARS: usize = 12_000;
const PAD: i32 = 3;

/// A BGRA8 frame with tight rows (`width * 4` bytes). Wiped on drop.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame({}x{})", self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// One OCR word with its box in frame pixels.
#[derive(Debug)]
pub struct Word {
    pub text: Sensitive<String>,
    pub rect: Rect,
}

#[derive(Debug, Default)]
pub struct Line {
    pub words: Vec<Word>,
}

pub struct MaskPlan {
    pub rects: Vec<Rect>,
    /// Values hidden by the mask, for the leak check only.
    secrets: Vec<Zeroizing<String>>,
    pub masked_words: u32,
}

impl std::fmt::Debug for MaskPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskPlan")
            .field("rects", &self.rects.len())
            .field("masked_words", &self.masked_words)
            .finish()
    }
}

fn digits_only(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Rectangles to black out (see module docs).
pub fn mask_plan(lines: &[Line], opts: Options, extra: &[Rect]) -> MaskPlan {
    let mut rects: Vec<Rect> = extra.to_vec();
    let mut secrets = Vec::new();
    let mut masked_words = 0;
    for line in lines {
        // Rebuild the line text with each word's char range.
        let mut text = String::new();
        let mut ranges = Vec::new();
        for w in &line.words {
            if !text.is_empty() {
                text.push(' ');
            }
            let s = text.len();
            text.push_str(w.text.expose());
            ranges.push((s, text.len()));
        }
        let whole_line = crate::ocr_guard::looks_like_password_screen(&text);
        let spans = find(&text, opts);
        for (i, (s, e)) in ranges.iter().enumerate() {
            let hit = whole_line || spans.iter().any(|sp| sp.start < *e && *s < sp.end);
            if hit {
                let r = line.words[i].rect;
                rects.push(Rect {
                    x: r.x - PAD,
                    y: r.y - PAD,
                    w: r.w + 2 * PAD,
                    h: r.h + 2 * PAD,
                });
                masked_words += 1;
            }
        }
        for sp in &spans {
            let v = &text[sp.start..sp.end];
            secrets.push(Zeroizing::new(v.to_owned()));
        }
        text.zeroize();
    }
    MaskPlan {
        rects,
        secrets,
        masked_words,
    }
}

fn clip(r: Rect, f: &Frame) -> Option<(usize, usize, usize, usize)> {
    let x0 = r.x.max(0) as usize;
    let y0 = r.y.max(0) as usize;
    let x1 = (r.x + r.w).clamp(0, f.width as i32) as usize;
    let y1 = (r.y + r.h).clamp(0, f.height as i32) as usize;
    (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
}

pub fn apply_masks(f: &mut Frame, rects: &[Rect]) {
    let stride = f.width as usize * 4;
    for r in rects {
        if let Some((x0, y0, x1, y1)) = clip(*r, f) {
            for y in y0..y1 {
                let row = &mut f.bgra[y * stride + x0 * 4..y * stride + x1 * 4];
                for px in row.chunks_mut(4) {
                    px.copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
    }
}

/// Every pixel inside every (clipped) rectangle is opaque black.
pub fn verify_masked(f: &Frame, rects: &[Rect]) -> bool {
    let stride = f.width as usize * 4;
    rects.iter().all(|r| match clip(*r, f) {
        None => true,
        Some((x0, y0, x1, y1)) => (y0..y1).all(|y| {
            f.bgra[y * stride + x0 * 4..y * stride + x1 * 4]
                .chunks(4)
                .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0)
        }),
    })
}

/// Box-filter downscale so the long side is at most `max`. Masks must be applied first.
pub fn downscale(f: &Frame, max: u32) -> Frame {
    let long = f.width.max(f.height);
    if long <= max {
        return Frame {
            width: f.width,
            height: f.height,
            bgra: Zeroizing::new(f.bgra.to_vec()),
        };
    }
    let nw = ((f.width as u64 * max as u64) / long as u64).max(1) as u32;
    let nh = ((f.height as u64 * max as u64) / long as u64).max(1) as u32;
    let mut out = Zeroizing::new(vec![0u8; (nw * nh * 4) as usize]);
    for y in 0..nh {
        let sy0 = (y as u64 * f.height as u64 / nh as u64) as u32;
        let sy1 = (((y + 1) as u64 * f.height as u64 / nh as u64) as u32).max(sy0 + 1);
        for x in 0..nw {
            let sx0 = (x as u64 * f.width as u64 / nw as u64) as u32;
            let sx1 = (((x + 1) as u64 * f.width as u64 / nw as u64) as u32).max(sx0 + 1);
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let i = ((sy * f.width + sx) * 4) as usize;
                    for (a, v) in acc.iter_mut().zip(&f.bgra[i..i + 4]) {
                        *a += *v as u32;
                    }
                    n += 1;
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    Frame {
        width: nw,
        height: nh,
        bgra: out,
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// PNG (RGBA8, zlib "stored" blocks — no compression, no dependency). In memory only.
pub fn encode_png(f: &Frame) -> Zeroizing<Vec<u8>> {
    let mut raw = Zeroizing::new(Vec::with_capacity((f.width * f.height * 4 + f.height) as usize));
    for y in 0..f.height as usize {
        raw.push(0); // filter: none
        for px in f.bgra[y * f.width as usize * 4..(y + 1) * f.width as usize * 4].chunks(4) {
            raw.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    let mut z = Zeroizing::new(vec![0x78, 0x01]);
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = Zeroizing::new(b"\x89PNG\r\n\x1a\n".to_vec());
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = kind.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&f.width.to_be_bytes());
    ihdr.extend_from_slice(&f.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

/// OCR quality score in [0, 1] (plan §5.4): printable ratio, word-like tokens, garbage penalty.
pub fn ocr_quality(text: &str) -> f32 {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < 8 {
        return 0.0;
    }
    let printable = chars
        .iter()
        .filter(|c| c.is_alphanumeric() || c.is_ascii_punctuation())
        .count() as f32
        / chars.len() as f32;
    let toks: Vec<&str> = text.split_whitespace().collect();
    let wordlike = toks
        .iter()
        .filter(|t| {
            let letters = t.chars().filter(|c| c.is_alphabetic()).count();
            (2..=24).contains(&letters) && letters * 2 >= t.chars().count()
        })
        .count() as f32
        / toks.len().max(1) as f32;
    let mut runs = 0;
    let mut prev = '\0';
    let mut run = 0;
    for &c in &chars {
        if c == prev && !c.is_alphanumeric() {
            run += 1;
            if run == 3 {
                runs += 1;
            }
        } else {
            run = 0;
        }
        prev = c;
    }
    let garbage = (runs as f32 * 10.0 / chars.len() as f32).min(1.0);
    (0.4 * printable + 0.6 * wordlike - 0.3 * garbage).clamp(0.0, 1.0)
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VlmError {
    #[error("mask verification failed")]
    MaskNotApplied,
    #[error("model: {0}")]
    Model(ClientError),
    #[error("malformed transcript: {0}")]
    Malformed(&'static str),
    #[error("answer contained a masked value; discarded")]
    Leak,
}

/// Observed text and an optional interpretation, kept apart.
pub struct Transcript {
    pub observed: Sensitive<String>,
    pub visual: Option<Sensitive<String>>,
}

pub fn parse_transcript(raw: &str) -> Result<Transcript, VlmError> {
    if raw.chars().count() > MAX_TRANSCRIPT_CHARS {
        return Err(VlmError::Malformed("too long"));
    }
    if raw
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(VlmError::Malformed("control characters"));
    }
    let lines: Vec<&str> = raw.lines().map(str::trim_end).collect();
    let visual_idx: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_start().to_ascii_uppercase().starts_with("VISUAL:"))
        .map(|(i, _)| i)
        .collect();
    if visual_idx.len() > 1 {
        return Err(VlmError::Malformed("more than one VISUAL line"));
    }
    let last_nonempty = lines.iter().rposition(|l| !l.trim().is_empty());
    if let Some(&i) = visual_idx.first()
        && Some(i) != last_nonempty
    {
        return Err(VlmError::Malformed("VISUAL line must be last"));
    }
    let observed: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !visual_idx.contains(i))
        .map(|(_, l)| *l)
        .collect();
    let visual = visual_idx
        .first()
        .map(|&i| lines[i].trim_start()[7..].trim().to_owned())
        .filter(|v| !v.is_empty());
    Ok(Transcript {
        observed: Sensitive::new(observed.join("\n").trim().to_owned()),
        visual: visual.map(Sensitive::new),
    })
}

pub struct VlmText {
    /// Observed text, redacted; the interpretation (if any) is appended as a final `VISUAL:`
    /// line so downstream code always knows which part is not a transcription.
    pub text: Sensitive<String>,
    pub has_visual: bool,
    pub redactions: u32,
}

/// Run rung 3 on a frame. The frame is masked in place and verified before encoding.
pub fn transcribe(
    model: &dyn ChatModel,
    frame: &mut Frame,
    plan: &MaskPlan,
    opts: Options,
    cancel: &CancelToken,
) -> Result<VlmText, VlmError> {
    apply_masks(frame, &plan.rects);
    if !verify_masked(frame, &plan.rects) {
        return Err(VlmError::MaskNotApplied);
    }
    let small = downscale(frame, MAX_LONG_SIDE);
    let png = encode_png(&small);
    drop(small);
    let req = ChatRequest {
        system: Sensitive::new(VLM_V1.system.to_owned()),
        // Fixed text only: nothing that could restore a masked value.
        user: Sensitive::new("Transcribe this window.".to_owned()),
        image_png: Some(Sensitive::new(png.to_vec())),
        temperature: 0.0,
        max_tokens: 1500,
        json_schema: None,
        stream: true,
        seed: Some(7),
    };
    drop(png);
    let resp = model.chat(&req, cancel).map_err(VlmError::Model)?;
    let raw = resp.content.expose();
    // Leak check against the hidden values (alphanumerics only, so spacing tricks do not help).
    let flat = digits_only(raw);
    for s in &plan.secrets {
        let v = digits_only(s);
        if v.len() >= 4 && flat.contains(&v) {
            return Err(VlmError::Leak);
        }
    }
    let t = parse_transcript(raw)?;
    let mut red = 0;
    let obs = redact(t.observed.expose(), opts);
    red += obs.total();
    let mut text = obs.text;
    let has_visual = t.visual.is_some();
    if let Some(v) = t.visual {
        let r = redact(v.expose(), opts);
        red += r.total();
        text.push_str("\nVISUAL: ");
        text.push_str(&r.text);
    }
    Ok(VlmText {
        text: Sensitive::new(text),
        has_visual,
        redactions: red,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tsr_infer::client::ChatResponse;

    fn frame(w: u32, h: u32, v: u8) -> Frame {
        Frame {
            width: w,
            height: h,
            bgra: Zeroizing::new(vec![v; (w * h * 4) as usize]),
        }
    }

    fn word(t: &str, x: i32) -> Word {
        Word {
            text: Sensitive::new(t.into()),
            rect: Rect { x, y: 10, w: 30, h: 12 },
        }
    }

    fn line(ws: &[&str]) -> Line {
        Line {
            words: ws
                .iter()
                .enumerate()
                .map(|(i, t)| word(t, 10 + 40 * i as i32))
                .collect(),
        }
    }

    #[test]
    fn plan_masks_only_secret_words_and_password_lines() {
        let lines = [
            line(&["Card", "4111", "1111", "1111", "1111", "expires", "soon"]),
            line(&["Meeting", "at", "noon"]),
            line(&["Password:", "••••••••"]),
        ];
        let p = mask_plan(&lines, Options::default(), &[Rect { x: 0, y: 0, w: 5, h: 5 }]);
        assert_eq!(p.masked_words, 4 + 2, "card digits + whole password line");
        assert_eq!(p.rects.len(), 7, "extra region kept");
        assert!(!format!("{p:?}").contains("4111"));
    }

    #[test]
    fn masks_are_applied_and_verified_before_encoding() {
        let mut f = frame(100, 50, 200);
        let rects = [
            Rect {
                x: 10,
                y: 10,
                w: 20,
                h: 10,
            },
            Rect {
                x: 90,
                y: 40,
                w: 50,
                h: 50,
            },
        ];
        assert!(!verify_masked(&f, &rects));
        apply_masks(&mut f, &rects);
        assert!(verify_masked(&f, &rects));
        assert_eq!(&f.bgra[0..4], &[200, 200, 200, 200], "outside untouched");
    }

    #[test]
    fn png_is_valid_and_round_trips_pixels() {
        let mut f = frame(3, 2, 0);
        f.bgra.copy_from_slice(&[
            1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255, 13, 14, 15, 255, 16, 17, 18, 255,
        ]);
        let png = encode_png(&f);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        // Walk chunks and check CRCs; pull the stored zlib payload back out.
        let mut i = 8;
        let mut idat = Vec::new();
        while i < png.len() {
            let len = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
            let kind = &png[i + 4..i + 8];
            let data = &png[i + 8..i + 8 + len];
            let crc = u32::from_be_bytes(png[i + 8 + len..i + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(&png[i + 4..i + 8 + len]), "crc of {kind:?}");
            if kind == b"IDAT" {
                idat.extend_from_slice(data);
            }
            i += 12 + len;
        }
        let raw = &idat[2 + 5..idat.len() - 4]; // zlib header, one stored-block header, adler
        assert_eq!(
            raw,
            &[
                0, 3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 0, 12, 11, 10, 255, 15, 14, 13, 255, 18, 17, 16, 255
            ]
        );
        assert_eq!(
            adler32(raw),
            u32::from_be_bytes(idat[idat.len() - 4..].try_into().unwrap())
        );
    }

    #[test]
    fn downscale_keeps_masked_areas_black() {
        let mut f = frame(2560, 1440, 220);
        let r = [Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        }];
        apply_masks(&mut f, &r);
        let s = downscale(&f, 1280);
        assert_eq!((s.width, s.height), (1280, 720));
        assert!(verify_masked(
            &s,
            &[Rect {
                x: 0,
                y: 0,
                w: 200,
                h: 200
            }]
        ));
    }

    #[test]
    fn transcript_parsing_is_strict() {
        let t = parse_transcript("Line one\nLine two\nVISUAL: a bar chart rising").unwrap();
        assert_eq!(t.observed.expose(), "Line one\nLine two");
        assert_eq!(t.visual.unwrap().expose(), "a bar chart rising");
        assert!(parse_transcript("VISUAL: a\nVISUAL: b").is_err());
        assert!(parse_transcript("VISUAL: first\nthen text").is_err());
        assert!(parse_transcript("bad\u{7}bell").is_err());
        assert!(parse_transcript(&"x".repeat(MAX_TRANSCRIPT_CHARS + 1)).is_err());
        assert!(parse_transcript("just text").unwrap().visual.is_none());
    }

    #[test]
    fn quality_score_separates_text_from_garbage() {
        assert!(ocr_quality("Quarterly budget review moved to Friday afternoon") > QUALITY_THRESHOLD);
        assert!(ocr_quality("|||| ~~~~ ^^^^ #### @@@@ ;;;; ____") < QUALITY_THRESHOLD);
        assert_eq!(ocr_quality(""), 0.0);
    }

    /// SIMULATED VLM: records the request and returns a fixed answer.
    struct Fake(String, Mutex<Option<(String, usize)>>);
    impl ChatModel for Fake {
        fn chat(&self, r: &ChatRequest, _: &CancelToken) -> Result<ChatResponse, ClientError> {
            *self.1.lock().unwrap() = Some((
                r.user.expose().clone(),
                r.image_png.as_ref().map_or(0, |p| p.expose().len()),
            ));
            Ok(ChatResponse {
                content: Sensitive::new(self.0.clone()),
                finish_reason: Some("stop".into()),
                prompt_tokens: None,
                completion_tokens: None,
                elapsed_ms: 1,
            })
        }
    }

    #[test]
    fn transcription_sends_only_the_masked_image_and_fixed_text() {
        let lines = [line(&["Token", "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"])];
        let plan = mask_plan(&lines, Options::default(), &[]);
        let mut f = frame(400, 100, 180);
        let m = Fake("Settings page\nVISUAL: a toggle switch".into(), Mutex::new(None));
        let out = transcribe(&m, &mut f, &plan, Options::default(), &CancelToken::new()).unwrap();
        let (user, png_len) = m.1.lock().unwrap().clone().unwrap();
        assert_eq!(user, "Transcribe this window.");
        assert!(png_len > 0);
        assert!(verify_masked(&f, &plan.rects), "the frame itself stays masked");
        assert_eq!(out.text.expose(), "Settings page\nVISUAL: a toggle switch");
        assert!(out.has_visual);
    }

    #[test]
    fn answers_revealing_a_masked_value_are_discarded() {
        let lines = [line(&["Card", "4111", "1111", "1111", "1111"])];
        let plan = mask_plan(&lines, Options::default(), &[]);
        for leak in [
            "Card 4111 1111 1111 1111",
            "card 4111-1111-1111-1111",
            "Card: 4111111111111111",
        ] {
            let m = Fake(leak.into(), Mutex::new(None));
            let r = transcribe(
                &m,
                &mut frame(300, 60, 90),
                &plan,
                Options::default(),
                &CancelToken::new(),
            );
            assert!(matches!(r, Err(VlmError::Leak)), "{leak}");
        }
    }

    #[test]
    fn secrets_the_model_produces_anyway_are_redacted() {
        let plan = mask_plan(&[], Options::default(), &[]);
        let m = Fake("Your OTP is 482913\nkey AKIAABCDEFGHIJKLMNOP".into(), Mutex::new(None));
        let out = transcribe(
            &m,
            &mut frame(10, 10, 0),
            &plan,
            Options::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert!(!out.text.expose().contains("482913") && !out.text.expose().contains("AKIA"));
        assert!(out.redactions >= 2);
    }

    #[test]
    fn model_errors_and_malformed_answers_surface() {
        let plan = mask_plan(&[], Options::default(), &[]);
        let m = Fake("VISUAL: x\nVISUAL: y".into(), Mutex::new(None));
        assert!(matches!(
            transcribe(
                &m,
                &mut frame(10, 10, 0),
                &plan,
                Options::default(),
                &CancelToken::new()
            ),
            Err(VlmError::Malformed(_))
        ));
    }
}
