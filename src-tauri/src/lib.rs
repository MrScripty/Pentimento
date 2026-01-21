//! Pentimento Tauri backend
//!
//! Provides Tauri command handlers for the desktop app.
//! In Tauri mode, Bevy runs as WASM in the same webview as the Svelte UI,
//! so most communication happens directly via JavaScript.

use pentimento_ipc::UiToBevy;
use tauri::Manager;

/// Handle messages from the UI (mainly for logging/debugging)
#[tauri::command]
fn handle_ui_message(message: String) -> Result<(), String> {
    let msg: UiToBevy = serde_json::from_str(&message).map_err(|e| e.to_string())?;
    tracing::debug!("UI message: {:?}", msg);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![handle_ui_message])
        .setup(|app| {
            // Open DevTools automatically in debug builds
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
