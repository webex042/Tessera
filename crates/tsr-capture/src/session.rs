//! Window sessions (plan §5.5 delta extraction, §5.6).
//!
//! A session is continuous attention on one window + document. Snapshots of the focused window
//! (already **redacted**) are merged into it: an unchanged snapshot (SimHash distance ≤ 3) only
//! extends `ended_at`; a changed one adds only the lines this session has not seen yet, so a
//! scrolling chat stores each message once. A session is flushed — becoming one stored item —
//! when focus has been elsewhere for 30 s, after 5 min of continuous use, when its text passes
//! 6 000 characters, or on lock/shutdown. Pause **drops** open sessions without storing them.
//! Times are passed in (unix ms), so all of this is testable without clocks or windows.

use std::collections::{HashMap, HashSet};

use tsr_core::Sensitive;

use crate::simhash::{same, simhash};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    Uia,
    Ocr,
    Vlm,
}

/// Which session a snapshot belongs to: same process, same top-level window, same document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub exe: String,
    pub hwnd: isize,
    /// URL for browsers; otherwise the redacted title with volatile counters such as "(3)" removed.
    pub document: Sensitive<String>,
}

/// Source facts stored with the item (plan §9.2 `sources`). Title and URL already redacted/cleaned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFacts {
    pub exe: String,
    pub app_name: Option<String>,
    pub title: Sensitive<String>,
    pub url: Option<Sensitive<String>>,
}

pub struct Snapshot {
    pub key: SessionKey,
    pub source: SourceFacts,
    /// Redacted text of the window.
    pub text: Sensitive<String>,
    pub rung: Rung,
    pub redactions: u32,
    pub at_ms: i64,
}

/// A finished session, ready to become an `items` row.
#[derive(Debug)]
pub struct Flushed {
    pub source: SourceFacts,
    pub started_at: i64,
    pub ended_at: i64,
    pub text: Sensitive<String>,
    pub rung: Rung,
    pub simhash: u64,
    pub redactions: u32,
    pub reason: FlushReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushReason {
    FocusLeft,
    MaxDuration,
    MaxChars,
    LockOrShutdown,
}

#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    pub flush_away_ms: i64,
    pub flush_max_ms: i64,
    pub flush_max_chars: usize,
}

impl Default for SessionConfig {
    /// Appendix A: `session_flush_away_s = 30`, `session_flush_max_s = 300`,
    /// `session_flush_max_chars = 6000`.
    fn default() -> Self {
        SessionConfig {
            flush_away_ms: 30_000,
            flush_max_ms: 300_000,
            flush_max_chars: 6_000,
        }
    }
}

struct Open {
    source: SourceFacts,
    started_at: i64,
    ended_at: i64,
    lines: Vec<String>,
    seen: HashSet<u64>,
    chars: usize,
    last_simhash: u64,
    left_at: Option<i64>,
    chars_by_rung: [usize; 3],
    redactions: u32,
}

impl Drop for Open {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        for l in &mut self.lines {
            l.zeroize();
        }
    }
}

fn line_key(l: &str) -> u64 {
    crate::simhash::simhash(l) ^ (l.len() as u64).rotate_left(32)
}

/// Strip volatile counters like "Inbox (3)" so an unread count does not split a session.
pub fn stable_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut chars = title.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '(' {
            let rest: String = chars.clone().take_while(|x| *x != ')').collect();
            if !rest.is_empty() && rest.chars().all(|x| x.is_ascii_digit() || x == '+') {
                for _ in 0..=rest.chars().count() {
                    chars.next();
                }
                continue;
            }
        }
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Default)]
pub struct SessionBook {
    cfg: SessionConfig,
    open: HashMap<SessionKey, Open>,
    focused: Option<SessionKey>,
}

impl SessionBook {
    pub fn new(cfg: SessionConfig) -> Self {
        SessionBook {
            cfg,
            open: HashMap::new(),
            focused: None,
        }
    }

    pub fn open_count(&self) -> usize {
        self.open.len()
    }

    /// Focus moved to another window (or to none / a blocked one).
    pub fn focus(&mut self, key: Option<&SessionKey>, now: i64) {
        if self.focused.as_ref() == key {
            return;
        }
        if let Some(old) = self.focused.take()
            && let Some(o) = self.open.get_mut(&old)
        {
            o.left_at = Some(now);
        }
        if let Some(k) = key {
            if let Some(o) = self.open.get_mut(k) {
                o.left_at = None; // came back within the grace period
            }
            self.focused = Some(k.clone());
        }
    }

    pub fn snapshot(&mut self, s: Snapshot) -> Vec<Flushed> {
        self.focus(Some(&s.key), s.at_ms);
        let h = simhash(s.text.expose());
        let rung_i = s.rung as usize;
        let o = self.open.entry(s.key.clone()).or_insert_with(|| Open {
            source: s.source.clone(),
            started_at: s.at_ms,
            ended_at: s.at_ms,
            lines: Vec::new(),
            seen: HashSet::new(),
            chars: 0,
            last_simhash: u64::MAX ^ h, // force the first snapshot through
            left_at: None,
            chars_by_rung: [0; 3],
            redactions: 0,
        });
        o.ended_at = o.ended_at.max(s.at_ms);
        o.source = s.source;
        if !(o.chars > 0 && same(o.last_simhash, h)) {
            o.last_simhash = h;
            o.redactions += s.redactions;
            for line in s.text.expose().lines().map(str::trim).filter(|l| !l.is_empty()) {
                if o.seen.insert(line_key(line)) {
                    o.chars += line.chars().count() + 1;
                    o.chars_by_rung[rung_i] += line.len();
                    o.lines.push(line.to_owned());
                }
            }
        }
        if o.chars >= self.cfg.flush_max_chars {
            return self.take(&s.key, FlushReason::MaxChars).into_iter().collect();
        }
        Vec::new()
    }

    /// Time-based flushes: away for too long, or open for too long.
    pub fn tick(&mut self, now: i64) -> Vec<Flushed> {
        let due: Vec<(SessionKey, FlushReason)> = self
            .open
            .iter()
            .filter_map(|(k, o)| {
                if o.left_at.is_some_and(|t| now - t >= self.cfg.flush_away_ms) {
                    Some((k.clone(), FlushReason::FocusLeft))
                } else if now - o.started_at >= self.cfg.flush_max_ms {
                    Some((k.clone(), FlushReason::MaxDuration))
                } else {
                    None
                }
            })
            .collect();
        due.into_iter().filter_map(|(k, r)| self.take(&k, r)).collect()
    }

    /// Lock / shutdown: store everything.
    pub fn flush_all(&mut self) -> Vec<Flushed> {
        let keys: Vec<SessionKey> = self.open.keys().cloned().collect();
        let out = keys
            .iter()
            .filter_map(|k| self.take(k, FlushReason::LockOrShutdown))
            .collect();
        self.focused = None;
        out
    }

    /// Pause: forget everything not yet stored (plan §4.3 step 2).
    pub fn drop_all(&mut self) {
        self.open.clear();
        self.focused = None;
    }

    fn take(&mut self, key: &SessionKey, reason: FlushReason) -> Option<Flushed> {
        let mut o = self.open.remove(key)?;
        if self.focused.as_ref() == Some(key) && reason != FlushReason::LockOrShutdown {
            // Still focused: the next snapshot starts a fresh session for the same window.
        }
        if o.lines.is_empty() {
            return None;
        }
        let text = std::mem::take(&mut o.lines).join("\n");
        let rung = match o
            .chars_by_rung
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| **c)
            .map(|(i, _)| i)
        {
            Some(1) => Rung::Ocr,
            Some(2) => Rung::Vlm,
            _ => Rung::Uia,
        };
        Some(Flushed {
            source: o.source.clone(),
            started_at: o.started_at,
            ended_at: o.ended_at,
            simhash: simhash(&text),
            text: Sensitive::new(text),
            rung,
            redactions: o.redactions,
            reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(doc: &str) -> SessionKey {
        SessionKey {
            exe: "chrome.exe".into(),
            hwnd: 1,
            document: Sensitive::new(doc.into()),
        }
    }

    fn snap(doc: &str, text: &str, at: i64) -> Snapshot {
        Snapshot {
            key: key(doc),
            source: SourceFacts {
                exe: "chrome.exe".into(),
                app_name: None,
                title: Sensitive::new(doc.into()),
                url: None,
            },
            text: Sensitive::new(text.into()),
            rung: Rung::Uia,
            redactions: 0,
            at_ms: at,
        }
    }

    #[test]
    fn scrolling_chat_stores_each_line_once() {
        let mut b = SessionBook::default();
        b.snapshot(snap("chat", "Meera: hi\nAsha: hello", 0));
        b.snapshot(snap("chat", "Asha: hello\nMeera: send the deck?", 5_000));
        b.snapshot(snap("chat", "Meera: send the deck?\nAsha: will do by 5pm", 10_000));
        let f = b.flush_all();
        assert_eq!(f.len(), 1);
        assert_eq!(
            f[0].text.expose(),
            "Meera: hi\nAsha: hello\nMeera: send the deck?\nAsha: will do by 5pm"
        );
        assert_eq!((f[0].started_at, f[0].ended_at), (0, 10_000));
        assert_eq!(f[0].reason, FlushReason::LockOrShutdown);
    }

    #[test]
    fn focus_away_flushes_after_thirty_seconds_but_a_quick_return_continues() {
        let mut b = SessionBook::default();
        b.snapshot(snap("mail", "Re: budget\nI'll send it Friday", 0));
        b.focus(Some(&key("other")), 1_000);
        assert!(b.tick(20_000).is_empty());
        b.focus(Some(&key("mail")), 25_000); // back within 30 s
        assert!(b.tick(60_000).is_empty());
        b.focus(None, 61_000);
        let f = b.tick(91_000);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].reason, FlushReason::FocusLeft);
    }

    #[test]
    fn long_sessions_and_large_buffers_flush() {
        let mut b = SessionBook::default();
        b.snapshot(snap("doc", "first line", 0));
        assert_eq!(b.tick(300_000)[0].reason, FlushReason::MaxDuration);
        let mut b = SessionBook::new(SessionConfig {
            flush_max_chars: 50,
            ..Default::default()
        });
        let big: String = (0..10).map(|i| format!("line number {i} with words\n")).collect();
        let f = b.snapshot(snap("doc", &big, 0));
        assert_eq!(f[0].reason, FlushReason::MaxChars);
        assert_eq!(b.open_count(), 0);
    }

    #[test]
    fn pause_drops_without_storing() {
        let mut b = SessionBook::default();
        b.snapshot(snap("doc", "never stored", 0));
        b.drop_all();
        assert!(b.flush_all().is_empty());
    }

    #[test]
    fn unchanged_snapshots_only_extend_time() {
        let mut b = SessionBook::default();
        // A large window where only a clock ticks (one token in ~60 changes).
        let text = "Inbox Starred Snoozed Sent Drafts\nRe: Q3 budget sheet\nFinance flagged that the travel line is \
12% over. Can you send the revised sheet with the travel numbers split by team? Thanks, Ravi\nSure Ravi, I will send \
the revised sheet by Friday. I need the headcount numbers first.\nReply Forward Archive Report spam Delete\n10:05";
        b.snapshot(snap("doc", text, 0));
        b.snapshot(snap("doc", &text.replace("10:05", "10:06"), 15_000));
        let f = b.flush_all();
        assert_eq!(f[0].text.expose(), text, "near-identical snapshot added nothing");
        assert_eq!(f[0].ended_at, 15_000);
    }

    #[test]
    fn titles_lose_unread_counters() {
        assert_eq!(
            stable_title("Inbox (3) - asha@northwind.example - Gmail"),
            "Inbox - asha@northwind.example - Gmail"
        );
        assert_eq!(stable_title("Chat (99+) | Teams"), "Chat | Teams");
        assert_eq!(stable_title("Notes (draft) - Word"), "Notes (draft) - Word");
    }

    #[test]
    fn debug_output_never_contains_text() {
        let mut b = SessionBook::default();
        b.snapshot(snap("doc", "TSRCANARY secret line", 0));
        let f = b.flush_all();
        assert!(!format!("{f:?}").contains("TSRCANARY"));
    }
}
