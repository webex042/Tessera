//! Rung 1: UI Automation text (plan §5.4).

use std::time::Instant;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows::core::Interface;

/// Caps from §5.4: 300 ms connection/transaction timeout, 3 000 nodes, 50 000 chars.
const UIA_TIMEOUT_MS: u32 = 300;
const MAX_NODES: i32 = 3_000;
const MAX_CHARS: i32 = 50_000;

#[derive(Debug, Default)]
pub struct UiaResult {
    pub method: &'static str,
    pub ms: u128,
    pub nodes: i32,
    pub password_nodes_skipped: u32,
    pub text: String,
    pub error: Option<String>,
}

pub struct Uia {
    uia: IUIAutomation,
}

impl Uia {
    pub fn new() -> windows::core::Result<Self> {
        // SAFETY: COM is initialised (MTA) by the caller before this runs.
        let uia: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
        if let Ok(u2) = uia.cast::<IUIAutomation2>() {
            // SAFETY: plain setters on a live COM object.
            unsafe {
                u2.SetConnectionTimeout(UIA_TIMEOUT_MS)?;
                u2.SetTransactionTimeout(UIA_TIMEOUT_MS)?;
            }
        }
        Ok(Self { uia })
    }

    pub fn read(&self, hwnd: HWND) -> UiaResult {
        let t0 = Instant::now();
        let mut r = match self.read_inner(hwnd) {
            Ok(r) => r,
            Err(e) => UiaResult {
                method: "failed",
                error: Some(format!("{:#010x}", e.code().0)),
                ..Default::default()
            },
        };
        r.ms = t0.elapsed().as_millis();
        r
    }

    fn read_inner(&self, hwnd: HWND) -> windows::core::Result<UiaResult> {
        // SAFETY: all calls below are COM calls on objects owned by this function.
        unsafe {
            let root = self.uia.ElementFromHandle(hwnd)?;

            // Step 2: a Document (or the root) exposing TextPattern gives the cleanest text.
            let doc_cond = self
                .uia
                .CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_DocumentControlTypeId.0))?;
            let candidates = [
                Some(root.clone()),
                root.FindFirst(TreeScope_Descendants, &doc_cond).ok(),
            ];
            for el in candidates.into_iter().flatten() {
                if el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(true) {
                    continue;
                }
                if let Ok(tp) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                    let text = tp.DocumentRange()?.GetText(MAX_CHARS)?.to_string();
                    if !text.trim().is_empty() {
                        return Ok(UiaResult {
                            method: "text_pattern",
                            text: normalise(&text),
                            ..Default::default()
                        });
                    }
                }
            }

            // Step 3: walk the control view with one cached round trip.
            let cache = self.uia.CreateCacheRequest()?;
            for p in [
                UIA_NamePropertyId,
                UIA_ControlTypePropertyId,
                UIA_IsPasswordPropertyId,
                UIA_IsOffscreenPropertyId,
                UIA_BoundingRectanglePropertyId,
                UIA_ValueValuePropertyId,
            ] {
                cache.AddProperty(p)?;
            }
            // Cache the Value pattern too, so its typed CachedValue() needs no extra round trip.
            cache.AddPattern(UIA_ValuePatternId)?;
            let cond = self.uia.ControlViewCondition()?;
            let all = root.FindAllBuildCache(TreeScope_Descendants, &cond, &cache)?;
            let total = all.Length()?;
            let n = total.min(MAX_NODES);

            let mut pw = 0u32;
            let mut parts: Vec<(i32, i32, String)> = Vec::new();
            for i in 0..n {
                let e = all.GetElement(i)?;
                // Step 4: never read password fields. FindAll flattens the tree, so a password
                // control's own subtree cannot be skipped by structure here; password edits
                // are leaves in every app tested so far. The Stage 1 walker uses TreeWalker.
                if e.CachedIsPassword().map(|b| b.as_bool()).unwrap_or(true) {
                    pw += 1;
                    continue;
                }
                if e.CachedIsOffscreen().map(|b| b.as_bool()).unwrap_or(false) {
                    continue;
                }
                let rect = e.CachedBoundingRectangle().unwrap_or_default();
                let name = e.CachedName().map(|b| b.to_string()).unwrap_or_default();
                let value = e
                    .GetCachedPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    .and_then(|vp| vp.CachedValue())
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                for s in [name, value] {
                    let s = s.trim().to_owned();
                    if !s.is_empty() {
                        parts.push((rect.top, rect.left, s));
                    }
                }
            }
            // Step 5: reading order = top, then left; drop exact repeats (containers often
            // repeat their children's names).
            parts.sort_by_key(|(t, l, _)| (*t, *l));
            parts.dedup_by(|a, b| a.2 == b.2);
            let text = parts.into_iter().map(|p| p.2).collect::<Vec<_>>().join("\n");
            Ok(UiaResult {
                method: "tree",
                nodes: total,
                password_nodes_skipped: pw,
                text: normalise(&text),
                ..Default::default()
            })
        }
    }
}

/// Collapse runs of spaces/tabs inside lines and drop empty lines.
fn normalise(s: &str) -> String {
    s.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
