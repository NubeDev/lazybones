//! The Tauri shell for lazybones. It hosts the same web UI the browser serves —
//! there is no desktop-only render path. The window is frameless-friendly and
//! the frontend feature-detects the bridge at runtime (`isDesktop()`), so this
//! shell stays deliberately thin: open a window, expose one greet-style command
//! for the bridge smoke-test, done.

/// A trivial command proving the JS↔Rust bridge is live (used by the runtime
/// platform check; the UI itself talks to lazybonesd over HTTP, not the bridge).
#[tauri::command]
fn ping() -> &'static str {
    "lazybones-desktop"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![ping])
        .run(tauri::generate_context!())
        .expect("error while running lazybones desktop");
}
