//! The capture core (plan §4.2 Gatekeeper + CaptureActor + SessionActor, §5).
//!
//! Platform-neutral: the OS side is a [`Platform`], storage is a [`Sink`], time is passed in.
//! Each [`CaptureCore::tick`]:
//! 1. flushes sessions that are due;
//! 2. evaluates the gates (pause, lock, suspend, idle, fullscreen, battery);
//! 3. when a check is due (cadence §5.3, or 750 ms after a focus/title change), looks at the
//!    foreground window: browser facts → blocklist (§6.1) → rung 1 UIA, rung 2 OCR on timeout or
//!    failure (§5.4) → password-screen check for OCR text (§6.2) → **redaction** of text, title and
//!    URL (§6.3) → session book (§5.6).
//!
//! Capture starts **paused** (plan D12). Pause drops unstored session text; lock, suspend and
//! shutdown flush it. Blocked windows produce one content-free audit event per transition.

use std::collections::HashMap;

use tsr_core::Sensitive;
use tsr_guard::blocklist::{Blocklist, Decision, PrivateState, WindowFacts, is_browser};
use tsr_guard::redact::{self, Options};

use crate::fallback::{UiaFallback, UiaOutcome};
use crate::gate::{Blocker, Gate, GateInputs, UserActivity, evaluate};
use crate::session::{Flushed, Rung, SessionBook, SessionConfig, SessionKey, Snapshot, SourceFacts, stable_title};

#[derive(Debug, Clone)]
pub struct Foreground {
    pub hwnd: isize,
    pub pid: u32,
    pub exe: String,
    pub exe_path: String,
    /// Raw window title: content, redacted before it is used for anything stored.
    pub title: Sensitive<String>,
    pub minimized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsState {
    pub idle_ms: u64,
    pub activity: UserActivity,
}

#[derive(Debug)]
pub enum ReadOutcome {
    Text(Sensitive<String>),
    TimedOut,
    Failed,
}

#[derive(Debug, Clone)]
pub struct BrowserInfo {
    pub url: Option<Sensitive<String>>,
    pub private: PrivateState,
}

/// The OS side. The Windows implementation is `platform_win::WinPlatform`.
pub trait Platform {
    fn foreground(&mut self) -> Option<Foreground>;
    fn os_state(&mut self) -> OsState;
    /// `None` if the facts could not be read (treated as private: fail closed).
    fn browser(&mut self, w: &Foreground) -> Option<BrowserInfo>;
    fn read_uia(&mut self, w: &Foreground) -> ReadOutcome;
    fn read_ocr(&mut self, w: &Foreground) -> ReadOutcome;
}

/// Where finished sessions and block events go (the store writer in the app).
pub trait Sink {
    fn store(&mut self, item: Flushed);
    fn blocked(&mut self, decision: &Decision, at_ms: i64);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    ForegroundChanged,
    TitleChanged,
    Locked,
    Unlocked,
    Suspending,
    Resumed,
    PowerSource { on_ac: bool },
    BatterySaver { on: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Pause,
    Resume,
    Shutdown,
    Os(Signal),
}

/// Plan §5.3 / Appendix A cadence.
#[derive(Debug, Clone, Copy)]
pub struct Cadence {
    pub debounce_ms: i64,
    pub active_check_ms: i64,
    pub passive_check_ms: i64,
    /// No input for longer than this (but less than the idle gate) = passive.
    pub passive_after_ms: u64,
    pub ocr_min_active_ms: i64,
    pub ocr_min_passive_ms: i64,
}

impl Default for Cadence {
    fn default() -> Self {
        Cadence {
            debounce_ms: 750,
            active_check_ms: 5_000,
            passive_check_ms: 20_000,
            passive_after_ms: 30_000,
            ocr_min_active_ms: 15_000,
            ocr_min_passive_ms: 60_000,
        }
    }
}

/// Content-free status for the notch and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub checks: u64,
    pub snapshots: u64,
    pub items_flushed: u64,
    pub blocked_events: u64,
    pub uia_timeouts: u64,
    pub ocr_reads: u64,
    pub password_screens_skipped: u64,
}

pub struct CaptureCore<P: Platform, S: Sink> {
    pub platform: P,
    pub sink: S,
    blocklist: Blocklist,
    redact_opts: Options,
    sessions: SessionBook,
    fallback: UiaFallback<isize>,
    cadence: Cadence,
    paused: bool,
    locked: bool,
    suspended: bool,
    on_battery: bool,
    battery_saver: bool,
    next_check_at: i64,
    last_ocr_at: HashMap<isize, i64>,
    last_blocked: Option<(isize, Option<String>)>,
    /// Diagnostics/tests only: capture nothing but these image names.
    pub only_exes: Option<Vec<String>>,
    pub stats: Stats,
    pub last_gate: Option<Gate>,
}

impl<P: Platform, S: Sink> CaptureCore<P, S> {
    pub fn new(platform: P, sink: S, blocklist: Blocklist, redact_opts: Options) -> Self {
        CaptureCore {
            platform,
            sink,
            blocklist,
            redact_opts,
            sessions: SessionBook::new(SessionConfig::default()),
            fallback: UiaFallback::default(),
            cadence: Cadence::default(),
            paused: true, // D12: nothing is captured until the user presses Start
            locked: false,
            suspended: false,
            on_battery: false,
            battery_saver: false,
            next_check_at: 0,
            last_ocr_at: HashMap::new(),
            last_blocked: None,
            only_exes: None,
            stats: Stats::default(),
            last_gate: None,
        }
    }

    pub fn with_session_config(mut self, cfg: SessionConfig) -> Self {
        self.sessions = SessionBook::new(cfg);
        self
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    fn store_all(&mut self, items: Vec<Flushed>) {
        for f in items {
            self.stats.items_flushed += 1;
            self.sink.store(f);
        }
    }

    /// Apply a control message. Returns `false` on shutdown.
    pub fn handle(&mut self, c: Control, now: i64) -> bool {
        match c {
            Control::Pause => {
                self.paused = true;
                self.sessions.drop_all(); // never stored, by design
            }
            Control::Resume => {
                self.paused = false;
                self.next_check_at = now;
            }
            Control::Shutdown => {
                let f = self.sessions.flush_all();
                self.store_all(f);
                return false;
            }
            Control::Os(s) => match s {
                // Look again shortly after the change settles (debounce §5.3), not immediately:
                // transient windows (alt-tab, splash screens) are not worth a read.
                Signal::ForegroundChanged | Signal::TitleChanged => self.next_check_at = now + self.cadence.debounce_ms,
                Signal::Locked | Signal::Suspending => {
                    if s == Signal::Locked {
                        self.locked = true;
                    } else {
                        self.suspended = true;
                    }
                    let f = self.sessions.flush_all();
                    self.store_all(f);
                }
                Signal::Unlocked => {
                    self.locked = false;
                    self.next_check_at = now + self.cadence.debounce_ms;
                }
                Signal::Resumed => {
                    self.suspended = false;
                    self.next_check_at = now + self.cadence.debounce_ms;
                }
                Signal::PowerSource { on_ac } => self.on_battery = !on_ac,
                Signal::BatterySaver { on } => self.battery_saver = on,
            },
        }
        true
    }

    pub fn tick(&mut self, now: i64) {
        let due = self.sessions.tick(now);
        self.store_all(due);

        let os = self.platform.os_state();
        let gate = evaluate(&GateInputs {
            paused: self.paused,
            locked: self.locked,
            suspended: self.suspended,
            idle_ms: os.idle_ms,
            activity: os.activity,
            on_battery: self.on_battery,
            battery_saver: self.battery_saver,
        });
        self.last_gate = Some(gate);
        if let Some(b) = gate.blocked {
            if !matches!(b, Blocker::Paused) {
                // Not looking at the screen any more: let open sessions time out normally.
                self.sessions.focus(None, now);
            }
            return;
        }
        if now < self.next_check_at {
            return;
        }
        let passive = os.idle_ms >= self.cadence.passive_after_ms;
        let interval = if passive {
            self.cadence.passive_check_ms
        } else {
            self.cadence.active_check_ms
        };
        self.next_check_at = now + interval * gate.cadence_multiplier as i64;
        self.stats.checks += 1;
        self.check(now, passive, gate.cadence_multiplier as i64);
    }

    fn check(&mut self, now: i64, passive: bool, mult: i64) {
        let Some(fg) = self.platform.foreground().filter(|w| !w.minimized) else {
            self.sessions.focus(None, now);
            return;
        };
        if let Some(only) = &self.only_exes
            && !only.iter().any(|e| e.eq_ignore_ascii_case(&fg.exe))
        {
            self.sessions.focus(None, now);
            return;
        }

        // Browser facts first: the blocklist needs URL and private state (fail closed).
        let browser = if is_browser(&fg.exe) {
            Some(self.platform.browser(&fg).unwrap_or(BrowserInfo {
                url: None,
                private: PrivateState::Unknown,
            }))
        } else {
            None
        };
        let url_raw = browser
            .as_ref()
            .and_then(|b| b.url.as_ref())
            .map(|u| u.expose().as_str());
        let decision = self.blocklist.check(&WindowFacts {
            exe: &fg.exe,
            exe_path: Some(&fg.exe_path),
            title: fg.title.expose(),
            url: url_raw,
            private: browser.as_ref().map_or(PrivateState::NotPrivate, |b| b.private),
        });
        if let Decision::Block { rule_key, .. } = &decision {
            let this = (fg.hwnd, rule_key.clone());
            if self.last_blocked.as_ref() != Some(&this) {
                self.stats.blocked_events += 1;
                self.sink.blocked(&decision, now);
                self.last_blocked = Some(this);
            }
            self.sessions.focus(None, now);
            return;
        }
        self.last_blocked = None;

        // Rung 1, then rung 2.
        let mut rung = Rung::Uia;
        let mut text = None;
        if self.fallback.allow_uia(&fg.hwnd, now) {
            match self.platform.read_uia(&fg) {
                ReadOutcome::Text(t) if !t.expose().trim().is_empty() => {
                    self.fallback.record(&fg.hwnd, UiaOutcome::Ok, now);
                    text = Some(t);
                }
                ReadOutcome::TimedOut => {
                    self.stats.uia_timeouts += 1;
                    self.fallback.record(&fg.hwnd, UiaOutcome::TimedOut, now);
                }
                _ => self.fallback.record(&fg.hwnd, UiaOutcome::Failed, now),
            }
        }
        if text.is_none() {
            let min_gap = if passive {
                self.cadence.ocr_min_passive_ms
            } else {
                self.cadence.ocr_min_active_ms
            } * mult;
            let last = self.last_ocr_at.get(&fg.hwnd).copied();
            if last.is_some_and(|t| now - t < min_gap) {
                return; // OCR is expensive; keep the previous snapshot for now
            }
            self.last_ocr_at.insert(fg.hwnd, now);
            if self.last_ocr_at.len() > 256 {
                self.last_ocr_at.retain(|_, t| now - *t < 600_000);
            }
            if let ReadOutcome::Text(t) = self.platform.read_ocr(&fg) {
                self.stats.ocr_reads += 1;
                // Rung 1 could not vouch for password fields here, so check the pixels' text.
                if crate::ocr_guard::looks_like_password_screen(t.expose()) {
                    self.stats.password_screens_skipped += 1;
                    self.sessions.focus(None, now);
                    return;
                }
                rung = Rung::Ocr;
                text = Some(t);
            }
        }
        let Some(text) = text else { return };

        // Redact before anything enters a session buffer (§6.3 a).
        let red = redact::redact(text.expose(), self.redact_opts);
        drop(text);
        let title = redact::redact(fg.title.expose(), self.redact_opts);
        let url = url_raw.map(|u| redact::redact(u, self.redact_opts));
        let redactions = red.total() + title.total() + url.as_ref().map_or(0, |u| u.total());
        let document = match &url {
            Some(u) => u.text.clone(),
            None => stable_title(&title.text),
        };
        let key = SessionKey {
            exe: fg.exe.to_ascii_lowercase(),
            hwnd: fg.hwnd,
            document: Sensitive::new(document),
        };
        let source = SourceFacts {
            exe: fg.exe.clone(),
            app_name: None,
            title: Sensitive::new(title.text),
            url: url.map(|u| Sensitive::new(u.text)),
        };
        self.stats.snapshots += 1;
        let flushed = self.sessions.snapshot(Snapshot {
            key,
            source,
            text: Sensitive::new(red.text),
            rung,
            redactions,
            at_ms: now,
        });
        self.store_all(flushed);
    }
}
