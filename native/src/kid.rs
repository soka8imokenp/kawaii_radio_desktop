//! Kawaii ID: sign in in the system browser (radio.kawaii.uz/auth/start?s=<state>&d=1 → id.kawaii.uz → the last
//! page opens kawaii://auth?t=<token>&s=<state>), the OS starts us with that link, and a running copy takes it over
//! a local socket. With lb=1 the page instead comes straight to that socket (http://127.0.0.1:47823/auth?…): no
//! scheme handler needed — sandboxed browsers (Flatpak, Snap) and AppImages can't always start one. Account calls go to radio.kawaii.uz with the token and this computer's chat identity.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const PORT: u16 = 47823;
pub const CLOUD: &str = "https://radio.kawaii.uz";

/// Open a link in the user's browser.
pub fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

/// Make kawaii:// links start this program (per user, no admin rights).
pub fn register_scheme() {
    // an AppImage runs from a temporary mount: the link must start the AppImage file itself
    let Some(exe) = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from).or_else(|| std::env::current_exe().ok()) else { return };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
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
    #[cfg(not(windows))]
    {
        let Some(home) = std::env::var_os("HOME") else { return };
        let dir = std::path::PathBuf::from(home).join(".local/share/applications");
        let _ = std::fs::create_dir_all(&dir);
        let desk = format!(
            "[Desktop Entry]\nType=Application\nName=Kawaii Radio (link)\nExec=\"{}\" %u\nNoDisplay=true\nMimeType=x-scheme-handler/kawaii;\n",
            exe.display()
        );
        let _ = std::fs::write(dir.join("kawaii-radio-link.desktop"), desk);
        let _ = std::process::Command::new("xdg-mime").args(["default", "kawaii-radio-link.desktop", "x-scheme-handler/kawaii"]).status();
        let _ = std::process::Command::new("update-desktop-database").arg(&dir).status();
    }
}

/// If a copy is already running, hand it our kawaii:// link (or just wake it) and return true: this copy quits.
pub fn hand_over(link: Option<&str>) -> bool {
    match TcpStream::connect_timeout(&([127, 0, 0, 1], PORT).into(), Duration::from_millis(300)) {
        Ok(mut s) => {
            let _ = s.write_all(link.unwrap_or("focus").as_bytes());
            true
        }
        Err(_) => false,
    }
}

/// The first copy listens for links from later ones.
pub fn listen(on_link: impl Fn(String) + Send + 'static) {
    let Ok(l) = TcpListener::bind(("127.0.0.1", PORT)) else { return };
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut s = s;
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            // a later copy writes its link and closes; a browser sends an HTTP request and waits for the page
            let mut buf = vec![];
            let mut chunk = [0u8; 2048];
            while let Ok(n) = s.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 16384 {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&buf).to_string();
            match text.strip_prefix("GET /") {
                Some(rest) => {
                    let path = rest.split_whitespace().next().unwrap_or("");
                    let ok = path.starts_with("auth?");
                    let page = if ok { DONE_PAGE } else { "" };
                    let status = if ok { "200 OK" } else { "404 Not Found" };
                    let head = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n", page.len());
                    let _ = s.write_all(head.as_bytes());
                    let _ = s.write_all(page.as_bytes());
                    if ok {
                        on_link(format!("kawaii://{path}"));
                    }
                }
                None => on_link(text),
            }
        }
    });
}

/// What the browser shows once the sign-in reached the app.
const DONE_PAGE: &str = r#"<!doctype html><html lang="uz"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Kawaii Radio</title>
<style>body{margin:0;min-height:100vh;display:grid;place-items:center;background:#1c1d21;color:#ecebf0;font:16px system-ui,sans-serif}
.c{max-width:360px;background:#26272c;border-radius:28px;padding:24px 20px;text-align:center}.l{background:#c9d7a0;color:#2f3b1c;border-radius:12px;padding:10px 14px;font-size:14px;letter-spacing:.08em}
h1{font-size:24px;margin:18px 0 6px}p{margin:0;color:#9a99a6}</style></head>
<body><div class="c"><div class="l">KAWAII RADIO · KAWAII ID</div><h1>Tayyor!</h1><p>Kawaii Radio ilovasiga qayting — bu oynani yopsa bo'ladi.</p></div>
<script>setTimeout(function(){window.close()},1500)</script></body></html>"#;

/// t and s from kawaii://auth?t=…&s=…
pub fn parse_link(link: &str) -> Option<(String, String)> {
    let q = link.strip_prefix("kawaii://auth")?.trim_start_matches('/').strip_prefix('?')?;
    let (mut t, mut s) = (None, None);
    for kv in q.split('&') {
        let (k, v) = kv.split_once('=')?;
        let v = v.replace("%5F", "_").replace("%2D", "-");
        if k == "t" {
            t = Some(v);
        } else if k == "s" {
            s = Some(v);
        }
    }
    Some((t?, s?))
}

pub fn new_state() -> String {
    let mut b = [0u8; 16];
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    for (i, x) in b.iter_mut().enumerate() {
        *x = ((n >> ((i % 8) * 8)) as u8) ^ (std::process::id() as u8).wrapping_mul(i as u8 + 7);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// An account call: GET /auth/me, POST /auth/logout, GET|POST /auth/sync.
pub fn api(path: &str, token: &str, uid: &str, key: &str, body: Option<&str>) -> Result<(u16, String), String> {
    let a = crate::net::agent();
    let url = format!("{CLOUD}{path}");
    let req = if body.is_some() || path == "/auth/logout" { a.post(&url) } else { a.get(&url) }
        .set("Authorization", &format!("Bearer {token}"))
        .set("X-Chat-Uid", uid)
        .set("X-Chat-Key", key)
        .set("Content-Type", "application/json");
    let res = match body {
        Some(b) => req.send_string(b),
        None if path == "/auth/logout" => req.send_string(""),
        None => req.call(),
    };
    match res {
        Ok(r) => Ok((r.status(), r.into_string().unwrap_or_default())),
        Err(ureq::Error::Status(st, r)) => Ok((st, r.into_string().unwrap_or_default())),
        Err(e) => Err(e.to_string()),
    }
}

/// The public profile (LEVEL, XP, season RANK) from id.kawaii.uz.
pub fn public(username: &str) -> Option<serde_json::Value> {
    crate::net::agent().get(&format!("https://id.kawaii.uz/api/v1/u/{username}")).call().ok()?.into_json().ok()
}
