//! The chat: one WebSocket to radio.kawaii.uz/chat on its own thread (reconnects by itself), and the CRT view.

use crate::gfx::Quad;
use crate::scene::{rgb, rgba, Canvas};
use crate::text::{self, Align, Txt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

/// Sends the hello and pumps messages both ways; `on_msg` gets every message from the server.
pub fn connect(hello: Value, out: Receiver<String>, on_msg: impl Fn(Value) + Send + 'static) {
    std::thread::spawn(move || loop {
        let res = (|| -> Result<(), String> {
            let (mut ws, _) = tungstenite::connect("wss://radio.kawaii.uz/chat").map_err(|e| e.to_string())?;
            // a short read timeout so the loop can also send what the app queued
            if let tungstenite::stream::MaybeTlsStream::NativeTls(s) = ws.get_mut() {
                let _ = s.get_mut().set_read_timeout(Some(Duration::from_millis(150)));
            }
            ws.send(tungstenite::Message::text(hello.to_string())).map_err(|e| e.to_string())?;
            let mut ping = Instant::now();
            loop {
                while let Ok(s) = out.try_recv() {
                    ws.send(tungstenite::Message::text(s)).map_err(|e| e.to_string())?;
                }
                if ping.elapsed() > Duration::from_secs(25) {
                    ping = Instant::now();
                    ws.send(tungstenite::Message::text("ping")).map_err(|e| e.to_string())?;
                }
                match ws.read() {
                    Ok(tungstenite::Message::Text(t)) => {
                        if let Ok(v) = serde_json::from_str::<Value>(&t) {
                            on_msg(v);
                        }
                    }
                    Ok(tungstenite::Message::Close(_)) => return Err("closed".into()),
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
        })();
        on_msg(json!({ "type": "_down", "error": res.err() }));
        std::thread::sleep(Duration::from_secs(4));
    });
}

#[derive(Clone)]
pub struct ChatMsg {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    pub kind: String,
    pub body: String,
    pub ts: i64,
}

impl ChatMsg {
    fn from(v: &Value) -> Option<Self> {
        Some(ChatMsg {
            id: v.get("id").map(|x| x.to_string()).unwrap_or_default(),
            name: v.get("name")?.as_str()?.to_string(),
            avatar: v.get("avatar").and_then(|a| a.as_str()).and_then(avatar_url),
            kind: v.get("kind").and_then(|k| k.as_str()).unwrap_or("text").to_string(),
            body: v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string(),
            ts: v.get("ts").and_then(|t| t.as_i64()).unwrap_or(0),
        })
    }
}

fn avatar_url(a: &str) -> Option<String> {
    if let Some(u) = a.strip_prefix("k:") {
        return Some(u.to_string());
    }
    if let Some(r) = a.strip_prefix("u:") {
        let p: Vec<&str> = r.split(':').collect();
        return Some(format!("https://radio.kawaii.uz/files/avatars/{}.jpg?v={}", p.first()?, p.get(1).unwrap_or(&"0")));
    }
    None
}

/// Phosphor colours a CRT shows well; a name always gets the same one.
fn name_ink(n: &str) -> u32 {
    const INKS: [u32; 6] = [0xffcf7a, 0x7fe9ff, 0xff9fd8, 0x9dff9a, 0xc8b0ff, 0xffa98a];
    let mut h: u32 = 0;
    for c in n.chars() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    INKS[(h % 6) as usize]
}
const GLYPH: [u32; 6] = [0xcdbbff, 0xb5efd3, 0xffcfa6, 0xff9aa8, 0xa6d8ff, 0xffe08a];

pub struct Chat {
    pub msgs: Vec<ChatMsg>,
    pub online: i64,
    pub live: i64,
    pub pinned: Option<String>,
    pub up: bool,
    /// px scrolled up from the newest message
    pub scroll: f32,
    pub stickers: HashMap<String, String>,
    pub out: Option<Sender<String>>,
    started: Instant,
}

pub const TUBE: [f32; 4] = [1436.0, 602.0, 1878.0, 994.0];

fn c8(hex: u32, a: f32) -> [u8; 4] {
    [((hex >> 16) & 255) as u8, ((hex >> 8) & 255) as u8, (hex & 255) as u8, (a.clamp(0.0, 1.0) * 255.0) as u8]
}

impl Chat {
    pub fn new() -> Self {
        Chat { msgs: vec![], online: 0, live: 0, pinned: None, up: false, scroll: 0.0, stickers: HashMap::new(), out: None, started: Instant::now() }
    }

    pub fn on_server(&mut self, v: &Value) {
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let presence = |c: &mut Chat, v: &Value| {
            if let Some(o) = v.get("online").and_then(|x| x.as_i64()) {
                c.online = o;
                c.live = v.get("live").and_then(|x| x.as_i64()).unwrap_or(o);
            }
        };
        match ty {
            "welcome" => {
                self.up = true;
                self.msgs = v.get("history").and_then(|h| h.as_array()).map(|a| a.iter().filter_map(ChatMsg::from).collect()).unwrap_or_default();
                presence(self, v);
                self.room(v.get("room"));
            }
            "msg" => {
                if let Some(m) = v.get("m").and_then(ChatMsg::from) {
                    self.msgs.push(m);
                    if self.msgs.len() > 200 {
                        self.msgs.remove(0);
                    }
                }
            }
            "presence" => presence(self, v),
            "room" => self.room(Some(v)),
            "deleted" => {
                let id = v.get("id").map(|x| x.to_string()).unwrap_or_default();
                self.msgs.retain(|m| m.id != id);
            }
            "cleared" => self.msgs.clear(),
            "_down" => self.up = false,
            _ => {}
        }
    }

    fn room(&mut self, r: Option<&Value>) {
        let Some(r) = r else { return };
        let p = r.get("pinned").or_else(|| r.get("pin"));
        self.pinned = p.and_then(|p| p.get("body").or_else(|| p.get("text"))).and_then(|b| b.as_str()).map(|b| {
            let t = b.replace("**", "").split_whitespace().collect::<Vec<_>>().join(" ");
            let who = p.and_then(|p| p.get("name")).and_then(|n| n.as_str()).map(|n| format!("  — {n}")).unwrap_or_default();
            t + &who
        });
    }

    /// The sticker file for an id (the pack's own path, a full path, or /files/stickers/<id>.webp).
    fn sticker_url(&self, b: &str) -> String {
        if let Some(f) = self.stickers.get(b) {
            return crate::net::url(f);
        }
        if b.starts_with('/') || b.starts_with("http") {
            return crate::net::url(b);
        }
        format!("https://radio.kawaii.uz/files/stickers/{b}.webp")
    }

    /// The CRT: header, the pinned ticker, the messages (newest at the bottom), scanlines over it all.
    /// `img` gives the texture of a picture (and starts loading it if it is not there yet).
    pub fn draw(&mut self, cv: &mut Canvas, over: &mut Canvas, tl: &mut text::TextLayer, tracks: &HashMap<String, crate::net::Track>, img: &mut dyn FnMut(&str) -> Option<usize>) {
        let [x0, y0, x1, y1] = TUBE;
        let (w, h) = (x1 - x0, y1 - y0);
        cv.flat(Quad::flat(x0, y0, w, h, rgb(0x050806)).rounded(32.0));
        cv.light(Quad::glow(x0 + w * 0.5, y0 + h * 0.45, w * 0.6, rgba(0x1d3a2a, 0.5)));
        let (px, inner) = (x0 + 22.0, w - 42.0);
        // header
        cv.light(Quad::glow(px + 60.0, y0 + 30.0, 50.0, rgba(0x6effaa, 0.12)));
        cv.text(Txt::new("CH·01 CHAT", text::VT, 28.0, px, y0 + 14.0, c8(0x9dffc4, 1.0)));
        let status = if self.up { format!("{} onlayn · {} radioda", self.online, self.live) } else { "qayta ulanmoqda…".into() };
        cv.text(Txt::new(status, text::VT, 19.0, px + 132.0, y0 + 21.0, c8(0x6f9c84, 1.0)).clip([x0, y0, x1 - 70.0, y1]));
        let blink = (self.started.elapsed().as_secs_f32() / 0.8) as i32 % 2 == 0;
        cv.text(Txt::new("LIVE", text::VT, 19.0, x1 - 20.0, y0 + 21.0, c8(0xff6b5e, 1.0)).align(Align::Right));
        if blink {
            cv.light(Quad::glow(x1 - 64.0, y0 + 32.0, 9.0, rgba(0xff5a4d, 0.8)));
            cv.flat(Quad::flat(x1 - 68.0, y0 + 28.0, 8.0, 8.0, rgb(0xff6b5e)).rounded(4.0));
        }
        let mut top = y0 + 52.0;
        // the pinned message runs along like a ticker
        if let Some(p) = &self.pinned {
            cv.flat(Quad::flat(x0, top, w, 26.0, rgba(0x6effaa, 0.07)));
            cv.flat(Quad::flat(x0 + 10.0, top, 102.0, 26.0, rgb(0x6fe3a0)));
            cv.text(Txt::new("QADALGAN", text::VT, 18.0, x0 + 61.0, top + 4.0, c8(0x07140c, 1.0)).align(Align::Center));
            let t = Txt::new(p.clone(), text::MR, 15.0, 0.0, top + 4.0, c8(0xc9f5d8, 1.0)).weight(500);
            let tw = tl.measure(&t) + 60.0;
            let speed = 40.0;
            let off = (self.started.elapsed().as_secs_f32() * speed) % tw;
            for k in 0..3 {
                let mut tk = t.clone();
                tk.x = x0 + 122.0 - off + k as f32 * tw;
                tk.clip = [x0 + 116.0, top, x1 - 8.0, top + 26.0];
                cv.text(tk);
            }
            top += 36.0;
        }
        // messages, laid out bottom-up from the newest
        let clip = [x0 + 6.0, top, x1 - 6.0, y1 - 8.0];
        cv.clip = Some(clip);
        let mut y = y1 - 14.0 + self.scroll;
        let mut total = 0.0;
        for m in self.msgs.iter().rev() {
            let body_h;
            let mut body: Vec<Box<dyn FnOnce(&mut Canvas, f32)>> = vec![];
            match m.kind.as_str() {
                "sticker" => {
                    body_h = 116.0;
                    let u = self.sticker_url(&m.body);
                    let tex = if u.ends_with(".webm") || u.ends_with(".mp4") { None } else { img(&u) };
                    let bx = px + 42.0;
                    body.push(Box::new(move |cv: &mut Canvas, by: f32| {
                        if let Some(t) = tex {
                            let q = Quad::plate_cut(0.0, 0.0, 1920.0, 1080.0, 1.0);
                            let q = Quad { rect: [bx, by + 4.0, 110.0, 110.0], uv: [0.0, 0.0, 1.0, 1.0], ..q };
                            cv.quad(Some(t), q);
                        }
                    }));
                }
                "track" => {
                    body_h = 62.0;
                    let t = tracks.get(&m.body).cloned();
                    let cover = t.as_ref().and_then(|t| img(&crate::net::url(&t.cover_art)));
                    let bx = px + 42.0;
                    body.push(Box::new(move |cv: &mut Canvas, by: f32| {
                        cv.flat(Quad::flat(bx, by + 4.0, 330.0, 56.0, rgba(0x9dffc4, 0.06)).rounded(8.0));
                        if let Some(c) = cover {
                            let q = Quad::plate_cut(0.0, 0.0, 1920.0, 1080.0, 1.0);
                            cv.quad(Some(c), Quad { rect: [bx + 6.0, by + 10.0, 44.0, 44.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 5.0, ..q });
                        }
                        let (ti, ar) = t.map(|t| (t.title, t.artist)).unwrap_or(("qo'shiq".into(), String::new()));
                        let cl = [bx, by.max(clip[1]), bx + 324.0, (by + 60.0).min(clip[3])];
                        cv.text(Txt::new(format!("♪ {ti}"), text::MR, 15.0, bx + 60.0, by + 12.0, c8(0xe2f6e8, 1.0)).weight(700).clip(cl));
                        cv.text(Txt::new(ar, text::MR, 13.0, bx + 60.0, by + 33.0, c8(0x8fb89f, 1.0)).weight(500).clip(cl));
                    }));
                }
                _ => {
                    let t = Txt::new(m.body.clone(), text::MR, 17.0, px + 42.0, 0.0, c8(0xe2f6e8, 1.0)).weight(500).wrap(inner - 42.0).line(23.5);
                    let (_, th) = tl.measure2(&t);
                    body_h = th + 2.0;
                    body.push(Box::new(move |cv: &mut Canvas, by: f32| {
                        let mut t = t;
                        t.y = by + 1.0;
                        t.clip = clip;
                        cv.text(t);
                    }));
                }
            }
            let mh = (22.0 + body_h).max(34.0);
            y -= mh;
            total += mh + 12.0;
            if y + mh >= top - 4.0 && y <= y1 {
                let ink = name_ink(&m.name);
                // avatar: the picture, or a coloured disc with the initial
                let av = m.avatar.as_ref().and_then(|u| img(u));
                if let Some(a) = av {
                    let q = Quad::plate_cut(0.0, 0.0, 1920.0, 1080.0, 1.0);
                    cv.quad(Some(a), Quad { rect: [px, y + 2.0, 32.0, 32.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 7.0, ..q });
                } else {
                    let n: u32 = m.name.chars().map(|c| c as u32).sum();
                    cv.flat(Quad::flat(px, y + 2.0, 32.0, 32.0, rgb(GLYPH[(n % 6) as usize])).rounded(7.0));
                    let ini: String = m.name.trim().chars().next().map(|c| c.to_uppercase().collect()).unwrap_or("?".into());
                    cv.text(Txt::new(ini, text::VT, 22.0, px + 16.0, y + 6.0, c8(0x07140c, 1.0)).align(Align::Center).clip(clip));
                }
                cv.light(Quad::glow(px + 70.0, y + 10.0, 40.0, rgba(ink, 0.06)));
                cv.text(Txt::new(m.name.clone(), text::MR, 15.0, px + 42.0, y, c8(ink, 1.0)).weight(700).clip([clip[0], clip[1], x1 - 70.0, clip[3]]));
                let tm = chrono_hm(m.ts);
                cv.text(Txt::new(tm, text::VT, 16.0, x1 - 20.0, y + 2.0, c8(0x5f8a73, 1.0)).align(Align::Right).clip(clip));
                let by = y + 22.0;
                for f in body {
                    f(cv, by);
                }
            }
            y -= 12.0;
            if y < top - 900.0 {
                break;
            }
        }
        self.scroll = self.scroll.clamp(0.0, (total - (y1 - top)).max(0.0));
        cv.clip = None;
        // the tube: scanlines, dark corners
        over.flat(Quad::scan(x0, y0, w, h, 0.28).rounded(32.0));
        over.flat(Quad::grad(x0, y0, w, 40.0, rgba(0, 0.5), rgba(0, 0.0), true).rounded(32.0));
        over.flat(Quad::grad(x0, y1 - 40.0, w, 40.0, rgba(0, 0.0), rgba(0, 0.55), true).rounded(32.0));
        over.flat(Quad::grad(x0, y0, 36.0, h, rgba(0, 0.5), rgba(0, 0.0), false).rounded(32.0));
        over.flat(Quad::grad(x1 - 36.0, y0, 36.0, h, rgba(0, 0.0), rgba(0, 0.5), false).rounded(32.0));
    }
}

/// Local "HH:MM" of a ms timestamp.
fn chrono_hm(ts: i64) -> String {
    // the device keeps the user's local time; the offset from UTC is taken from the OS once
    let off = local_offset_secs();
    let s = ts / 1000 + off;
    format!("{:02}:{:02}", (s / 3600).rem_euclid(24), (s / 60).rem_euclid(60))
}

fn local_offset_secs() -> i64 {
    use std::sync::OnceLock;
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(|| {
        #[cfg(windows)]
        {
            #[repr(C)]
            struct Tzi {
                bias: i32,
                _rest: [u8; 168],
            }
            unsafe extern "system" {
                fn GetTimeZoneInformation(t: *mut Tzi) -> u32;
            }
            let mut t = Tzi { bias: 0, _rest: [0; 168] };
            let r = unsafe { GetTimeZoneInformation(&mut t) };
            // bias is UTC - local in minutes (daylight bias ignored: Uzbekistan has none)
            let _ = r;
            -(t.bias as i64) * 60
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("date").arg("+%z").output().ok().and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                let sign = if s.starts_with('-') { -1 } else { 1 };
                let d = s.trim_start_matches(['+', '-']);
                let (h, m) = (d.get(0..2)?.parse::<i64>().ok()?, d.get(2..4)?.parse::<i64>().ok()?);
                Some(sign * (h * 3600 + m * 60))
            }).unwrap_or(0)
        }
    })
}

/// The listeners VFD (966,120 398x50): a counter and a row of pixel people.
pub fn draw_listeners(cv: &mut Canvas, n: i64, t: f32, level: f32) {
    let (x, y) = (966.0, 120.0);
    let ink = 0x7dffcf;
    cv.light(Quad::glow(x + 40.0, y + 25.0, 40.0, rgba(ink, 0.15)));
    cv.text(Txt::new(format!("{:03}", n.clamp(0, 999)), text::VT, 40.0, x + 14.0, y + 2.0, c8(ink, 1.0)));
    cv.text(Txt::new("TINGLOVCHI", text::BAR, 9.0, x + 16.0, y + 36.0, c8(ink, 0.5)).weight(600));
    // a pixel person (7 x 11): head, body, arms, legs
    const P: [(f32, f32, f32, f32); 6] = [(2., 0., 3., 3.), (1., 4., 5., 4.), (0., 4., 1., 3.), (6., 4., 1., 3.), (1., 8., 2., 3.), (4., 8., 2., 3.)];
    for k in 0..20 {
        let on = (k as i64) < n;
        let px = x + 92.0 + k as f32 * 14.6;
        let bob = if on { ((t / 0.16 + k as f32 * 1.3).sin().max(0.0)) * level * 4.0 } else { 0.0 };
        let py = y + 14.0 - bob;
        if on {
            cv.light(Quad::glow(px + 7.0, py + 11.0, 12.0, rgba(ink, 0.25)));
        }
        for (rx, ry, rw, rh) in P {
            cv.flat(Quad::flat(px + rx * 1.5, py + 4.0 + ry * 1.5, rw * 1.5, rh * 1.5, rgba(ink, if on { 1.0 } else { 0.09 })));
        }
    }
}
