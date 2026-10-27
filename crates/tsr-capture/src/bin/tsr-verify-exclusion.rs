//! Capture-exclusion verifier for the notch window (plan §13.1, Stage 3 exit criterion).
//!
//!     tsr-verify-exclusion --pid <tessara.exe pid>
//!
//! Proves with real pixels, not just the API's read-back, that screen capture does not see the
//! notch:
//!
//! 1. Finds the visible top-level windows of `--pid` and reads their display affinity.
//! 2. Puts a solid magenta "backdrop" window directly under each notch window (z-order just
//!    below it), larger by a margin, plus a small cyan "decoy" window over the margin.
//! 3. Captures the whole monitor twice, with Windows.Graphics.Capture (what screen sharing and
//!    the Snipping Tool use) and with GDI BitBlt (what older recorders use).
//! 4. Checks, in each capture:
//!    - control A: the decoy (not excluded) is visible as cyan → the capture sees windows;
//!    - the notch rectangle reads as magenta → the notch is absent from the capture.
//! 5. Sets WDA_EXCLUDEFROMCAPTURE on the decoy and captures again:
//!    - control B: the decoy rectangle now reads as magenta → exclusion behaves as expected here.
//!
//! The windows are tool windows that never take focus; they are destroyed before exit. Nothing
//! is saved: the frames are only counted. Prints one JSON report; exit code 0 = PASS, 1 = FAIL,
//! 2 = could not run.

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    match win::run() {
        Ok((pass, report)) => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
            std::process::exit(if pass { 0 } else { 1 });
        }
        Err(e) => {
            eprintln!("tsr-verify-exclusion: {e}");
            std::process::exit(2);
        }
    }
}

#[cfg(windows)]
mod win {
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
        CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW, HBRUSH,
        MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, GWL_EXSTYLE, GetClassNameW,
        GetWindowDisplayAffinity, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, HWND_TOPMOST,
        IsWindowVisible, MSG, PM_REMOVE, PeekMessageW, RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
        SetWindowDisplayAffinity, SetWindowPos, ShowWindow, TranslateMessage, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
        WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };
    use windows::core::{BOOL, PCWSTR, w};

    const MARGIN: i32 = 48;
    /// Fraction of pixels that must match: the notch rectangle must be backdrop-coloured almost
    /// everywhere (the only allowance is antialiasing at the backdrop's own edge).
    const NOTCH_MIN: f64 = 0.99;
    const CONTROL_MIN: f64 = 0.95;
    const MAGENTA: (u8, u8, u8) = (255, 0, 255);
    const CYAN: (u8, u8, u8) = (0, 255, 255);

    fn rgb(c: (u8, u8, u8)) -> COLORREF {
        COLORREF(u32::from(c.0) | (u32::from(c.1) << 8) | (u32::from(c.2) << 16))
    }

    /// BGRA image of one monitor.
    struct Shot {
        w: i32,
        h: i32,
        bgra: Vec<u8>,
    }

    impl Shot {
        /// Fraction of pixels in `r` (monitor-relative) within ±12 of `c`.
        fn fraction(&self, r: &RECT, c: (u8, u8, u8)) -> f64 {
            let (x0, y0) = (r.left.max(0), r.top.max(0));
            let (x1, y1) = (r.right.min(self.w), r.bottom.min(self.h));
            if x1 <= x0 || y1 <= y0 {
                return 0.0;
            }
            let near = |a: u8, b: u8| a.abs_diff(b) <= 12;
            let mut hit = 0usize;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * self.w + x) * 4) as usize;
                    let (b, g, r_) = (self.bgra[i], self.bgra[i + 1], self.bgra[i + 2]);
                    if near(r_, c.0) && near(g, c.1) && near(b, c.2) {
                        hit += 1;
                    }
                }
            }
            hit as f64 / ((x1 - x0) * (y1 - y0)) as f64
        }
    }

    fn arg(name: &str) -> Option<String> {
        let a: Vec<String> = std::env::args().collect();
        a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).cloned()
    }

    fn pump(d: Duration) {
        let end = Instant::now() + d;
        let mut msg = MSG::default();
        while Instant::now() < end {
            // SAFETY: standard message pump on this thread, which owns the test windows.
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> BOOL {
        // SAFETY: lp is the &mut (u32, Vec<isize>) passed by `windows_of`, alive for the call.
        let st = unsafe { &mut *(lp.0 as *mut (u32, Vec<isize>)) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
        if pid == st.0 && unsafe { IsWindowVisible(h) }.as_bool() {
            st.1.push(h.0 as isize);
        }
        BOOL(1)
    }

    fn windows_of(pid: u32) -> Vec<HWND> {
        let mut st: (u32, Vec<isize>) = (pid, Vec::new());
        // SAFETY: the callback only touches `st` through the pointer, which outlives the call.
        let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut st as *mut _ as isize)) };
        st.1.into_iter().map(|h| HWND(h as *mut _)).collect()
    }

    fn class_name(h: HWND) -> String {
        let mut buf = [0u16; 128];
        // SAFETY: buffer length is passed implicitly by the slice.
        let n = unsafe { GetClassNameW(h, &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    fn rect(h: HWND) -> RECT {
        let mut r = RECT::default();
        // SAFETY: h is a window handle; r is a valid out pointer.
        let _ = unsafe { GetWindowRect(h, &mut r) };
        r
    }

    unsafe extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        // SAFETY: forwards every message to the default procedure.
        unsafe { DefWindowProcW(h, m, w, l) }
    }

    fn solid_window(class: PCWSTR, color: (u8, u8, u8)) -> Result<HWND, String> {
        // SAFETY: registers a class with a stock window procedure and a solid brush; creates a
        // hidden, non-activating tool window owned by this thread.
        unsafe {
            let hinst = GetModuleHandleW(None).map_err(|e| e.to_string())?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinst.into(),
                lpszClassName: class,
                hbrBackground: HBRUSH(CreateSolidBrush(rgb(color)).0),
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                class,
                w!("tsr-verify-exclusion"),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(hinst.into()),
                None,
            )
            .map_err(|e| format!("CreateWindowExW: {e}"))
        }
    }

    fn monitor_of(h: HWND) -> (isize, RECT) {
        // SAFETY: h is a valid window; MONITORINFO has cbSize set.
        unsafe {
            let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(m, &mut mi);
            (m.0 as isize, mi.rcMonitor)
        }
    }

    fn wgc(hmon: isize) -> Result<Shot, String> {
        let f = tsr_capture::ocr::capture_monitor_frame(hmon).map_err(|e| format!("WGC: {e:?}"))?;
        Ok(Shot {
            w: f.width as i32,
            h: f.height as i32,
            bgra: f.bgra.to_vec(),
        })
    }

    fn gdi(mon: &RECT) -> Result<Shot, String> {
        let (w, h) = (mon.right - mon.left, mon.bottom - mon.top);
        // SAFETY: plain GDI screen copy into a DIB of the monitor's size; every object created
        // here is released before returning.
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, bmp.into());
            let ok = BitBlt(mem, 0, 0, w, h, Some(screen), mon.left, mon.top, SRCCOPY | CAPTUREBLT);
            // GetDIBits may write colour masks after the header even for BI_RGB, which overruns
            // a plain BITMAPINFO (one RGBQUAD); reserve a full colour table.
            #[repr(C)]
            struct Info {
                header: BITMAPINFOHEADER,
                colors: [u32; 256],
            }
            let mut bi = Info {
                header: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                colors: [0; 256],
            };
            let mut buf = vec![0u8; (w * h * 4) as usize];
            SelectObject(mem, old);
            let lines = GetDIBits(
                mem,
                bmp,
                0,
                h as u32,
                Some(buf.as_mut_ptr().cast()),
                (&mut bi as *mut Info).cast::<BITMAPINFO>(),
                DIB_RGB_COLORS,
            );
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            ok.map_err(|e| format!("BitBlt: {e}"))?;
            if lines != h {
                return Err("GetDIBits".into());
            }
            Ok(Shot { w, h, bgra: buf })
        }
    }

    fn rel(r: &RECT, mon: &RECT) -> RECT {
        RECT {
            left: r.left - mon.left,
            top: r.top - mon.top,
            right: r.right - mon.left,
            bottom: r.bottom - mon.top,
        }
    }

    fn place(h: HWND, after: HWND, r: &RECT) {
        // SAFETY: positions a window owned by this thread; never activates it.
        unsafe {
            let _ = SetWindowPos(
                h,
                Some(after),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOACTIVATE,
            );
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
    }

    pub fn run() -> Result<(bool, Value), String> {
        // Physical pixels everywhere, so window rects and captured frames line up on any DPI.
        // SAFETY: called once at startup before any window exists.
        let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let pid: u32 = arg("--pid")
            .and_then(|s| s.parse().ok())
            .ok_or("--pid <tessara.exe pid> is required")?;
        let targets = windows_of(pid);
        if targets.is_empty() {
            return Err(format!("no visible windows for pid {pid}"));
        }
        let backdrop = solid_window(w!("TsrVerifyBackdrop"), MAGENTA)?;
        let decoy = solid_window(w!("TsrVerifyDecoy"), CYAN)?;
        let mut results = Vec::new();
        let mut pass = true;
        for t in &targets {
            let r = rect(*t);
            let mut aff = 0u32;
            // SAFETY: reads another process's window affinity; out pointer is valid.
            let _ = unsafe { GetWindowDisplayAffinity(*t, &mut aff) };
            // SAFETY: reads the extended style of a window handle.
            let topmost = unsafe { GetWindowLongW(*t, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0;
            let (hmon, mon) = monitor_of(*t);
            let back = RECT {
                left: r.left - MARGIN,
                top: r.top - MARGIN,
                right: r.right + MARGIN,
                bottom: r.bottom + MARGIN,
            };
            // Decoy: inside a side margin (left, or right at the screen's left edge), so it sits
            // over magenta but not under the notch, and stays on this monitor.
            let dec = if back.left + 8 >= mon.left {
                RECT {
                    left: back.left + 8,
                    top: r.top.max(mon.top),
                    right: back.left + MARGIN - 8,
                    bottom: r.top.max(mon.top) + 24,
                }
            } else {
                RECT {
                    left: r.right + 8,
                    top: r.top.max(mon.top),
                    right: r.right + MARGIN - 8,
                    bottom: r.top.max(mon.top) + 24,
                }
            };
            // SAFETY: decoy starts with no exclusion for control A.
            let _ = unsafe { SetWindowDisplayAffinity(decoy, WDA_NONE) };
            place(backdrop, *t, &back);
            place(decoy, HWND_TOPMOST, &dec);
            // Keep the backdrop directly under the notch after the decoy went to the top.
            place(backdrop, *t, &back);
            pump(Duration::from_millis(400));

            let mut per_method = serde_json::Map::new();
            for name in ["wgc", "gdi"] {
                let s = if name == "wgc" { wgc(hmon)? } else { gdi(&mon)? };
                let notch = s.fraction(&rel(&r, &mon), MAGENTA);
                let control_a = s.fraction(&rel(&dec, &mon), CYAN);
                let ok = notch >= NOTCH_MIN && control_a >= CONTROL_MIN;
                pass &= ok;
                per_method.insert(
                    name.into(),
                    json!({"notch_backdrop_fraction": round(notch), "control_a_decoy_visible_fraction": round(control_a), "pass": ok}),
                );
            }
            // Control B: exclude the decoy and expect the backdrop where it was.
            // SAFETY: changes the affinity of this process's own window.
            let excl = unsafe { SetWindowDisplayAffinity(decoy, WDA_EXCLUDEFROMCAPTURE) }.is_ok();
            pump(Duration::from_millis(400));
            for name in ["wgc", "gdi"] {
                let s = if name == "wgc" { wgc(hmon)? } else { gdi(&mon)? };
                let b = s.fraction(&rel(&dec, &mon), MAGENTA);
                let ok = excl && b >= CONTROL_MIN;
                pass &= ok;
                if let Some(Value::Object(m)) = per_method.get_mut(name) {
                    m.insert("control_b_excluded_decoy_hidden_fraction".into(), json!(round(b)));
                    m.insert("pass".into(), json!(m["pass"].as_bool().unwrap_or(false) && ok));
                }
            }
            let aff_ok = aff == WDA_EXCLUDEFROMCAPTURE.0;
            pass &= aff_ok;
            results.push(json!({
                "class": class_name(*t),
                "rect": [r.left, r.top, r.right - r.left, r.bottom - r.top],
                "topmost": topmost,
                "affinity": format!("0x{aff:x}"),
                "affinity_is_exclude_from_capture": aff_ok,
                "methods": per_method,
            }));
        }
        // SAFETY: destroys this thread's own test windows.
        unsafe {
            let _ = DestroyWindow(decoy);
            let _ = DestroyWindow(backdrop);
        }
        Ok((pass, json!({"pid": pid, "windows": results, "pass": pass})))
    }

    fn round(x: f64) -> f64 {
        (x * 10_000.0).round() / 10_000.0
    }
}
