//! The notch window (plan §13.1–13.2): one transparent, always-on-top, capture-excluded window
//! resized per state and kept top-centre of the monitor under the cursor.
//!
//! Resize order (plan §13.2): to expand, the window grows first and the UI animates in; to
//! collapse, the UI animates out first and then asks for the smaller size. Peeks never take
//! focus; only an explicit user action (click, hotkey) focuses the expanded panel.

use serde::{Deserialize, Serialize};
use tauri::{LogicalSize, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    Pill,
    Peek,
    Expanded,
    Draft,
    Hidden,
}

impl Panel {
    /// Size in device-independent pixels.
    pub fn size(self) -> (f64, f64) {
        match self {
            Panel::Pill => (220.0, 34.0),
            Panel::Peek => (380.0, 92.0),
            Panel::Expanded => (760.0, 520.0),
            Panel::Draft => (640.0, 360.0),
            Panel::Hidden => (220.0, 34.0),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pill" => Panel::Pill,
            "peek" => Panel::Peek,
            "expanded" => Panel::Expanded,
            "draft" => Panel::Draft,
            "hidden" => Panel::Hidden,
            _ => return None,
        })
    }
}

pub const TOP_MARGIN: f64 = 8.0;

/// The window's fixed logical size: the largest panel (Expanded).
pub const FRAME: (f64, f64) = (760.0, 520.0);

/// Physical `(x, width, height)` of a `w × h` logical panel centred at the top of a frame `fw`
/// logical pixels wide.
pub fn region(fw: f64, w: f64, h: f64, scale: f64) -> (i32, i32, i32) {
    let rw = (w * scale).round() as i32;
    let x = (((fw * scale).round() as i32) - rw) / 2;
    (x, rw, (h * scale).round() as i32)
}

/// Physical top-left and size of a panel `w` logical pixels wide, centred at the top of a
/// monitor with physical origin `(mon_x, mon_y)`, physical width `mon_w` and DPI `scale`.
/// Everything is physical so mixed-DPI setups land on the right monitor: a logical position
/// would be converted with the window's *current* monitor scale, not the target's.
pub fn top_centre(mon_x: i32, mon_y: i32, mon_w: u32, scale: f64, w: f64) -> (i32, i32) {
    let w_phys = (w * scale).round() as i64;
    let x = i64::from(mon_x) + (i64::from(mon_w) - w_phys) / 2;
    (x as i32, mon_y + (TOP_MARGIN * scale).round() as i32)
}

/// The monitor under the cursor, else the window's current one, else the primary.
fn target_monitor<R: Runtime>(win: &WebviewWindow<R>) -> Option<tauri::Monitor> {
    let app = win.app_handle();
    if let Ok(p) = app.cursor_position()
        && let Ok(Some(m)) = app.monitor_from_point(p.x, p.y)
    {
        return Some(m);
    }
    win.current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten())
}

/// Resize to `panel` and place top-centre, per-monitor DPI aware.
pub fn place<R: Runtime>(win: &WebviewWindow<R>, panel: Panel) -> tauri::Result<()> {
    if panel == Panel::Hidden {
        return win.hide();
    }
    // The window always has the largest panel's size; each panel is a region of it, centred at
    // the top. Switching panels changes only the region, never the WebView's size.
    let (fw, fh) = FRAME;
    let (w, h) = panel.size();
    let Some(m) = target_monitor(win) else {
        return win.set_size(LogicalSize::new(fw, fh));
    };
    let scale = m.scale_factor();
    let (x, y) = top_centre(m.position().x, m.position().y, m.size().width, scale, fw);
    let size = PhysicalSize::new((fw * scale).round() as u32, (fh * scale).round() as u32);
    if win.outer_position().ok() != Some(PhysicalPosition::new(x, y)) {
        win.set_position(PhysicalPosition::new(x, y))?;
    }
    if win.outer_size().ok() != Some(size) {
        win.set_size(size)?;
    }
    #[cfg(windows)]
    if let Ok(hwnd) = win.hwnd() {
        let (rx, rw, rh) = region(fw, w, h, scale);
        tsr_win::display::set_region(hwnd.0 as isize, rx, 0, rw, rh);
    }
    if !win.is_visible().unwrap_or(true) {
        win.show()?;
    }
    if panel == Panel::Expanded || panel == Panel::Draft {
        // User-initiated: keyboard focus for the panel.
        win.set_focus()?;
    }
    Ok(())
}

/// After a display change: reassert always-on-top (cheap, event-driven) and re-place.
pub fn on_display_change<R: Runtime>(win: &WebviewWindow<R>, panel: Panel) -> tauri::Result<()> {
    win.set_always_on_top(false)?;
    win.set_always_on_top(true)?;
    place(
        win,
        if panel == Panel::Expanded || panel == Panel::Draft {
            Panel::Pill
        } else {
            panel
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_have_plan_sizes_and_parse() {
        assert_eq!(Panel::Expanded.size(), (760.0, 520.0));
        assert_eq!(Panel::parse("peek"), Some(Panel::Peek));
        assert_eq!(Panel::parse("fullscreen"), None);
    }

    #[test]
    fn panel_regions_are_top_centred_in_the_frame() {
        assert_eq!(region(760.0, 760.0, 520.0, 1.0), (0, 760, 520));
        assert_eq!(region(760.0, 220.0, 34.0, 1.0), (270, 220, 34));
        // 150 %: frame 1140, pill 330 × 51.
        assert_eq!(region(760.0, 220.0, 34.0, 1.5), (405, 330, 51));
    }

    #[test]
    fn top_centre_on_secondary_monitor_with_negative_origin() {
        // A 2560-wide 100 % monitor left of the primary at x = -2560.
        assert_eq!(top_centre(-2560, 0, 2560, 1.0, 760.0), (-1660, 8));
        // A 1920-wide 125 % primary: 220 logical = 275 physical; margin 8 → 10.
        assert_eq!(top_centre(0, 0, 1920, 1.25, 220.0), (822, 10));
        // A 150 % monitor above the primary (negative y).
        assert_eq!(top_centre(0, -1440, 2560, 1.5, 760.0), (710, -1428));
    }
}
