//! Stage 0 capture spike (plan §17 Stage 0: "UIA coverage spike", "WGC single-frame grab").
//!
//! For each visible top-level window of the selected apps, measure rung 1 (UI Automation)
//! and rung 2 (one WGC frame + Windows OCR). Output is metrics only. Window titles and
//! captured text are never printed or saved, because a probe run on a real desktop would
//! otherwise copy personal content into the repo. `--show-text` prints a short excerpt and is
//! meant only for windows that display synthetic fixtures.
//!
//! Usage:
//!   uia-probe --exe notepad.exe --exe msedge.exe [--pid N]... [--title-contains S]
//!             [--label NAME] [--csv out.csv] [--no-ocr] [--show-text] [--assert-absent S]...
//!
//! `--pid` and `--title-contains` narrow the selection further so a probe run never reads the
//! user's own windows of the same app (e.g. their everyday browser). Titles are matched but
//! never printed.

#[cfg(windows)]
mod quality;
#[cfg(windows)]
mod uia;
#[cfg(windows)]
mod wgc;

#[cfg(not(windows))]
fn main() {
    eprintln!("uia-probe only runs on Windows");
}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use std::io::Write;

    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut exes: Vec<String> = Vec::new();
    let mut pids: Vec<u32> = Vec::new();
    let mut title_contains: Option<String> = None;
    let mut label: Option<String> = None;
    let mut assert_absent: Vec<String> = Vec::new();
    let mut csv_path: Option<String> = None;
    let mut no_ocr = false;
    let mut show_text = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--exe" => exes.push(it.next().expect("--exe needs a value").to_ascii_lowercase()),
            "--pid" => pids.push(it.next().expect("--pid needs a value").parse()?),
            "--title-contains" => {
                title_contains = Some(it.next().expect("--title-contains needs a value").to_lowercase())
            }
            "--assert-absent" => assert_absent.push(it.next().expect("--assert-absent needs a value").clone()),
            "--label" => label = Some(it.next().expect("--label needs a value").clone()),
            "--csv" => csv_path = Some(it.next().expect("--csv needs a path").clone()),
            "--no-ocr" => no_ocr = true,
            "--show-text" => show_text = true,
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    if exes.is_empty() {
        anyhow::bail!("pass at least one --exe <image name>; the probe never scans every window");
    }

    // SAFETY: first COM call on this thread; MTA as in plan §4.1 (T2).
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let uia = uia::Uia::new()?;
    let cap = if no_ocr { None } else { Some(wgc::Capturer::new()?) };
    if let Some(c) = &cap {
        println!("borderless capture access: {}", c.borderless_access);
    }

    let header = "label,exe,class,uia_method,uia_ms,uia_nodes,uia_pw_skipped,uia_chars,uia_quality,\
wgc_ms,wgc_w,wgc_h,wgc_border_free,ocr_ms,ocr_chars,ocr_quality,usable,error";
    let mut rows = vec![header.to_string()];
    let mut leaked = 0usize;
    println!("{header}");

    for w in windows_for(&exes, &pids, title_contains.as_deref()) {
        let u = uia.read(w.hwnd);
        let uq = quality::score(&u.text, None);
        let o = cap.as_ref().map(|c| c.capture_and_ocr(w.hwnd)).unwrap_or_default();
        let oq = quality::score(&o.text, (o.height > 0).then_some(o.height as u32));
        // "Usable" per Stage 0 exit criterion: either rung yields >= 40 chars at quality >= 0.55.
        let usable = (u.text.chars().count() >= 40 && uq >= 0.55) || (o.text.chars().count() >= 40 && oq >= 0.55);
        let err = [u.error.clone(), o.error.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("|");
        let row = format!(
            "{},{},{},{},{},{},{},{},{:.2},{},{},{},{},{},{},{:.2},{},{}",
            label.as_deref().unwrap_or(""),
            w.exe,
            w.class,
            u.method,
            u.ms,
            u.nodes,
            u.password_nodes_skipped,
            u.text.chars().count(),
            uq,
            o.capture_ms,
            o.width,
            o.height,
            o.border_free,
            o.ocr_ms,
            o.text.chars().count(),
            oq,
            usable,
            err
        );
        println!("{row}");
        if show_text {
            println!("  uia: {:?}", u.text.chars().take(300).collect::<String>());
            println!("  ocr: {:?}", o.text.chars().take(300).collect::<String>());
        }
        rows.push(row);
        for needle in &assert_absent {
            if u.text.contains(needle.as_str()) || o.text.contains(needle.as_str()) {
                // Report which rung leaked, never the surrounding text.
                eprintln!(
                    "LEAK: asserted-absent string found (uia={}, ocr={}) in {}",
                    u.text.contains(needle.as_str()),
                    o.text.contains(needle.as_str()),
                    w.exe
                );
                leaked += 1;
            }
        }
    }

    if let Some(p) = csv_path {
        // Append so several labelled runs build one table; header only for a new file.
        let new = !std::path::Path::new(&p).exists();
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p)?;
        for r in rows.into_iter().skip(if new { 0 } else { 1 }) {
            writeln!(f, "{r}")?;
        }
        println!("wrote {p}");
    }
    if leaked > 0 {
        anyhow::bail!("{leaked} window(s) exposed an --assert-absent string");
    }
    Ok(())
}

#[cfg(windows)]
struct Win {
    hwnd: windows::Win32::Foundation::HWND,
    exe: String,
    class: String,
}

/// Visible, uncloaked, titled top-level windows whose process image matches `exes`.
#[cfg(windows)]
fn windows_for(exes: &[String], pids: &[u32], title_contains: Option<&str>) -> Vec<Win> {
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{BOOL, PWSTR};

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam is the &mut Vec<HWND> passed below, alive for the EnumWindows call.
        unsafe { (*(lparam.0 as *mut Vec<HWND>)).push(hwnd) };
        BOOL(1)
    }

    let mut all: Vec<HWND> = Vec::new();
    // SAFETY: callback only pushes into `all`.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize));
    }

    let mut out = Vec::new();
    for hwnd in all {
        // SAFETY: read-only queries on window handles from EnumWindows.
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() || GetWindowTextLengthW(hwnd) == 0 {
                continue;
            }
            let mut cloaked: u32 = 0;
            let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
            if cloaked != 0 {
                continue;
            }
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 {
                continue;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if !pids.is_empty() && !pids.contains(&pid) {
                continue;
            }
            if let Some(needle) = title_contains {
                let mut t = [0u16; 512];
                let n = GetWindowTextW(hwnd, &mut t);
                if !String::from_utf16_lossy(&t[..n as usize])
                    .to_lowercase()
                    .contains(needle)
                {
                    continue;
                }
            }
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(h);
            if !ok {
                continue;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let exe = path.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
            if !exes.contains(&exe) {
                continue;
            }
            let mut cls = [0u16; 256];
            let n = GetClassNameW(hwnd, &mut cls);
            out.push(Win {
                hwnd,
                exe,
                class: String::from_utf16_lossy(&cls[..n as usize]),
            });
        }
    }
    out
}
