//! WebView2 network policy (plan §13.5).
//!
//! Three layers, all verified at run time by [`report`]:
//! 1. Browser arguments. Setting `additionalBrowserArgs` *replaces* wry's defaults, so
//!    [`BROWSER_ARGS`] starts with the exact default string from wry 0.57.1
//!    (`src/webview2/mod.rs`) and appends Chromium background-networking switches.
//!    Whether WebView2 honours each switch is measured by the Stage 0 network test,
//!    not assumed.
//! 2. SmartScreen off per webview via `ICoreWebView2Settings8::SetIsReputationCheckingRequired(false)`.
//! 3. Navigation lock: only the app's own origin may load.
//!
//! The WebView2 runtime's updater (`msedgeupdate`) is a system service outside this process
//! tree; nothing here affects it (documented in docs/privacy.md).

/// wry 0.57.1 default, kept verbatim. Re-check on every wry upgrade.
pub const WRY_DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// Chromium switches that stop background network activity. Kept separate from the wry
/// defaults so the report can show both parts.
/// `--no-proxy-server` was added after the Stage 0 NetLog showed WPAD (`wpad:80`) proxy
/// auto-discovery lookups; the UI loads nothing remote, so no proxy is ever needed.
pub const BACKGROUND_NETWORK_ARGS: &str = "--disable-background-networking \
--disable-component-update --disable-domain-reliability --disable-sync --no-pings \
--disable-default-apps --no-proxy-server";

pub fn browser_args() -> String {
    let args = format!("{WRY_DEFAULT_ARGS} {BACKGROUND_NETWORK_ARGS}");
    // Debug builds only: write a Chromium NetLog (hostnames of every request WebView2 makes)
    // so the Stage 0 network test can name what it sees. Release builds ignore the variable.
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("TSR_STAGE0_NETLOG") {
        return format!("{args} --log-net-log=\"{path}\" --net-log-capture-mode=Default");
    }
    args
}

/// WebView2 reads these variables and they override what the app configures
/// (e.g. extra browser arguments, a different runtime folder). Clearing them at start-up is
/// defence in depth: a process running as the same user could still set them, which the
/// threat model (§15.2) places out of scope.
pub const WEBVIEW2_OVERRIDE_VARS: &[&str] = &[
    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
    "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
    "WEBVIEW2_USER_DATA_FOLDER",
    "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE",
    "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
];

/// The only origins a Tessara webview may navigate to. Tauri 2 on Windows serves
/// the bundled frontend from `http://tauri.localhost` (and `devUrl` in debug builds).
pub fn is_allowed_navigation(url: &tauri::Url) -> bool {
    match (url.scheme(), url.host_str()) {
        ("http", Some("tauri.localhost")) | ("https", Some("tauri.localhost")) => true,
        ("tauri", _) => true,
        #[cfg(debug_assertions)]
        ("http", Some("localhost")) if url.port() == Some(5173) => true,
        _ => false,
    }
}

/// Facts read back from a live webview, printed by `--stage0-report`.
#[derive(Debug, serde::Serialize)]
pub struct Report {
    pub runtime_version: Option<String>,
    pub browser_args_requested: String,
    pub reputation_checking_required: Option<bool>,
    pub cleared_env_vars: Vec<String>,
}

#[cfg(windows)]
pub fn disable_smartscreen(webview: &tauri::webview::PlatformWebview) -> Option<bool> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings8;
    use windows::core::Interface;

    // SAFETY: COM calls on interfaces owned by the live controller; errors are surfaced as None.
    unsafe {
        let core = webview.controller().CoreWebView2().ok()?;
        let settings8: ICoreWebView2Settings8 = core.Settings().ok()?.cast().ok()?;
        settings8.SetIsReputationCheckingRequired(false).ok()?;
        let mut now = windows::core::BOOL(1);
        settings8.IsReputationCheckingRequired(&mut now).ok()?;
        Some(now.as_bool())
    }
}

#[cfg(windows)]
pub fn runtime_version() -> Option<String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use windows::core::PWSTR;

    let mut out = PWSTR::null();
    // SAFETY: out-pointer receives a CoTaskMem string that we read once and leak (tiny, once per run).
    unsafe {
        GetAvailableCoreWebView2BrowserVersionString(None, &mut out).ok()?;
        out.to_string().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wry_defaults_are_preserved_verbatim() {
        assert!(browser_args().starts_with(WRY_DEFAULT_ARGS));
        assert!(browser_args().contains("msSmartScreenProtection"));
    }

    #[test]
    fn navigation_lock() {
        let ok = |s: &str| is_allowed_navigation(&s.parse().unwrap());
        assert!(ok("http://tauri.localhost/index.html"));
        assert!(!ok("https://example.com/"));
        assert!(!ok("http://tauri.localhost.evil.com/"));
        assert!(!ok("file:///C:/Windows/win.ini"));
        assert!(!ok("http://localhost:8080/"));
    }
}
