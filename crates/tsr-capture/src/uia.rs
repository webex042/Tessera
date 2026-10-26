//! Rung 1: UI Automation text with a wall-clock deadline (plan §5.4).
//!
//! All UIA calls run on one dedicated MTA thread ([`DeadlineWorker`]); the caller never waits
//! longer than the deadline (300 ms by default). How a window is read:
//!
//! 1. If the window has a `Document` with `TextPattern` **and no password field anywhere inside
//!    it**, read the document range (≤ 50 000 chars). Stage 0 relied on browsers masking password
//!    values in TextPattern output; this check removes that reliance.
//! 2. Otherwise fetch the control-view subtree in **one** cached round trip
//!    (`ElementFromHandleBuildCache` with `TreeScope_Subtree`) and walk it locally: password
//!    controls and everything under them are skipped structurally, off-screen subtrees are skipped,
//!    caps are 3 000 nodes, depth 40, 50 000 chars. Text is ordered top-to-bottom, left-to-right.
//!
//! [`UiaText`]'s `Debug` output never contains the text.

use std::fmt;
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows::core::Interface;

use crate::deadline::{DeadlineError, DeadlineWorker};

/// The budget is 300 ms caller-observed (plan §19). Windows wakes waiting threads on a ~15.6 ms
/// timer tick, so a 300 ms wait was measured returning after 308–314 ms; 285 ms keeps the
/// caller-observed time inside the budget.
pub const DEFAULT_DEADLINE: Duration = Duration::from_millis(285);
const UIA_CALL_TIMEOUT_MS: u32 = 300;
const MAX_NODES: u32 = 3_000;
const MAX_DEPTH: u32 = 40;
const MAX_CHARS: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    TextPattern,
    Tree,
}

/// Text read from a window. Content: never log it; `Debug` prints sizes only.
#[derive(Clone, PartialEq, Eq)]
pub struct UiaText {
    pub method: Method,
    pub text: String,
    pub nodes: u32,
    pub password_controls_skipped: u32,
    pub truncated: bool,
}

impl fmt::Debug for UiaText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiaText")
            .field("method", &self.method)
            .field("chars", &self.text.chars().count())
            .field("nodes", &self.nodes)
            .field("password_controls_skipped", &self.password_controls_skipped)
            .field("truncated", &self.truncated)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UiaError {
    #[error(transparent)]
    Deadline(#[from] DeadlineError),
    #[error("UIA error {0:#010x}")]
    Com(i32),
    #[error("UIA is not available on the worker thread")]
    Unavailable,
}

impl From<windows::core::Error> for UiaError {
    fn from(e: windows::core::Error) -> Self {
        UiaError::Com(e.code().0)
    }
}

struct WorkerState {
    uia: Option<IUIAutomation>,
}

/// Reads windows through UIA on a dedicated thread with a wall-clock deadline.
pub struct UiaReader {
    worker: DeadlineWorker<isize, Result<UiaText, UiaError>>,
    pub deadline: Duration,
}

impl UiaReader {
    pub fn new() -> std::io::Result<Self> {
        let worker = DeadlineWorker::spawn(
            "tsr-uia",
            || {
                // SAFETY: first COM call on this new thread; MTA as plan §4.1 (T2).
                let uia = unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    CoCreateInstance::<_, IUIAutomation>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()
                };
                if let Some(u) = &uia
                    && let Ok(u2) = u.cast::<IUIAutomation2>()
                {
                    // Per-call limits; the wall-clock limit is the DeadlineWorker.
                    unsafe {
                        let _ = u2.SetConnectionTimeout(UIA_CALL_TIMEOUT_MS);
                        let _ = u2.SetTransactionTimeout(UIA_CALL_TIMEOUT_MS);
                    }
                }
                WorkerState { uia }
            },
            |st: &mut WorkerState, hwnd: isize| {
                let uia = st.uia.as_ref().ok_or(UiaError::Unavailable)?;
                read_window(uia, HWND(hwnd as *mut _))
            },
        )?;
        Ok(UiaReader {
            worker,
            deadline: DEFAULT_DEADLINE,
        })
    }

    /// Read a window. Returns within `self.deadline` (plus scheduling jitter), always.
    pub fn read(&self, hwnd: HWND) -> Result<UiaText, UiaError> {
        self.worker.call(hwnd.0 as isize, self.deadline)?
    }

    /// For diagnostics: wait until an abandoned read has finished; returns how long it took.
    pub fn wait_idle(&self, max: Duration) -> Option<Duration> {
        self.worker.wait_idle(max)
    }
}

fn read_window(uia: &IUIAutomation, hwnd: HWND) -> Result<UiaText, UiaError> {
    // SAFETY: COM calls on objects owned by this worker thread.
    unsafe {
        let root = uia.ElementFromHandle(hwnd)?;
        if let Some(t) = try_text_pattern(uia, &root)? {
            return Ok(t);
        }
        walk_tree(uia, hwnd)
    }
}

unsafe fn try_text_pattern(uia: &IUIAutomation, root: &IUIAutomationElement) -> Result<Option<UiaText>, UiaError> {
    unsafe {
        let doc_cond =
            uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_DocumentControlTypeId.0))?;
        let pw_cond = uia.CreatePropertyCondition(UIA_IsPasswordPropertyId, &VARIANT::from(true))?;
        let candidates = [
            Some(root.clone()),
            root.FindFirst(TreeScope_Descendants, &doc_cond).ok(),
        ];
        for el in candidates.into_iter().flatten() {
            let Ok(tp) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) else {
                continue;
            };
            // A password control anywhere in the document: do not trust the app to mask it.
            if el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(true)
                || el.FindFirst(TreeScope_Subtree, &pw_cond).is_ok()
            {
                return Ok(None);
            }
            let raw = tp.DocumentRange()?.GetText(MAX_CHARS as i32)?.to_string();
            let text = normalise(&raw);
            if !text.is_empty() {
                let truncated = raw.chars().count() >= MAX_CHARS;
                return Ok(Some(UiaText {
                    method: Method::TextPattern,
                    text,
                    nodes: 0,
                    password_controls_skipped: 0,
                    truncated,
                }));
            }
        }
        Ok(None)
    }
}

unsafe fn walk_tree(uia: &IUIAutomation, hwnd: HWND) -> Result<UiaText, UiaError> {
    unsafe {
        let cache = uia.CreateCacheRequest()?;
        for p in [
            UIA_NamePropertyId,
            UIA_IsPasswordPropertyId,
            UIA_IsOffscreenPropertyId,
            UIA_BoundingRectanglePropertyId,
            UIA_ValueValuePropertyId,
        ] {
            cache.AddProperty(p)?;
        }
        cache.AddPattern(UIA_ValuePatternId)?;
        cache.SetTreeScope(TreeScope_Subtree)?;
        cache.SetTreeFilter(&uia.ControlViewCondition()?)?;
        // One cross-process round trip for the whole subtree; the walk below is local.
        let root = uia.ElementFromHandleBuildCache(hwnd, &cache)?;

        let (mut nodes, mut pw, mut chars, mut truncated) = (0u32, 0u32, 0usize, false);
        let mut parts: Vec<(i32, i32, String)> = Vec::new();
        let mut stack: Vec<(IUIAutomationElement, u32)> = vec![(root, 0)];
        while let Some((el, depth)) = stack.pop() {
            if nodes >= MAX_NODES || chars >= MAX_CHARS {
                truncated = true;
                break;
            }
            nodes += 1;
            if el.CachedIsPassword().map(|b| b.as_bool()).unwrap_or(true) {
                pw += 1;
                continue; // skip the control *and* its subtree
            }
            if el.CachedIsOffscreen().map(|b| b.as_bool()).unwrap_or(false) {
                continue;
            }
            let rect = el.CachedBoundingRectangle().unwrap_or_default();
            let name = el.CachedName().map(|b| b.to_string()).unwrap_or_default();
            let value = el
                .GetCachedPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|vp| vp.CachedValue())
                .map(|b| b.to_string())
                .unwrap_or_default();
            for s in [name, value] {
                let s = s.trim();
                if !s.is_empty() {
                    chars += s.len();
                    parts.push((rect.top, rect.left, s.to_owned()));
                }
            }
            if depth < MAX_DEPTH
                && let Ok(children) = el.GetCachedChildren()
            {
                let n = children.Length().unwrap_or(0);
                for i in (0..n).rev() {
                    if let Ok(c) = children.GetElement(i) {
                        stack.push((c, depth + 1));
                    }
                }
            } else if depth >= MAX_DEPTH {
                truncated = true;
            }
        }
        parts.sort_by_key(|(t, l, _)| (*t, *l));
        parts.dedup_by(|a, b| a.2 == b.2);
        let mut text = normalise(&parts.into_iter().map(|p| p.2).collect::<Vec<_>>().join("\n"));
        if text.len() > MAX_CHARS {
            let cut = text.char_indices().nth(MAX_CHARS).map_or(text.len(), |(i, _)| i);
            text.truncate(cut);
            truncated = true;
        }
        Ok(UiaText {
            method: Method::Tree,
            text,
            nodes,
            password_controls_skipped: pw,
            truncated,
        })
    }
}

/// Collapse runs of whitespace inside lines and drop empty lines.
pub fn normalise(s: &str) -> String {
    s.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_text() {
        let t = UiaText {
            method: Method::Tree,
            text: "TSRCANARY secret".into(),
            nodes: 3,
            password_controls_skipped: 1,
            truncated: false,
        };
        let d = format!("{t:?}");
        assert!(!d.contains("TSRCANARY") && d.contains("chars: 16"), "{d}");
    }

    #[test]
    fn normalise_collapses_whitespace() {
        assert_eq!(normalise("  a   b \n\n\t c  "), "a b\nc");
    }

    /// REAL: the desktop shell window always exists; reading it must return within the deadline.
    #[test]
    fn real_read_of_the_shell_tray_respects_the_deadline() {
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
        use windows::core::w;
        let Ok(tray) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else {
            return;
        };
        let r = UiaReader::new().unwrap();
        let t0 = std::time::Instant::now();
        let out = r.read(tray);
        let took = t0.elapsed();
        println!("REAL Shell_TrayWnd: {out:?} in {took:?}");
        assert!(
            took <= Duration::from_millis(300),
            "caller-observed budget is 300 ms: {took:?}"
        );
    }
}
