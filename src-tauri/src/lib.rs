//! Tessara app shell.
//!
//! Stage 0 scope: the WebView2 spike (plan §13.5, Stage 0 "WebView2 network spike").
//! One transparent pill window with the network policy applied, and a `--stage0-report`
//! mode that prints the settings read back from the live webview as JSON.

pub mod webview_policy;

use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Remove WebView2 override variables before any webview exists (see `webview_policy`).
///
/// # Safety
/// Must run at the very start of `main`, before any other thread is spawned:
/// `std::env::remove_var` is not thread-safe.
pub unsafe fn clear_webview2_overrides() -> Vec<String> {
    let mut cleared = Vec::new();
    for name in webview_policy::WEBVIEW2_OVERRIDE_VARS {
        if std::env::var_os(name).is_some() {
            // SAFETY: caller guarantees single-threaded start-up.
            unsafe { std::env::remove_var(name) };
            cleared.push((*name).to_owned());
        }
    }
    cleared
}

pub fn run(cleared_env_vars: Vec<String>) {
    let stage0_report = std::env::args().any(|a| a == "--stage0-report");
    let exit_after = std::env::var("TSR_STAGE0_EXIT_AFTER_S")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());

    tauri::Builder::default()
        .setup(move |app| {
            let window = WebviewWindowBuilder::new(app, "notch", WebviewUrl::App("index.html".into()))
                .title("Tessara")
                .inner_size(220.0, 34.0)
                .resizable(false)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .focused(false)
                .additional_browser_args(&webview_policy::browser_args())
                .on_navigation(webview_policy::is_allowed_navigation)
                .build()?;

            // Top-centre of the primary monitor (plan §13.1; multi-monitor comes in Stage 3).
            if let Some(monitor) = window.primary_monitor()? {
                let scale = monitor.scale_factor();
                let size = monitor.size().to_logical::<f64>(scale);
                let pos = monitor.position().to_logical::<f64>(scale);
                window.set_position(tauri::LogicalPosition::new(
                    pos.x + (size.width - 220.0) / 2.0,
                    pos.y + 8.0,
                ))?;
            }

            let cleared = cleared_env_vars.clone();
            window.with_webview(move |wv| {
                #[cfg(windows)]
                let reputation = webview_policy::disable_smartscreen(&wv);
                #[cfg(not(windows))]
                let reputation: Option<bool> = {
                    let _ = wv;
                    None
                };

                if stage0_report {
                    #[cfg(windows)]
                    let runtime_version = webview_policy::runtime_version();
                    #[cfg(not(windows))]
                    let runtime_version = None;
                    let report = webview_policy::Report {
                        runtime_version,
                        browser_args_requested: webview_policy::browser_args(),
                        reputation_checking_required: reputation,
                        cleared_env_vars: cleared,
                    };
                    println!("STAGE0_REPORT {}", serde_json::to_string(&report).unwrap_or_default());
                }
            })?;

            if let Some(secs) = exit_after {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(secs));
                    handle.exit(0);
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tessara");
}
