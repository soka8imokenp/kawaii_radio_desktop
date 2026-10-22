// KR-01 DESK: the Rust shell. The window, the native side and a few commands; the device itself
// (rendered panels + live content) is in ui/.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Manager, WebviewWindow};

/// The window width (logical px) before full screen, so leaving full screen returns to it.
static LAST_W: AtomicU32 = AtomicU32::new(800);
const MIN_W: f64 = 800.0;

/// Resize to a width (16:9 kept); the window can then be dragged bigger or smaller freely, never below S.
fn size_to(window: &WebviewWindow, w: f64) {
    let h = (w * 9.0 / 16.0).round();
    let _ = window.set_max_size(None::<tauri::LogicalSize<f64>>);
    let _ = window.set_min_size(Some(tauri::LogicalSize::new(MIN_W, (MIN_W * 9.0 / 16.0).round())));
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
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
                size_to(&window, LAST_W.load(Ordering::Relaxed) as f64);
                r
            } else {
                // remember the size to come back to; size limits would also clamp the full-screen window
                if let (Ok(sz), Ok(sf)) = (window.inner_size(), window.scale_factor()) {
                    LAST_W.store((sz.width as f64 / sf).round() as u32, Ordering::Relaxed);
                }
                let _ = window.set_min_size(None::<tauri::LogicalSize<f64>>);
                let _ = window.set_max_size(None::<tauri::LogicalSize<f64>>);
                window.set_fullscreen(true)
            }
        }
        "close" => window.close(),
        _ => Ok(()),
    };
}

/// Resizing from the edges: the window has no frame, so the page has thin edge zones that hand the drag to the OS.
#[tauri::command]
fn start_resize(window: WebviewWindow, dir: String) {
    // the sizing loop has to start on the window's own thread
    #[cfg(windows)]
    if let Ok(h) = window.hwnd() {
        let hw = h.0 as isize;
        let _ = window.run_on_main_thread(move || native::resize_drag(hw, &dir));
    }
}

/* ───────────── Kawaii ID: the same browser round trip as the phone app, in the user's own browser.
 * radio.kawaii.uz/auth/start?s=<state>&d=1 -> id.kawaii.uz (cookie on .kawaii.uz) -> /auth/done, whose page opens
 * kawaii://auth?t=krs_…&s=<state> (the phone gets intent://… instead). Windows hands that link to this exe
 * (registered under HKCU\Software\Classes\kawaii); a second copy passes it to the running one and quits. */
const CLOUD: &str = "https://radio.kawaii.uz";
static KID: Mutex<Option<String>> = Mutex::new(None);
static KID_STATE: Mutex<Option<String>> = Mutex::new(None);

/// A kawaii://auth?t=…&s=… link from the browser: keep the token if the state is ours.
fn kid_link(arg: &str) {
    let Ok(u) = arg.parse::<tauri::Url>() else { return };
    if u.scheme() != "kawaii" || u.host_str() != Some("auth") { return; }
    let (mut t, mut s) = (None, None);
    for (k, v) in u.query_pairs() {
        if k == "t" { t = Some(v.to_string()); }
        if k == "s" { s = Some(v.to_string()); }
    }
    if let (Some(t), Some(s)) = (t, s) {
        if t.starts_with("krs_") && KID_STATE.lock().unwrap().as_deref() == Some(s.as_str()) {
            *KID.lock().unwrap() = Some(t);
        }
    }
}

/// Register the kawaii:// scheme for this user (no admin rights), pointing at this exe.
#[cfg(windows)]
fn register_scheme() {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return };
    let cmd = format!("\"{}\" \"%1\"", exe.display());
    let base = r"HKCU\Software\Classes\kawaii";
    for args in [
        vec!["add", base, "/ve", "/d", "URL:Kawaii Radio", "/f"],
        vec!["add", base, "/v", "URL Protocol", "/d", "", "/f"],
        vec!["add", &format!(r"{base}\shell\open\command"), "/ve", "/d", &cmd, "/f"],
    ] {
        let _ = std::process::Command::new("reg").args(&args).creation_flags(0x0800_0000).status();
    }
}

/// Open the sign-in in the default browser.
#[tauri::command]
fn kid_login(state: String) -> Result<(), String> {
    if state.len() < 16 || state.len() > 64 || !state.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("bad state".into());
    }
    *KID_STATE.lock().unwrap() = Some(state.clone());
    let url = format!("{CLOUD}/auth/start?s={state}&d=1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url]).creation_flags(0x0800_0000).spawn().map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

/// The token brought back by the browser (once).
#[tauri::command]
fn kid_take() -> Option<String> {
    KID.lock().unwrap().take()
}

/// The two account calls, made from here (no CORS): GET /auth/me (also links the chat identity to the account)
/// and POST /auth/logout. Returns (http status, body).
#[tauri::command]
async fn kid_api(path: String, token: String, uid: String, key: String) -> Result<(u16, String), String> {
    let post = match path.as_str() {
        "/auth/me" => false,
        "/auth/logout" => true,
        _ => return Err("not allowed".into()),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let tls = native_tls::TlsConnector::new().map_err(|e| format!("{e}"))?;
        let agent = ureq::AgentBuilder::new().tls_connector(Arc::new(tls)).timeout(std::time::Duration::from_secs(15)).build();
        let url = format!("{CLOUD}{path}");
        let req = if post { agent.post(&url) } else { agent.get(&url) }
            .set("Authorization", &format!("Bearer {token}"))
            .set("X-Chat-Uid", &uid)
            .set("X-Chat-Key", &key)
            .set("User-Agent", "KawaiiRadioDesk");
        let res = if post { req.send_string("") } else { req.call() };
        match res {
            Ok(r) => { let st = r.status(); Ok((st, r.into_string().unwrap_or_default())) }
            Err(ureq::Error::Status(st, r)) => Ok((st, r.into_string().unwrap_or_default())),
            Err(e) => Err(format!("{e}")),
        }
    })
    .await
    .map_err(|e| format!("{e}"))?
}

/// Lets the device be dragged by its chassis.
#[tauri::command]
fn start_drag(window: WebviewWindow) {
    let _ = window.start_dragging();
}

/// Window size: a preset (S 800x450 — about a messenger window, M 1120, L 1440, XL 1920) or a width in logical px
/// (the size the user last dragged it to). Between those the window resizes freely, always 16:9.
#[tauri::command]
fn set_size(window: WebviewWindow, size: String) -> String {
    let want: f64 = match size.as_str() {
        "m" => 1120.0,
        "l" => 1440.0,
        "xl" => 1920.0,
        "s" => MIN_W,
        n => n.parse::<f64>().unwrap_or(MIN_W),
    };
    let w = want.max(MIN_W).min(fit_width(&window).max(MIN_W));
    let h = w * 9.0 / 16.0;
    let _ = window.set_fullscreen(false);
    LAST_W.store(w as u32, Ordering::Relaxed);
    size_to(&window, w);
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

/// The widest 16:9 window that fits the monitor's work area.
fn fit_width(window: &WebviewWindow) -> f64 {
    if let Ok(Some(m)) = window.current_monitor() {
        let sf = m.scale_factor();
        let sz = m.work_area().size;
        let (mw, mh) = (sz.width as f64 / sf, sz.height as f64 / sf);
        return mw.min(mh * 16.0 / 9.0).floor();
    }
    1920.0
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

    /// Hand a left-button drag on an edge to Windows' own sizing loop (WM_SIZING above keeps it 16:9).
    pub fn resize_drag(hwnd: isize, dir: &str) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            PostMessageW, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, WM_NCLBUTTONDOWN,
        };
        let ht = match dir {
            "n" => HTTOP, "s" => HTBOTTOM, "e" => HTRIGHT, "w" => HTLEFT,
            "ne" => HTTOPRIGHT, "nw" => HTTOPLEFT, "se" => HTBOTTOMRIGHT, _ => HTBOTTOMLEFT,
        };
        unsafe {
            ReleaseCapture();
            PostMessageW(hwnd as HWND, WM_NCLBUTTONDOWN, ht as WPARAM, 0);
        }
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
    #[cfg(windows)]
    register_scheme();
    tauri::Builder::default()
        // a second copy (started by a kawaii:// link) hands its arguments over and quits
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            for a in &argv { kid_link(a); }
            if let Some(w) = app.get_webview_window("main") { let _ = w.unminimize(); let _ = w.set_focus(); }
        }))
        .invoke_handler(tauri::generate_handler![window_action, start_drag, start_resize, is_maximized, set_size, kid_login, kid_take, kid_api])
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                // the window is transparent and the device rounds its own corners (Windows does not round borderless windows)
                let _ = w.set_shadow(false);
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
