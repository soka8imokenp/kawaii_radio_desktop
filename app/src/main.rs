// KR-01 DESK: the Rust shell. The window, the native side and a few commands; the device itself
// (rendered panels + live content) is in ui/.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{Manager, WebviewWindow};

/// The chosen preset width (logical px), so leaving full screen returns to it.
static LAST_W: AtomicU32 = AtomicU32::new(1600);

/// A fixed size without the white frame Windows draws around non-resizable borderless windows:
/// the window stays "resizable", but its min and max size are the same.
fn lock_size(window: &WebviewWindow, w: f64) {
    let h = (w * 9.0 / 16.0).round();
    let _ = window.set_min_size(None::<tauri::LogicalSize<f64>>);
    let _ = window.set_max_size(None::<tauri::LogicalSize<f64>>);
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
    let _ = window.set_min_size(Some(tauri::LogicalSize::new(w, h)));
    let _ = window.set_max_size(Some(tauri::LogicalSize::new(w, h)));
}

/// Window keys drawn on the device (the window has no system title bar).
#[tauri::command]
fn window_action(window: WebviewWindow, action: String) {
    let _ = match action.as_str() {
        "minimize" => window.minimize(),
        // the device has fixed sizes (like a real one); the "max" key goes full screen and back
        "maximize" | "fullscreen" => {
            let on = window.is_fullscreen().unwrap_or(false);
            if on {
                let r = window.set_fullscreen(false);
                lock_size(&window, LAST_W.load(Ordering::Relaxed) as f64);
                r
            } else {
                // size limits would also clamp the full-screen window
                let _ = window.set_min_size(None::<tauri::LogicalSize<f64>>);
                let _ = window.set_max_size(None::<tauri::LogicalSize<f64>>);
                window.set_fullscreen(true)
            }
        }
        "close" => window.close(),
        _ => Ok(()),
    };
}

/// Lets the device be dragged by its chassis.
#[tauri::command]
fn start_drag(window: WebviewWindow) {
    let _ = window.start_dragging();
}

/// Fixed window sizes: S 1280x720, M 1600x900, L 1920x1080. The picture is never stretched in between,
/// so the rendered device stays crisp and the window never jitters while resizing.
#[tauri::command]
fn set_size(window: WebviewWindow, size: String) -> String {
    let want = match size.as_str() {
        "s" => 1280.0,
        "l" => 1920.0,
        "m" => 1600.0,
        _ => fit_width(&window),
    };
    let w = want.min(fit_width(&window).max(1280.0));
    let h = w * 9.0 / 16.0;
    let _ = window.set_fullscreen(false);
    LAST_W.store(w as u32, Ordering::Relaxed);
    lock_size(&window, w);
    // centre in the work area (above the taskbar), not on the whole screen
    if let Ok(Some(m)) = window.current_monitor() {
        let sf = m.scale_factor();
        let wa = m.work_area();
        let (ax, ay) = (wa.position.x as f64 / sf, wa.position.y as f64 / sf);
        let (aw, ah) = (wa.size.width as f64 / sf, wa.size.height as f64 / sf);
        let _ = window.set_position(tauri::LogicalPosition::new(ax + ((aw - w) / 2.0).max(0.0), ay + ((ah - h) / 2.0).max(0.0)));
    }
    format!("{}", w as i32)
}

/// The biggest preset that fits the monitor's work area.
fn fit_width(window: &WebviewWindow) -> f64 {
    if let Ok(Some(m)) = window.current_monitor() {
        let sf = m.scale_factor();
        let sz = m.work_area().size;
        let (mw, mh) = (sz.width as f64 / sf, sz.height as f64 / sf);
        for w in [1920.0, 1600.0, 1280.0] {
            if w <= mw && w * 9.0 / 16.0 <= mh {
                return w;
            }
        }
    }
    1280.0
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
            // DWMWA_BORDER_COLOR = 34, DWMWA_COLOR_NONE: no 1 px accent/white frame around the device
            let none: u32 = 0xFFFF_FFFE;
            DwmSetWindowAttribute(h, 34, &none as *const u32 as *const core::ffi::c_void, 4);
        }
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![window_action, start_drag, is_maximized, set_size])
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
