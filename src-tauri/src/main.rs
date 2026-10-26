// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // SAFETY: first statement in main, before Tauri or any other thread starts.
    let cleared = unsafe { tessara_lib::clear_webview2_overrides() };
    tessara_lib::run(cleared);
}
