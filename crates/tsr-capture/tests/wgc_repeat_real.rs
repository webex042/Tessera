//! REAL check: Windows.Graphics.Capture can be used repeatedly in one process. Rung 3 captures
//! one frame per VLM job, so a crash on the second capture would take the app down. Captures
//! the primary monitor three times; frames are only measured, never saved.
//!     cargo test -p tsr-capture --test wgc_repeat_real -- --ignored --nocapture
#![cfg(windows)]

use tsr_capture::ocr::capture_monitor_frame;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};

#[test]
#[ignore = "real desktop: captures the primary monitor"]
fn monitor_capture_three_times_in_one_process() {
    // SAFETY: plain monitor lookup.
    let m = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    for i in 0..3 {
        let f = capture_monitor_frame(m.0 as isize).expect("capture");
        assert!(f.width > 0 && f.height > 0);
        eprintln!("capture {i}: {}x{}", f.width, f.height);
    }
}
