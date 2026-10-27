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
//!
//! The same worker also reads **browser facts** for the blocklist ([`BrowserFacts`]): the address
//! bar URL and whether the window is private. Verified against Chrome and Edge (2026-10-27):
//! Chrome's Incognito window title is identical to a normal one, so the private state is read
//! from the `BrowserRootView` element's name (`... - Google Chrome (Incognito)`,
//! `... - Microsoft Edge (InPrivate)`) and the profile button. Anything not positively "normal" is
//! [`PrivateState::Unknown`], which the blocklist blocks (fail closed).

use std::fmt;
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows::core::Interface;

use tsr_core::Sensitive;
use tsr_guard::blocklist::PrivateState;

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

/// What the blocklist needs from a browser window. `url` is content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserFacts {
    pub url: Option<Sensitive<String>>,
    pub private: PrivateState,
}

enum Request {
    Text(isize),
    Browser(isize),
    Focus,
}

enum Response {
    Text(Result<UiaText, UiaError>),
    Browser(Result<BrowserFacts, UiaError>),
    Focus(Result<FocusTarget, UiaError>),
}

/// The element that has keyboard focus, as the draft assistant needs it (plan §12.1). Contains
/// no text from the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusTarget {
    /// Foreground top-level window.
    pub hwnd: isize,
    pub pid: u32,
    /// UIA RuntimeId: identifies this element while it exists.
    pub runtime_id: Vec<i32>,
    /// A password field, or one whose password state could not be read (fail closed).
    pub is_password: bool,
    /// Enabled, focusable, and an edit/document or a writable value.
    pub editable: bool,
}

impl FocusTarget {
    /// The draft may be inserted here.
    pub fn insertable(&self) -> bool {
        self.editable && !self.is_password && !self.runtime_id.is_empty()
    }

    /// Same window and same element as `other`.
    pub fn same_as(&self, other: &FocusTarget) -> bool {
        self.hwnd == other.hwnd && self.runtime_id == other.runtime_id
    }
}

/// Reads windows through UIA on a dedicated thread with a wall-clock deadline.
pub struct UiaReader {
    worker: DeadlineWorker<Request, Response>,
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
            |st: &mut WorkerState, req: Request| {
                let uia = st.uia.as_ref().ok_or(UiaError::Unavailable);
                match req {
                    Request::Text(h) => Response::Text(uia.and_then(|u| read_window(u, HWND(h as *mut _)))),
                    Request::Browser(h) => Response::Browser(uia.and_then(|u| browser_facts(u, HWND(h as *mut _)))),
                    Request::Focus => Response::Focus(uia.and_then(read_focus)),
                }
            },
        )?;
        Ok(UiaReader {
            worker,
            deadline: DEFAULT_DEADLINE,
        })
    }

    /// Read a window. Returns within `self.deadline` (plus scheduling jitter), always.
    pub fn read(&self, hwnd: HWND) -> Result<UiaText, UiaError> {
        match self.worker.call(Request::Text(hwnd.0 as isize), self.deadline)? {
            Response::Text(r) => r,
            _ => Err(UiaError::Unavailable),
        }
    }

    /// Address-bar URL and private state of a Chromium-based browser window. On any failure
    /// (including the deadline) the caller must treat the window as private.
    pub fn browser(&self, hwnd: HWND) -> Result<BrowserFacts, UiaError> {
        match self.worker.call(Request::Browser(hwnd.0 as isize), self.deadline)? {
            Response::Browser(r) => r,
            _ => Err(UiaError::Unavailable),
        }
    }

    /// The focused element (draft target). Within the deadline, like every UIA call here.
    pub fn focus(&self) -> Result<FocusTarget, UiaError> {
        match self.worker.call(Request::Focus, self.deadline)? {
            Response::Focus(r) => r,
            _ => Err(UiaError::Unavailable),
        }
    }

    /// For diagnostics: wait until an abandoned read has finished; returns how long it took.
    pub fn wait_idle(&self, max: Duration) -> Option<Duration> {
        self.worker.wait_idle(max)
    }
}

fn read_focus(uia: &IUIAutomation) -> Result<FocusTarget, UiaError> {
    use windows::Win32::System::Ole::{SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    // SAFETY: COM calls on this worker's MTA thread; the SAFEARRAY from GetRuntimeId is read
    // within its bounds and destroyed here.
    unsafe {
        let hwnd = GetForegroundWindow();
        let el = uia.GetFocusedElement()?;
        let sa = el.GetRuntimeId()?;
        let mut runtime_id = Vec::new();
        if !sa.is_null() {
            let lb = SafeArrayGetLBound(sa, 1).unwrap_or(0);
            let ub = SafeArrayGetUBound(sa, 1).unwrap_or(-1);
            for i in lb..=ub {
                let mut v = 0i32;
                if SafeArrayGetElement(sa, &i, &mut v as *mut i32 as *mut _).is_ok() {
                    runtime_id.push(v);
                }
            }
            let _ = SafeArrayDestroy(sa);
        }
        let flag = |r: windows::core::Result<windows::core::BOOL>| r.map(|b| b.as_bool());
        // Unknown password state counts as a password field.
        let is_password = flag(el.CurrentIsPassword()).unwrap_or(true);
        let enabled = flag(el.CurrentIsEnabled()).unwrap_or(false);
        let focusable = flag(el.CurrentIsKeyboardFocusable()).unwrap_or(false);
        let ct = el.CurrentControlType().map(|c| c.0).unwrap_or(0);
        let value_writable = el
            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .and_then(|v| v.CurrentIsReadOnly())
            .map(|ro| !ro.as_bool())
            .unwrap_or(false);
        let text_kind = ct == UIA_EditControlTypeId.0 || ct == UIA_DocumentControlTypeId.0;
        Ok(FocusTarget {
            hwnd: hwnd.0 as isize,
            pid: el.CurrentProcessId().unwrap_or(0) as u32,
            runtime_id,
            is_password,
            editable: enabled && focusable && (value_writable || text_kind),
        })
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

/// Product names that end a *normal* window's root-view name, matched after removing zero-width
/// characters (Edge writes "Microsoft<U+200B> Edge" in its window title).
const NORMAL_SUFFIXES: &[&str] = &[" - Google Chrome", " - Microsoft Edge", " - Chromium"];
/// Profile-button names of private windows (English UI; other languages fall to Unknown).
const PRIVATE_AVATARS: &[&str] = &["Incognito", "InPrivate browsing", "InPrivate"];

fn strip_zero_width(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(*c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}'))
        .collect()
}

/// Decide the private state from the root-view and profile-button names. Fails closed: only a
/// root-view name ending exactly in a known product name counts as "not private".
pub fn classify_private(root_name: Option<&str>, avatar_name: Option<&str>) -> PrivateState {
    if avatar_name
        .map(strip_zero_width)
        .is_some_and(|a| PRIVATE_AVATARS.contains(&a.trim()))
    {
        return PrivateState::Private;
    }
    let Some(root) = root_name.map(strip_zero_width) else {
        return PrivateState::Unknown;
    };
    let root = root.trim_end();
    if NORMAL_SUFFIXES.iter().any(|s| root.ends_with(s)) {
        return PrivateState::NotPrivate;
    }
    // "... - Google Chrome (Incognito)" in any UI language.
    if root.ends_with(')')
        && NORMAL_SUFFIXES
            .iter()
            .any(|s| root.contains(s.trim_start_matches(" - ")))
    {
        return PrivateState::Private;
    }
    PrivateState::Unknown
}

/// Browser UI elements are found by a bounded walk of the control view that never enters the
/// page (`Document`) subtree: a "find descendant" search would crawl the whole web page's
/// accessibility tree and blow the per-call UIA timeout (seen on Chrome, 2026-10-27).
const BROWSER_WALK_MAX_NODES: u32 = 300;
const BROWSER_WALK_MAX_DEPTH: u32 = 12;

/// The address bar's text: the Value pattern, with the LegacyIAccessible value as a fallback.
/// TextPattern is deliberately *not* used: on an empty box it returns the placeholder
/// ("Address and search bar"), which must never be taken for a URL.
unsafe fn omnibox_value(el: &IUIAutomationElement) -> Option<String> {
    let non_empty = |s: String| (!s.trim().is_empty()).then_some(s);
    unsafe {
        el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .ok()
            .and_then(|vp| vp.CurrentValue().ok())
            .map(|b| b.to_string())
            .and_then(non_empty)
            .or_else(|| {
                el.GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(UIA_LegacyIAccessiblePatternId)
                    .ok()
                    .and_then(|lp| lp.CurrentValue().ok())
                    .map(|b| b.to_string())
                    .and_then(non_empty)
            })
    }
}

fn browser_facts(uia: &IUIAutomation, hwnd: HWND) -> Result<BrowserFacts, UiaError> {
    // SAFETY: COM calls on objects owned by this worker thread.
    unsafe {
        let root = uia.ElementFromHandle(hwnd)?;
        let walker = uia.ControlViewWalker()?;
        let (mut root_name, mut avatar_name, mut url) = (None::<String>, None::<String>, None::<String>);
        let mut nodes = 0u32;
        let mut stack: Vec<(IUIAutomationElement, u32)> = vec![(root, 0)];
        while let Some((el, depth)) = stack.pop() {
            nodes += 1;
            if nodes > BROWSER_WALK_MAX_NODES || (root_name.is_some() && avatar_name.is_some() && url.is_some()) {
                break;
            }
            let class = el.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
            match class.as_str() {
                "BrowserRootView" => root_name = el.CurrentName().ok().map(|b| b.to_string()),
                "AvatarToolbarButton" | "EdgeAvatarToolbarButton" => {
                    avatar_name = el.CurrentName().ok().map(|b| b.to_string())
                }
                "OmniboxViewViews" => {
                    url = omnibox_value(&el);
                    continue;
                }
                _ => {}
            }
            if depth >= BROWSER_WALK_MAX_DEPTH || el.CurrentControlType().is_ok_and(|t| t == UIA_DocumentControlTypeId)
            {
                continue; // never walk into the web page
            }
            let mut children = Vec::new();
            let mut c = walker.GetFirstChildElement(&el).ok();
            while let Some(ch) = c {
                c = walker.GetNextSiblingElement(&ch).ok();
                children.push(ch);
            }
            for ch in children.into_iter().rev() {
                stack.push((ch, depth + 1));
            }
        }
        Ok(BrowserFacts {
            url: url.filter(|u| !u.trim().is_empty()).map(Sensitive::new),
            private: classify_private(root_name.as_deref(), avatar_name.as_deref()),
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

    /// Names observed on real Chrome/Edge windows (2026-10-27), plus fail-closed cases.
    #[test]
    fn private_state_classification() {
        use PrivateState::*;
        assert_eq!(
            classify_private(Some("TSR fixture chat - Google Chrome"), Some("Sign in to Chrome?")),
            NotPrivate
        );
        assert_eq!(
            classify_private(Some("TSR fixture chat - Google Chrome (Incognito)"), Some("Incognito")),
            Private
        );
        assert_eq!(
            classify_private(Some("TSR fixture chat - Microsoft Edge"), Some("Personal Profile")),
            NotPrivate
        );
        assert_eq!(
            classify_private(
                Some("TSR fixture chat - Microsoft Edge (InPrivate)"),
                Some("InPrivate browsing")
            ),
            Private
        );
        assert_eq!(
            classify_private(Some("Seite - Google Chrome (Inkognito)"), Some("Inkognito")),
            Private,
            "other languages"
        );
        assert_eq!(
            classify_private(Some("Page - Brave"), None),
            Unknown,
            "unverified browser"
        );
        assert_eq!(classify_private(None, None), Unknown);
        assert_eq!(
            classify_private(Some("Inbox (3) - Google Chrome"), None),
            NotPrivate,
            "page-title brackets"
        );
        assert_eq!(
            classify_private(Some("x - Google Chrome"), Some("Incognito")),
            Private,
            "avatar wins"
        );
        assert_eq!(classify_private(Some("x - Microsoft\u{200B} Edge"), None), NotPrivate);
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
