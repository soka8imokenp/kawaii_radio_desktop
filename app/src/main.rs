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

#[tauri::command]
fn is_maximized(window: WebviewWindow) -> bool {
    window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false)
}

/// Windows: the device is 16:9, so the window keeps 16:9 while it is resized (no empty bars),
/// and gets the rounded corners and shadow of Windows 11.
#[cfg(windows)]
mod native {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WMSZ_BOTTOM, WMSZ_BOTTOMLEFT, WMSZ_BOTTOMRIGHT, WMSZ_LEFT, WMSZ_RIGHT, WMSZ_TOP, WMSZ_TOPLEFT, WMSZ_TOPRIGHT, WM_SIZING,
    };

    unsafe extern "system" fn keep_aspect(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if msg == WM_SIZING && lp != 0 {
            let r = &mut *(lp as *mut RECT);
            let w = r.right - r.left;
            let h = r.bottom - r.top;
            match wp as u32 {
                WMSZ_TOP | WMSZ_BOTTOM => r.right = r.left + h * 16 / 9,
                WMSZ_TOPLEFT | WMSZ_TOPRIGHT => r.top = r.bottom - w * 9 / 16,
                WMSZ_LEFT | WMSZ_RIGHT | WMSZ_BOTTOMLEFT | WMSZ_BOTTOMRIGHT => r.bottom = r.top + w * 9 / 16,
                _ => {}
            }
            return 1;
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }

    pub fn setup(hwnd: isize) {
        unsafe {
            let h = hwnd as HWND;
            SetWindowSubclass(h, Some(keep_aspect), 0x4b52, 0);
            let pref: i32 = DWMWCP_ROUND;
            DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &pref as *const i32 as *const core::ffi::c_void, 4);
        }
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![window_action, start_drag, is_maximized])
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_shadow(true);
                #[cfg(windows)]
                if let Ok(h) = w.hwnd() {
                    native::setup(h.0 as isize);
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Kawaii Radio could not start");
}
