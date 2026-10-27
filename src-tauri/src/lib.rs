//! Tessara app shell (plan §4, §13, §14).
//!
//! - [`services`]: store, capture, inference runtime and job workers.
//! - [`views`]: data for the notch views (plain Rust, tested without a window).
//! - [`window`]: the notch window — sizes, placement, display changes.
//! - [`webview_policy`]: WebView2 network policy (Stage 0).
//!
//! Commands are thin wrappers that validate input and return content-free error codes.
//! `--stage0-report` keeps the Stage 0 WebView2 report mode.

pub mod ask;
pub mod diagnostics;
pub mod drafts;
pub mod services;
pub mod views;
pub mod webview_policy;
pub mod window;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use services::{Services, Status, local_tz, now_ms};
use views::{ApiError, E_INVALID, FactDto, HourBucket, ItemDetail, SearchHit, Today};
use window::Panel;

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

pub struct AppState {
    pub services: Option<Arc<Services>>,
    pub panel: Mutex<Panel>,
    /// Content-free start-up problem, shown by the UI's error state.
    pub start_error: Option<String>,
    /// The other app's window that had focus when a focused panel opened; focus goes back to
    /// it when the panel closes, so typing never lands in a hidden notch.
    pub return_focus: Mutex<Option<isize>>,
    pub hotkeys: Mutex<Vec<HotkeyInfo>>,
}

const E_UNAVAILABLE: ApiError = ApiError::new("unavailable", "error.unavailable");

fn svc(s: &State<'_, AppState>) -> Result<Arc<Services>, ApiError> {
    s.services.clone().ok_or(E_UNAVAILABLE)
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> Result<Status, ApiError> {
    Ok(svc(&state)?.status())
}

#[tauri::command]
fn set_paused(app: AppHandle, state: State<'_, AppState>, paused: bool) -> Result<Status, ApiError> {
    let s = svc(&state)?;
    s.set_paused(paused);
    let st = s.status();
    let _ = app.emit("status_changed", &st);
    Ok(st)
}

#[tauri::command]
fn get_today(state: State<'_, AppState>) -> Result<Today, ApiError> {
    svc(&state)?.views.today(now_ms(), &local_tz())
}

#[tauri::command]
fn get_timeline_day(state: State<'_, AppState>, at_ms: i64) -> Result<Vec<HourBucket>, ApiError> {
    if !(0..=4_102_444_800_000).contains(&at_ms) {
        return Err(E_INVALID);
    }
    svc(&state)?.views.timeline_day(at_ms, &local_tz())
}

#[tauri::command]
fn list_open_loops(state: State<'_, AppState>, kind: Option<String>) -> Result<Vec<FactDto>, ApiError> {
    svc(&state)?.views.open_loops(kind.as_deref())
}

#[tauri::command]
fn search(
    state: State<'_, AppState>,
    query: String,
    app_filter: Option<String>,
    limit: u32,
) -> Result<Vec<SearchHit>, ApiError> {
    if app_filter.as_ref().is_some_and(|a| a.len() > 120) {
        return Err(E_INVALID);
    }
    svc(&state)?.views.search(&query, app_filter.as_deref(), limit)
}

#[tauri::command]
fn get_item(state: State<'_, AppState>, item_id: i64) -> Result<ItemDetail, ApiError> {
    svc(&state)?.views.item(item_id)
}

#[tauri::command]
fn get_brief(state: State<'_, AppState>) -> Result<tsr_pipeline::brief::Brief, ApiError> {
    svc(&state)?.views.brief(now_ms(), &local_tz())
}

#[tauri::command]
fn get_loop_groups(state: State<'_, AppState>) -> Result<views::LoopGroups, ApiError> {
    svc(&state)?.views.loop_groups(now_ms())
}

#[tauri::command]
fn get_rollups(state: State<'_, AppState>, level: String, at_ms: i64) -> Result<views::RollupView, ApiError> {
    if !(0..=4_102_444_800_000).contains(&at_ms) {
        return Err(E_INVALID);
    }
    svc(&state)?.views.rollups(&level, at_ms, &local_tz())
}

#[tauri::command]
fn get_items(state: State<'_, AppState>, item_ids: Vec<i64>) -> Result<Vec<views::ItemCard>, ApiError> {
    svc(&state)?.views.items_by_ids(&item_ids)
}

#[tauri::command]
fn propose_bulk_status(state: State<'_, AppState>, fact_ids: Vec<i64>, status: String) -> Result<String, ApiError> {
    svc(&state)?
        .views
        .propose_bulk_status(&fact_ids, &status, Instant::now())
}

#[tauri::command]
fn set_bulk_status(
    state: State<'_, AppState>,
    fact_ids: Vec<i64>,
    status: String,
    token: String,
) -> Result<u32, ApiError> {
    svc(&state)?
        .views
        .set_bulk_status(&fact_ids, &status, &token, Instant::now())
}

/// Ask: sources at once; with a model, `ask_sentence` events then `ask_done`.
#[tauri::command]
fn ask(state: State<'_, AppState>, question: String) -> Result<ask::AskStart, ApiError> {
    svc(&state)?.asks.start(&question, now_ms(), &local_tz())
}

/// The diagnostics bundle, for the user to read before deciding to share it.
#[tauri::command]
fn get_diagnostics(state: State<'_, AppState>) -> Result<serde_json::Value, ApiError> {
    Ok(svc(&state)?.diagnostics(state.start_error.as_deref()))
}

/// Save the bundle the user just reviewed into the data folder; returns the file name.
#[tauri::command]
fn save_diagnostics(state: State<'_, AppState>) -> Result<String, ApiError> {
    let v = svc(&state)?.diagnostics(state.start_error.as_deref());
    let dir = services::data_dir().join("diagnostics");
    std::fs::create_dir_all(&dir).map_err(|_| views::E_STORE)?;
    let name = format!("diagnostics-{}.json", now_ms());
    std::fs::write(dir.join(&name), serde_json::to_vec_pretty(&v).unwrap_or_default()).map_err(|_| views::E_STORE)?;
    Ok(name)
}

/// Draft step 2: generate with the user's optional instruction (streams `draft_chunk`, then
/// `draft_ready` or `draft_error`).
#[tauri::command]
fn draft_generate(state: State<'_, AppState>, instruction: String) -> Result<(), ApiError> {
    svc(&state)?.drafts.generate(&instruction)
}

/// Draft step 3: give focus back to the target and paste there, only with the draft's token and
/// only into the same, still editable, non-password field.
#[tauri::command]
fn draft_insert(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
    edited: Option<String>,
) -> Result<(), ApiError> {
    let s = svc(&state)?;
    apply_panel(&app, &state, Panel::Pill)?;
    #[cfg(windows)]
    return s.drafts.insert(&token, edited.as_deref(), now_ms());
    #[cfg(not(windows))]
    {
        let _ = (s, token, edited);
        Err(E_UNAVAILABLE)
    }
}

#[tauri::command]
fn draft_copy(state: State<'_, AppState>, token: String, edited: Option<String>) -> Result<(), ApiError> {
    let s = svc(&state)?;
    #[cfg(windows)]
    return s.drafts.copy(&token, edited.as_deref(), now_ms());
    #[cfg(not(windows))]
    {
        let _ = (s, token, edited);
        Err(E_UNAVAILABLE)
    }
}

#[tauri::command]
fn draft_discard(state: State<'_, AppState>) -> Result<(), ApiError> {
    svc(&state)?.drafts.discard(now_ms());
    Ok(())
}

/// Make the morning brief if it is due, and tell the UI (plan §11.2: on unlock or start).
fn brief_check(app: &AppHandle) {
    let st = app.state::<AppState>();
    if let Some(s) = &st.services
        && let Some((b, ms)) = s.brief_if_due()
    {
        frame_log("brief_ms", &[ms as f64]);
        let _ = app.emit("brief_ready", b.day);
    }
}

#[tauri::command]
fn propose_fact_status(state: State<'_, AppState>, fact_id: i64, status: String) -> Result<String, ApiError> {
    svc(&state)?.views.propose_fact_status(fact_id, &status, Instant::now())
}

#[tauri::command]
fn set_fact_status(
    state: State<'_, AppState>,
    fact_id: i64,
    status: String,
    token: String,
) -> Result<FactDto, ApiError> {
    if token.len() != 32 {
        return Err(views::E_TOKEN);
    }
    svc(&state)?
        .views
        .set_fact_status(fact_id, &status, &token, Instant::now())
}

#[tauri::command]
fn set_panel(app: AppHandle, state: State<'_, AppState>, panel: String) -> Result<(), ApiError> {
    let p = Panel::parse(&panel).ok_or(E_INVALID)?;
    apply_panel(&app, &state, p)
}

/// Show `p`: remember or return keyboard focus, set the window region, record the state.
fn apply_panel(app: &AppHandle, state: &AppState, p: Panel) -> Result<(), ApiError> {
    let w = app.get_webview_window("notch").ok_or(E_UNAVAILABLE)?;
    let focused = |p: Panel| p == Panel::Expanded || p == Panel::Draft;
    let before = *state.panel.lock().unwrap_or_else(|e| e.into_inner());
    #[cfg(windows)]
    if focused(p) && !focused(before) {
        *state.return_focus.lock().unwrap_or_else(|e| e.into_inner()) = tsr_win::display::foreign_foreground();
    }
    window::place(&w, p).map_err(|_| E_UNAVAILABLE)?;
    #[cfg(windows)]
    if !focused(p)
        && focused(before)
        && let Some(h) = state.return_focus.lock().unwrap_or_else(|e| e.into_inner()).take()
    {
        tsr_win::display::restore_foreground(h);
    }
    *state.panel.lock().unwrap_or_else(|e| e.into_inner()) = p;
    Ok(())
}

#[tauri::command]
fn start_error(state: State<'_, AppState>) -> Option<String> {
    state.start_error.clone()
}

/// Whether the notch is excluded from screen capture (read back after setting it). Asked by
/// the UI on load, because an event sent during window setup arrives before it listens.
static CAPTURE_EXCLUDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
fn capture_excluded() -> bool {
    CAPTURE_EXCLUDED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Frame-timing report from the UI (performance harness; numbers only).
#[tauri::command]
fn report_frames(label: String, frame_ms: Vec<f64>) -> Result<(), ApiError> {
    if label.len() > 40 || frame_ms.len() > 2000 || !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(E_INVALID);
    }
    frame_log(&label, &frame_ms);
    Ok(())
}

/// Append one line of numbers to `TSR_FRAME_LOG` (performance harness only; no content).
fn frame_log(label: &str, values: &[f64]) {
    if let Ok(path) = std::env::var("TSR_FRAME_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(
                f,
                "{label} {}",
                values.iter().map(|x| format!("{x:.2}")).collect::<Vec<_>>().join(" ")
            );
        }
    }
}

const HK_TOGGLE: i32 = 1;
const HK_PAUSE: i32 = 2;
const HK_ASK: i32 = 3;

/// One global hotkey as the UI shows it. `chord: None` = no chord could be registered.
#[derive(Clone, Debug, serde::Serialize, PartialEq)]
pub struct HotkeyInfo {
    pub action: &'static str,
    pub default: &'static str,
    pub chord: Option<String>,
}

fn default_hotkeys() -> Vec<HotkeyInfo> {
    [
        ("toggle", "Ctrl+Alt+Space"),
        ("pause", "Ctrl+Alt+P"),
        ("ask", "Ctrl+Alt+K"),
        ("draft", "Ctrl+Alt+D"),
    ]
    .into_iter()
    .map(|(action, default)| HotkeyInfo {
        action,
        default,
        chord: None,
    })
    .collect()
}

/// "Ctrl+Alt+N" from Win32 `MOD_*` flags and a virtual-key code (letters, digits, Space).
pub fn chord_label(modifiers: u32, vk: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (bit, name) in [(0x2, "Ctrl"), (0x1, "Alt"), (0x4, "Shift"), (0x8, "Win")] {
        if modifiers & bit != 0 {
            parts.push(name.into());
        }
    }
    parts.push(match vk {
        0x20 => "Space".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        other => format!("VK{other:02X}"),
    });
    parts.join("+")
}

#[cfg(windows)]
fn set_hotkey(state: &AppState, id: i32, chord: Option<String>) {
    let i = match id {
        HK_TOGGLE => 0,
        HK_PAUSE => 1,
        HK_ASK => 2,
        HK_DRAFT => 3,
        _ => return,
    };
    if let Some(h) = state.hotkeys.lock().unwrap_or_else(|e| e.into_inner()).get_mut(i) {
        h.chord = chord;
    }
}

/// The hotkeys as registered (asked by the UI on load; `hotkeys_changed` signals updates).
#[tauri::command]
fn hotkeys(state: State<'_, AppState>) -> Vec<HotkeyInfo> {
    state.hotkeys.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// `panel_request` payload. `at` is the Unix time (ms) the user's input reached Rust, so the UI
/// can measure input-to-visual latency across the whole path (hotkey → Rust → WebView → frame).
#[derive(Clone, serde::Serialize)]
struct PanelRequest {
    panel: &'static str,
    at: i64,
    /// Rust is placing the window itself (opens from a hotkey or the tray), so the UI only
    /// animates. The event goes out first and the region is applied right after, so the two
    /// overlap; the harness counts input-to-visual as the later of "region applied" (logged
    /// here) and "first animation frame painted" (logged by the UI).
    placed: bool,
}

fn request_panel(app: &AppHandle, panel: &'static str) {
    let at = services::now_ms();
    let target = match panel {
        "expanded" | "search" => Some(Panel::Expanded),
        "draft" => Some(Panel::Draft),
        _ => None,
    };
    let _ = app.emit(
        "panel_request",
        PanelRequest {
            panel,
            at,
            placed: target.is_some(),
        },
    );
    if let Some(p) = target {
        if apply_panel(app, &app.state::<AppState>(), p).is_err() {
            // The UI animated into a window we could not resize: put it back to the pill.
            let _ = app.emit(
                "panel_request",
                PanelRequest {
                    panel: "pill",
                    at,
                    placed: false,
                },
            );
        }
        frame_log("region_after_input_ms", &[(services::now_ms() - at) as f64]);
    }
}
const HK_DRAFT: i32 = 4;

#[cfg(windows)]
fn os_events(app: AppHandle) {
    use tsr_win::events::{Hotkey, MOD_ALT, MOD_CONTROL, OsEvent, spawn_with};
    let (tx, rx) = std::sync::mpsc::channel();
    let hotkeys = vec![
        // Ctrl+Alt+Space is also the default of other launchers (on the dev machine another app
        // already owned it), so the toggle falls back to Ctrl+Alt+N ("notch").
        Hotkey {
            id: HK_TOGGLE,
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: 0x20,
            alternates: &[(MOD_CONTROL | MOD_ALT, b'N' as u32)],
        },
        Hotkey {
            id: HK_PAUSE,
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: b'P' as u32,
            alternates: &[],
        },
        Hotkey {
            id: HK_ASK,
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: b'K' as u32,
            alternates: &[],
        },
        Hotkey {
            id: HK_DRAFT,
            modifiers: MOD_CONTROL | MOD_ALT,
            vk: b'D' as u32,
            alternates: &[],
        },
    ];
    let Ok(thread) = spawn_with(tx, hotkeys) else { return };
    std::thread::Builder::new()
        .name("tsr-ui-events".into())
        .spawn(move || {
            let _keep = thread;
            for e in rx {
                let Some(w) = app.get_webview_window("notch") else {
                    continue;
                };
                let state = app.state::<AppState>();
                let panel = *state.panel.lock().unwrap_or_else(|p| p.into_inner());
                match e {
                    OsEvent::Hotkey { id: HK_TOGGLE } => {
                        request_panel(&app, if panel == Panel::Expanded { "pill" } else { "expanded" });
                    }
                    OsEvent::Hotkey { id: HK_ASK } => {
                        request_panel(&app, "search");
                    }
                    OsEvent::Hotkey { id: HK_DRAFT } => {
                        // Read the target before the notch takes focus.
                        let start = state.services.as_ref().map(|s| s.drafts.begin());
                        let payload = match start {
                            Some(Ok(d)) => {
                                serde_json::json!({"ok": true, "app": d.app, "thread_chars": d.thread_chars})
                            }
                            Some(Err(e)) => serde_json::json!({"ok": false, "code": e.code}),
                            None => serde_json::json!({"ok": false, "code": "unavailable"}),
                        };
                        let _ = app.emit("draft_target", payload);
                        request_panel(&app, "draft");
                    }
                    OsEvent::Hotkey { id: HK_PAUSE } => {
                        if let Some(s) = &state.services {
                            s.set_paused(!s.is_paused());
                            let _ = app.emit("status_changed", s.status());
                        }
                    }
                    OsEvent::HotkeyUnavailable { id } => {
                        set_hotkey(&state, id, None);
                        let _ = app.emit("hotkeys_changed", ());
                    }
                    OsEvent::HotkeyRegistered { id, modifiers, vk } => {
                        set_hotkey(&state, id, Some(chord_label(modifiers, vk)));
                        let _ = app.emit("hotkeys_changed", ());
                    }
                    OsEvent::DisplayChanged => {
                        let _ = window::on_display_change(&w, panel);
                    }
                    OsEvent::SettingsChanged => {
                        let _ = app.emit("settings_changed", ());
                    }
                    OsEvent::PowerSource { on_ac } => {
                        if let Some(s) = &state.services {
                            s.set_on_battery(!on_ac);
                        }
                    }
                    OsEvent::SessionUnlocked => brief_check(&app),
                    OsEvent::Foreground { .. } => {
                        // Hidden state while a fullscreen game or presentation is in front.
                        use tsr_win::query::{UserState, user_state};
                        let busy = matches!(
                            user_state(),
                            UserState::FullscreenD3D | UserState::Presentation | UserState::Busy
                        );
                        if busy && panel != Panel::Expanded {
                            let _ = w.hide();
                        } else if !busy && !w.is_visible().unwrap_or(true) {
                            let _ = window::place(&w, panel);
                        }
                    }
                    _ => {}
                }
            }
        })
        .ok();
}

pub fn run(cleared_env_vars: Vec<String>) {
    let stage0_report = std::env::args().any(|a| a == "--stage0-report");
    let exit_after = std::env::var("TSR_STAGE0_EXIT_AFTER_S")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());

    let (services, startup_error) = if stage0_report {
        (None, None)
    } else {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        let third_party = std::env::var_os("TSR_THIRD_PARTY")
            .map(std::path::PathBuf::from)
            .or_else(|| exe_dir.map(|d| d.join("third_party")))
            .unwrap_or_else(|| "third_party".into());
        match Services::start(&services::data_dir(), &third_party) {
            Ok(s) => (Some(Arc::new(s)), None),
            Err(e) => (None, Some(e)),
        }
    };

    tauri::Builder::default()
        .manage(AppState {
            services,
            panel: Mutex::new(Panel::Pill),
            return_focus: Mutex::new(None),
            hotkeys: Mutex::new(default_hotkeys()),
            start_error: startup_error,
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            set_paused,
            get_today,
            get_timeline_day,
            list_open_loops,
            search,
            get_item,
            ask,
            get_diagnostics,
            save_diagnostics,
            draft_generate,
            draft_insert,
            draft_copy,
            draft_discard,
            get_brief,
            get_loop_groups,
            get_rollups,
            get_items,
            propose_bulk_status,
            set_bulk_status,
            propose_fact_status,
            set_fact_status,
            set_panel,
            start_error,
            capture_excluded,
            hotkeys,
            report_frames
        ])
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

            // Keep the notch out of screenshots and screen shares (plan §13.1); read back.
            #[cfg(windows)]
            if let Ok(h) = window.hwnd() {
                let notch = tsr_win::display::exclude_from_capture(h.0 as isize).unwrap_or(false);
                // Also the toolkit's own helper windows, so "every visible window" holds.
                let (seen, done) = tsr_win::display::exclude_own_windows();
                CAPTURE_EXCLUDED.store(notch && seen == done, std::sync::atomic::Ordering::Relaxed);
            }
            let _ = window::place(&window, Panel::Pill);
            {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    brief_check(&h);
                });
            }
            if let Some(s) = &app.state::<AppState>().services {
                let h = app.handle().clone();
                s.drafts.set_emitter(std::sync::Arc::new(move |ev, payload| {
                    let _ = h.emit(ev, payload);
                }));
                let h2 = app.handle().clone();
                s.asks.set_emitter(std::sync::Arc::new(move |ev, payload| {
                    let _ = h2.emit(ev, payload);
                }));
            }
            // Test hook (tools/verify-capture-exclusion.ps1): open a panel once the UI is up.
            if let Ok(p) = std::env::var("TSR_START_PANEL")
                && p == "expanded"
            {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    request_panel(&h, "expanded");
                });
            }

            if !stage0_report {
                let open = MenuItem::with_id(app, "open", "Open Tessara", true, None::<&str>)?;
                let pause = MenuItem::with_id(app, "pause", "Pause / resume capture", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&open, &pause, &quit])?;
                let mut tray = TrayIconBuilder::with_id("tessara").menu(&menu).tooltip("Tessara");
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
                tray.on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => {
                        request_panel(app, "expanded");
                    }
                    "pause" => {
                        let st = app.state::<AppState>();
                        if let Some(s) = &st.services {
                            s.set_paused(!s.is_paused());
                            let _ = app.emit("status_changed", s.status());
                        }
                    }
                    "quit" => {
                        if let Some(s) = &app.state::<AppState>().services {
                            s.shutdown();
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
                #[cfg(windows)]
                os_events(app.handle().clone());
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
                    if let Some(s) = &handle.state::<AppState>().services {
                        s.shutdown();
                    }
                    handle.exit(0);
                });
            }
            Ok(())
        })
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::ScaleFactorChanged { .. } = ev
                && let Some(win) = w.app_handle().get_webview_window("notch")
            {
                let p = *w
                    .app_handle()
                    .state::<AppState>()
                    .panel
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let _ = window::place(&win, p);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Tessara");
}

#[cfg(test)]
mod chord_tests {
    #[test]
    fn labels() {
        assert_eq!(super::chord_label(0x3, 0x20), "Ctrl+Alt+Space");
        assert_eq!(super::chord_label(0x3, b'N' as u32), "Ctrl+Alt+N");
        assert_eq!(super::chord_label(0x6, 0x0D), "Ctrl+Shift+VK0D");
    }
}
