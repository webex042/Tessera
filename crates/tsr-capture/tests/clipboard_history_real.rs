//! REAL check: a draft put on the clipboard by Tessara does not enter clipboard history (Win+V).
//! Compares only the *id* of the newest history item (no content is read), before and after.
//! Positive control: an ordinary copy does change the newest item; that one control entry is
//! deleted from history again, after confirming it is the control text. The user's clipboard
//! contents are saved first and put back at the end. Skipped when clipboard history is off.
//! Cloud clipboard upload cannot be observed from here: only the exclusion format is checked.
//!     cargo test -p tsr-capture --test clipboard_history_real -- --ignored --nocapture
#![cfg(windows)]

use std::time::Duration;

use tsr_win::clipboard::{inspect, restore, restore_unconditionally, set_ordinary_text_for_test, set_private_text};
use windows::ApplicationModel::DataTransfer::Clipboard;

fn newest() -> Option<String> {
    let r = Clipboard::GetHistoryItemsAsync().ok()?.join().ok()?;
    let items = r.Items().ok()?;
    if items.Size().ok()? == 0 {
        return None;
    }
    items.GetAt(0).ok()?.Id().ok().map(|h| h.to_string())
}

#[test]
#[ignore = "real desktop: uses the clipboard and clipboard history (contents saved and restored)"]
fn private_drafts_stay_out_of_clipboard_history() {
    if !Clipboard::IsHistoryEnabled().unwrap_or(false) {
        eprintln!("SKIPPED: clipboard history (Win+V) is turned off on this machine");
        return;
    }
    let before = newest();
    // 1. Private set: history must not change.
    let g = set_private_text("tsr-history-private-canary").expect("set");
    assert!(inspect().unwrap().1, "exclusion formats present");
    std::thread::sleep(Duration::from_millis(1500));
    let after_private = newest();
    println!(
        "REAL newest history item unchanged after a private set: {}",
        after_private == before
    );
    // 2. Positive control: an ordinary set must change it.
    set_ordinary_text_for_test("tsr-history-control").unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let after_control = newest();
    let control_seen = after_control != after_private;
    println!("REAL ordinary copy reached history (control): {control_seen}");
    // Remove only the control entry, after checking it is ours.
    if control_seen
        && let Ok(r) = Clipboard::GetHistoryItemsAsync().and_then(|op| op.join())
        && let Ok(items) = r.Items()
        && let Ok(item) = items.GetAt(0)
    {
        let is_ours = item
            .Content()
            .and_then(|c| c.GetTextAsync())
            .and_then(|op| op.join())
            .map(|t| t == "tsr-history-control")
            .unwrap_or(false);
        if is_ours {
            let _ = Clipboard::DeleteItemFromHistory(&item);
        }
    }
    restore_unconditionally(g).unwrap();
    let _ = restore; // the conditional restore is covered by insert_real
    assert!(
        control_seen,
        "the control must reach history, or this check proves nothing"
    );
    assert_eq!(after_private, before, "a private draft entered clipboard history");
}
