// KR-01 DESK: the Rust shell. The window, the native side and a few commands; the device itself
// (rendered panels + live content) is in ui/.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Manager, WebviewWindow};

/// The window width (logical px) before full screen, so leaving full screen returns to it.
static LAST_W: AtomicU32 = AtomicU32::new(800);
/// Small enough for Windows Snap (half / quarter / thirds of the screen at any display scaling).
const MIN_W: f64 = 360.0;
/// The S preset (about a messenger window).
const DEFAULT_W: f64 = 800.0;

/// Resize to a width (16:9 kept); the window can then be dragged bigger or smaller freely, never below MIN_W.
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
        // the "max" key (and a double click on the chassis) maximizes like any Windows window, so Snap works too
        "maximize" => {
            if window.is_fullscreen().unwrap_or(false) {
                let r = window.set_fullscreen(false);
                size_to(&window, LAST_W.load(Ordering::Relaxed) as f64);
                r
            } else if window.is_maximized().unwrap_or(false) {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        // F11: full screen and back
        "fullscreen" => {
            let on = window.is_fullscreen().unwrap_or(false);
            if on {
                let r = window.set_fullscreen(false);
                size_to(&window, LAST_W.load(Ordering::Relaxed) as f64);
                r
            } else {
                // remember the size to come back to; size limits would also clamp the full-screen window
                if !window.is_maximized().unwrap_or(false) {
                    if let (Ok(sz), Ok(sf)) = (window.inner_size(), window.scale_factor()) {
                        LAST_W.store((sz.width as f64 / sf).round() as u32, Ordering::Relaxed);
                    }
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

/* ───────────── Kawaii ID: the same round trip as the phone app, but inside a small login window of the app.
 * radio.kawaii.uz/auth/start?s=<state> -> id.kawaii.uz (Telegram login, cookie on .kawaii.uz) -> /auth/done, whose
 * page tries to open intent://auth?t=krs_…&s=<state> (made for Android; desktop browsers ignore it, which is why
 * the browser round trip got stuck on "Salom!"). The login window catches that link itself, keeps the token and
 * closes. A kawaii://auth link (from a browser, through the registered scheme) is still accepted too. */
const CLOUD: &str = "https://radio.kawaii.uz";
static KID: Mutex<Option<String>> = Mutex::new(None);
static KID_STATE: Mutex<Option<String>> = Mutex::new(None);
const KID_WIN: &str = "kid-auth";

/// Keep the token if the state is ours. True when it was taken.
fn kid_accept(t: Option<String>, s: Option<String>) -> bool {
    if let (Some(t), Some(s)) = (t, s) {
        if t.starts_with("krs_") && KID_STATE.lock().unwrap().as_deref() == Some(s.as_str()) {
            *KID.lock().unwrap() = Some(t);
            return true;
        }
    }
    false
}

/// t / s from the query of an auth link (intent://auth?…#Intent;…, kawaii://auth?…, https://…/__desk_auth?…).
fn kid_pairs(u: &tauri::Url) -> (Option<String>, Option<String>) {
    let (mut t, mut s) = (None, None);
    for (k, v) in u.query_pairs() {
        if k == "t" { t = Some(v.to_string()); }
        if k == "s" { s = Some(v.to_string()); }
    }
    (t, s)
}

/// A kawaii://auth?t=…&s=… link from the browser.
fn kid_link(arg: &str) {
    let Ok(u) = arg.parse::<tauri::Url>() else { return };
    if u.scheme() != "kawaii" || u.host_str() != Some("auth") { return; }
    let (t, s) = kid_pairs(&u);
    kid_accept(t, s);
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

/// Open a link in the user's default browser / app (Telegram links open the Telegram app).
fn open_external(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000).spawn();
    }
}

/// On /auth/done: send the intent:// link of the page to an address the login window catches (in case the
/// webview does not report intent:// navigations).
const KID_SCRIPT: &str = r#"(function () {
  if (location.hostname !== 'radio.kawaii.uz' || location.pathname.indexOf('/auth/done') !== 0) return;
  var done = false;
  function go() {
    if (done) return true;
    var a = document.querySelector('a[href^="intent:"], a[href^="kawaii:"]');
    if (!a) return false;
    var h = a.getAttribute('href'), i = h.indexOf('?');
    if (i < 0) return false;
    done = true;
    location.replace('https://radio.kawaii.uz/__desk_auth?' + h.slice(i + 1).split('#')[0]);
    return true;
  }
  function start() { if (!go()) { var n = 0, t = setInterval(function () { if (go() || ++n > 50) clearInterval(t); }, 100); } }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start); else start();
})();"#;

/// Open the sign-in in a small window of the app (async: creating a window in a sync command deadlocks on Windows).
#[tauri::command]
async fn kid_login(app: tauri::AppHandle, state: String) -> Result<(), String> {
    if state.len() < 16 || state.len() > 64 || !state.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("bad state".into());
    }
    *KID_STATE.lock().unwrap() = Some(state.clone());
    if let Some(old) = app.get_webview_window(KID_WIN) { let _ = old.destroy(); }
    let url: tauri::Url = format!("{CLOUD}/auth/start?s={state}").parse().map_err(|e| format!("{e}"))?;
    let nav_app = app.clone();
    let b = tauri::WebviewWindowBuilder::new(&app, KID_WIN, tauri::WebviewUrl::External(url))
        .title("Kawaii ID · Kirish")
        .inner_size(480.0, 720.0)
        .min_inner_size(380.0, 520.0)
        .center()
        .resizable(true)
        .focused(true)
        .initialization_script(KID_SCRIPT)
        // target=_blank links (the Telegram button) open in the Telegram app / browser; the page keeps waiting
        .on_new_window(|u, _| { open_external(u.as_str()); tauri::webview::NewWindowResponse::Deny })
        .on_navigation(move |u| {
            let auth = matches!(u.scheme(), "intent" | "kawaii") && u.host_str() == Some("auth")
                || (u.host_str() == Some("radio.kawaii.uz") && u.path() == "/__desk_auth");
            if auth {
                let (t, s) = kid_pairs(u);
                if kid_accept(t, s) {
                    // close outside the webview's navigation callback
                    let a = nav_app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        if let Some(w) = a.get_webview_window(KID_WIN) { let _ = w.close(); }
                        if let Some(w) = a.get_webview_window("main") { let _ = w.unminimize(); let _ = w.set_focus(); }
                    });
                }
                return false;
            }
            // Telegram links in the same window: hand them to the Telegram app
            if u.scheme() == "tg" || u.host_str() == Some("t.me") || u.host_str() == Some("telegram.me") {
                open_external(u.as_str());
                return false;
            }
            matches!(u.scheme(), "http" | "https" | "about" | "data" | "blob")
        });
    b.build().map_err(|e| format!("{e}"))?;
    Ok(())
}

/// The token brought back by the login window (once).
#[tauri::command]
fn kid_take() -> Option<String> {
    KID.lock().unwrap().take()
}

/// Is the login window still open (the user may close it without signing in).
#[tauri::command]
fn kid_open(app: tauri::AppHandle) -> bool {
    app.get_webview_window(KID_WIN).is_some()
}

/// Account and listening calls, made from here (no CORS): /auth/me, /auth/logout, /auth/sync, /chat/play.
#[tauri::command]
async fn kid_api(path: String, token: String, uid: String, key: String, body: Option<String>) -> Result<(u16, String), String> {
    let (post, is_json) = match path.as_str() {
        "/auth/me" => (false, false),
        "/auth/logout" => (true, false),
        "/auth/sync" => (body.is_some(), true),
        "/chat/play" => (true, true),
        _ => return Err("not allowed".into()),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let tls = native_tls::TlsConnector::new().map_err(|e| format!("{e}"))?;
        let agent = ureq::AgentBuilder::new().tls_connector(Arc::new(tls)).timeout(std::time::Duration::from_secs(15)).build();
        let url = format!("{CLOUD}{path}");
        let mut req = if post { agent.post(&url) } else { agent.get(&url) }
            .set("X-Chat-Uid", &uid)
            .set("X-Chat-Key", &key)
            .set("User-Agent", "KawaiiRadioDesk");
        if !token.is_empty() {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        if is_json {
            req = req.set("Content-Type", "application/json");
        }
        let payload = body.unwrap_or_default();
        let res = if post { req.send_string(&payload) } else { req.call() };
        match res {
            Ok(r) => { let st = r.status(); Ok((st, r.into_string().unwrap_or_default())) }
            Err(ureq::Error::Status(st, r)) => Ok((st, r.into_string().unwrap_or_default())),
            Err(e) => Err(format!("{e}")),
        }
    })
    .await
    .map_err(|e| format!("{e}"))?
}

/// Lets the device be dragged by its chassis. Runs Windows' own move loop on the window's thread with the real
/// cursor position, so Aero Snap (edges, corners, the Snap Layouts bar at the top) works like on a normal window.
#[tauri::command]
fn start_drag(window: WebviewWindow) {
    #[cfg(windows)]
    {
        if let Ok(h) = window.hwnd() {
            let hw = h.0 as isize;
            let _ = window.run_on_main_thread(move || native::drag_window(hw));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = window.start_dragging();
    }
}

/// Window size: a preset (S 800x450 — about a messenger window, M 1120, L 1440, XL 1920) or a width in logical px
/// (the size the user last dragged it to).
#[tauri::command]
fn set_size(window: WebviewWindow, size: String) -> String {
    let want: f64 = match size.as_str() {
        "m" => 1120.0,
        "l" => 1440.0,
        "xl" => 1920.0,
        "s" => DEFAULT_W,
        n => n.parse::<f64>().unwrap_or(DEFAULT_W),
    };
    let w = want.max(MIN_W).min(fit_width(&window).max(MIN_W));
    let h = w * 9.0 / 16.0;
    let _ = window.set_fullscreen(false);
    if window.is_maximized().unwrap_or(false) { let _ = window.unmaximize(); }
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

/// Windows: while the user drags an edge the window keeps 16:9 (Snap and maximize place it freely, the page then
/// fits the device into whatever shape it got), and it gets the rounded corners of Windows 11.
#[cfg(windows)]
mod native {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_STYLE, WMSZ_BOTTOM, WMSZ_BOTTOMLEFT, WMSZ_BOTTOMRIGHT, WMSZ_LEFT,
        WMSZ_RIGHT, WMSZ_TOP, WMSZ_TOPLEFT, WMSZ_TOPRIGHT, WM_SIZING, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU,
        WS_THICKFRAME,
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

    /// Windows' own move loop from the cursor position: the same as grabbing a title bar (Snap included).
    pub fn drag_window(hwnd: isize) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, PostMessageW, HTCAPTION, WM_NCLBUTTONDOWN};
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            ReleaseCapture();
            let lp = ((pt.x as u16 as u32) | ((pt.y as u16 as u32) << 16)) as LPARAM;
            PostMessageW(hwnd as HWND, WM_NCLBUTTONDOWN, HTCAPTION as WPARAM, lp);
        }
    }

    /// Hand a left-button drag on an edge to Windows' own sizing loop (WM_SIZING above keeps it 16:9).
    pub fn resize_drag(hwnd: isize, dir: &str) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetCursorPos, PostMessageW, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT,
            WM_NCLBUTTONDOWN,
        };
        let ht = match dir {
            "n" => HTTOP, "s" => HTBOTTOM, "e" => HTRIGHT, "w" => HTLEFT,
            "ne" => HTTOPRIGHT, "nw" => HTTOPLEFT, "se" => HTBOTTOMRIGHT, _ => HTBOTTOMLEFT,
        };
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            ReleaseCapture();
            let lp = ((pt.x as u16 as u32) | ((pt.y as u16 as u32) << 16)) as LPARAM;
            PostMessageW(hwnd as HWND, WM_NCLBUTTONDOWN, ht as WPARAM, lp);
        }
    }

    pub fn setup(hwnd: isize) {
        unsafe {
            let h = hwnd as HWND;
            // the styles Windows looks at for Snap, Win+arrows and the Snap Layouts flyout
            let st = GetWindowLongPtrW(h, GWL_STYLE);
            SetWindowLongPtrW(h, GWL_STYLE, st | (WS_THICKFRAME | WS_MAXIMIZEBOX | WS_MINIMIZEBOX | WS_SYSMENU) as isize);
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
        .invoke_handler(tauri::generate_handler![window_action, start_drag, start_resize, is_maximized, set_size, kid_login, kid_take, kid_open, kid_api])
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
