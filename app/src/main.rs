// KR-01 DESK: the Rust shell. The window, the native side and a few commands; the device itself
// (rendered panels + live content) is in ui/.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, WebviewWindow};

/// Window keys drawn on the device (the window has no system title bar).
#[tauri::command]
fn window_action(window: WebviewWindow, action: String) {
    let _ = match action.as_str() {
        "minimize" => window.minimize(),
        "maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        "close" => window.close(),
        "fullscreen" => {
            let on = window.is_fullscreen().unwrap_or(false);
            window.set_fullscreen(!on)
        }
        _ => Ok(()),
    };
}

/// Lets the device be dragged by its chassis.
#[tauri::command]
fn start_drag(window: WebviewWindow) {
    let _ = window.start_dragging();
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![window_action, start_drag])
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_shadow(true);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Kawaii Radio could not start");
}
