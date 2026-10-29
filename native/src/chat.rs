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

/// The four reactions the server knows.
pub const REACTIONS: [&str; 4] = ["❤️", "🔥", "😂", "😭"];

#[derive(Clone)]
pub struct Reply {
    pub name: String,
    pub body: String,
    pub kind: String,
}

#[derive(Clone)]
pub struct ChatMsg {
    pub id: i64,
    pub uid: String,
    pub kid: Option<String>,
    pub name: String,
    pub avatar: Option<String>,
    pub kind: String,
    pub body: String,
    pub ts: i64,
    pub reply: Option<Reply>,
    /// emoji -> who reacted
    pub reactions: Vec<(String, Vec<String>)>,
}

fn reactions_of(v: Option<&Value>) -> Vec<(String, Vec<String>)> {
    let Some(o) = v.and_then(|r| r.as_object()) else { return vec![] };
    let mut out: Vec<(String, Vec<String>)> = REACTIONS
        .iter()
        .filter_map(|e| {
            let who: Vec<String> = o.get(*e)?.as_array()?.iter().filter_map(|u| u.as_str().map(String::from)).collect();
            (!who.is_empty()).then(|| (e.to_string(), who))
        })
        .collect();
    out.retain(|r| !r.1.is_empty());
    out
}

impl ChatMsg {
    fn from(v: &Value) -> Option<Self> {
        let reply = v.get("reply").filter(|r| r.is_object()).map(|r| Reply {
            name: r.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            body: r.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            kind: r.get("kind").and_then(|x| x.as_str()).unwrap_or("text").to_string(),
        });
        Some(ChatMsg {
            id: v.get("id").and_then(|x| x.as_i64()).unwrap_or(0),
            uid: v.get("uid").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            kid: v.get("kid").and_then(|x| x.as_str()).map(String::from),
            name: v.get("name")?.as_str()?.to_string(),
            avatar: v.get("avatar").and_then(|a| a.as_str()).and_then(avatar_url),
            kind: v.get("kind").and_then(|k| k.as_str()).unwrap_or("text").to_string(),
            body: v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string(),
            ts: v.get("ts").and_then(|t| t.as_i64()).unwrap_or(0),
            reply,
            reactions: reactions_of(v.get("reactions")),
        })
    }
    /// One line for a quote / preview.
    pub fn preview(&self) -> String {
        preview(&self.kind, &self.body)
    }
}

fn preview(kind: &str, body: &str) -> String {
    match kind {
        "sticker" => "🖼 stiker".into(),
        "track" => "♪ qo'shiq".into(),
        _ => body.replace("**", "").split_whitespace().collect::<Vec<_>>().join(" "),
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

/// Something clickable in the chat tube this frame.
#[derive(Clone, PartialEq)]
enum Hit {
    Msg(i64),
    User(String),
    React(i64, String),
    Jump,
    Sheet,
    Key(&'static str),
    MenuReact(String),
    ReplyCancel,
    Unpin,
    Room,
}

/// What the app has to do after a click in the chat (the rest the chat does itself).
pub enum ChatAct {
    /// start typing a reply (the compose field gets the keyboard)
    Reply,
    Copy(String),
    Toast(String),
}

pub struct Chat {
    pub msgs: Vec<ChatMsg>,
    pub online: i64,
    pub live: i64,
    pinned: Option<(i64, String)>,
    pub up: bool,
    /// px scrolled up from the newest message: where the wheel wants it, and where it is (eased toward it)
    pub goal: f32,
    pub scroll: f32,
    /// who the chat server says we are (uid, admin, owner, kid, banned, mutedUntil)
    pub you: Option<Value>,
    /// admins (1) and the owner (2) by uid
    staff: HashMap<String, i64>,
    slow: i64,
    locked: bool,
    last: Instant,
    pub stickers: HashMap<String, String>,
    pub out: Option<Sender<String>>,
    started: Instant,
    hits: Vec<([f32; 4], Hit)>,
    /// the message whose action sheet is open
    menu: Option<i64>,
    /// the profile card open (the server's "user" answer), or asked for
    user: Option<Value>,
    user_wait: bool,
    pub room_open: bool,
    /// the message being answered
    pub reply: Option<ChatMsg>,
    typing: Option<(String, Instant)>,
    /// scrolled up: the newest message id seen then (to count what came since)
    seen_upto: Option<i64>,
    older_wait: bool,
    no_older: bool,
    clear_armed: Option<Instant>,
}

pub const TUBE: [f32; 4] = [1436.0, 602.0, 1878.0, 994.0];

fn c8(hex: u32, a: f32) -> [u8; 4] {
    [((hex >> 16) & 255) as u8, ((hex >> 8) & 255) as u8, (hex & 255) as u8, (a.clamp(0.0, 1.0) * 255.0) as u8]
}

const INK: u32 = 0x9dffc4;
const DIM: u32 = 0x6f9c84;
const RED: u32 = 0xff6b5e;

/// A key in the tube: a raised phosphor plate with its label.
fn key(cv: &mut Canvas, r: [f32; 4], label: &str, ink: u32, on: bool) {
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    cv.flat(Quad::flat(r[0], r[1], w, h, rgba(ink, if on { 0.2 } else { 0.08 })).rounded(8.0));
    cv.flat(Quad::rim(r[0], r[1], w, h, 8.0, 1.2, rgba(ink, 0.45), rgba(ink, 0.18)));
    cv.text(Txt::new(label, text::VT, 19.0, (r[0] + r[2]) / 2.0, r[1] + h / 2.0 - 10.0, c8(ink, 1.0)).align(Align::Center).clip(r).glow([110, 255, 170, 60], 2.0));
}

impl Chat {
    pub fn new() -> Self {
        Chat {
            msgs: vec![],
            online: 0,
            live: 0,
            pinned: None,
            up: false,
            goal: 0.0,
            scroll: 0.0,
            you: None,
            staff: HashMap::new(),
            slow: 0,
            locked: false,
            last: Instant::now(),
            stickers: HashMap::new(),
            out: None,
            started: Instant::now(),
            hits: vec![],
            menu: None,
            user: None,
            user_wait: false,
            room_open: false,
            reply: None,
            typing: None,
            seen_upto: None,
            older_wait: false,
            no_older: false,
            clear_armed: None,
        }
    }

    fn me(&self, k: &str) -> bool {
        self.you.as_ref().and_then(|y| y.get(k)).and_then(|v| v.as_bool()).unwrap_or(false)
    }
    fn my_uid(&self) -> String {
        self.you.as_ref().and_then(|y| y.get("uid")).and_then(|v| v.as_str()).unwrap_or("").to_string()
    }
    pub fn admin(&self) -> bool {
        self.me("admin")
    }
    fn owns(&self, m: &ChatMsg) -> bool {
        let kid = self.you.as_ref().and_then(|y| y.get("kid")).and_then(|v| v.as_str());
        m.uid == self.my_uid() || (kid.is_some() && m.kid.as_deref() == kid)
    }
    fn role(&self, uid: &str) -> i64 {
        self.staff.get(uid).copied().unwrap_or(0)
    }
    /// May I block / mute this person (never myself, admins only for the owner, the owner never)?
    fn can_touch(&self, uid: &str) -> bool {
        let r = self.role(uid);
        self.admin() && uid != self.my_uid() && (r == 0 || (r == 1 && self.me("owner")))
    }
    fn send(&self, v: Value) {
        if let Some(o) = &self.out {
            let _ = o.send(v.to_string());
        }
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
                self.you = v.get("you").cloned().filter(|y| !y.is_null());
                self.staff(v.get("staff"));
                self.no_older = false;
            }
            "you" => {
                let mut y = self.you.clone().unwrap_or(json!({}));
                let src = v.get("you").cloned().unwrap_or(v.clone());
                if let (Some(o), Some(s)) = (y.as_object_mut(), src.as_object()) {
                    for (k, x) in s {
                        if k != "type" {
                            o.insert(k.clone(), x.clone());
                        }
                    }
                }
                self.you = Some(y);
            }
            "staff" => self.staff(v.get("staff")),
            "msg" => {
                if let Some(m) = v.get("m").and_then(ChatMsg::from) {
                    if self.typing.as_ref().is_some_and(|t| t.0 == m.name) {
                        self.typing = None;
                    }
                    self.msgs.push(m);
                    if self.msgs.len() > 400 {
                        self.msgs.remove(0);
                    }
                }
            }
            "history" => {
                self.older_wait = false;
                let old: Vec<ChatMsg> = v.get("msgs").and_then(|h| h.as_array()).map(|a| a.iter().filter_map(ChatMsg::from).collect()).unwrap_or_default();
                let first = self.msgs.first().map(|m| m.id).unwrap_or(i64::MAX);
                let old: Vec<ChatMsg> = old.into_iter().filter(|m| m.id < first).collect();
                if old.is_empty() {
                    self.no_older = true;
                }
                self.msgs.splice(0..0, old);
            }
            "reactions" => {
                let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(-1);
                if let Some(m) = self.msgs.iter_mut().find(|m| m.id == id) {
                    m.reactions = reactions_of(v.get("reactions"));
                }
            }
            "face" => {
                let uid = v.get("uid").and_then(|x| x.as_str()).unwrap_or("");
                for m in self.msgs.iter_mut().filter(|m| m.uid == uid) {
                    if let Some(n) = v.get("name").and_then(|x| x.as_str()) {
                        m.name = n.to_string();
                    }
                    m.avatar = v.get("avatar").and_then(|a| a.as_str()).and_then(avatar_url);
                    m.kid = v.get("kid").and_then(|x| x.as_str()).map(String::from);
                }
            }
            "typing" => {
                if let Some(n) = v.get("name").and_then(|x| x.as_str()) {
                    self.typing = Some((n.to_string(), Instant::now()));
                }
            }
            "user" => {
                self.user_wait = false;
                self.user = Some(v.clone());
            }
            "presence" => presence(self, v),
            "room" => self.room(Some(v)),
            "deleted" => {
                let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(-1);
                self.msgs.retain(|m| m.id != id);
                if self.menu == Some(id) {
                    self.menu = None;
                }
            }
            "purged" => {
                let uid = v.get("uid").and_then(|x| x.as_str()).unwrap_or("");
                self.msgs.retain(|m| m.uid != uid);
            }
            "cleared" => self.msgs.clear(),
            "_down" => self.up = false,
            _ => {}
        }
    }

    fn staff(&mut self, s: Option<&Value>) {
        if let Some(o) = s.and_then(|s| s.as_object()) {
            self.staff = o.iter().map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(0))).collect();
        }
    }

    fn room(&mut self, r: Option<&Value>) {
        let Some(r) = r else { return };
        if let Some(s) = r.get("slow").and_then(|x| x.as_i64()) {
            self.slow = s;
        }
        if let Some(l) = r.get("locked").and_then(|x| x.as_bool()) {
            self.locked = l;
        }
        if r.get("pin").is_none() && r.get("pinned").is_none() {
            return;
        }
        let p = r.get("pin").or_else(|| r.get("pinned")).filter(|p| !p.is_null());
        self.pinned = p.and_then(|p| {
            let b = p.get("body").or_else(|| p.get("text")).and_then(|b| b.as_str())?;
            let kind = p.get("kind").and_then(|k| k.as_str()).unwrap_or("text");
            let who = p.get("name").and_then(|n| n.as_str()).map(|n| format!("  — {n}")).unwrap_or_default();
            Some((p.get("id").and_then(|x| x.as_i64()).unwrap_or(-1), preview(kind, b) + &who))
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

    /// Is (x, y) something of the chat's (the tube, or a sheet over it)?
    pub fn wants(&self, x: f32, y: f32) -> bool {
        let [x0, y0, x1, y1] = TUBE;
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }

    /// A click in the tube (`right` = the context button: the message's sheet straight away).
    pub fn click(&mut self, x: f32, y: f32, right: bool) -> Option<ChatAct> {
        let hit = self.hits.iter().rev().find(|(r, _)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]).map(|h| h.1.clone());
        let sheet_open = self.menu.is_some() || self.user.is_some() || self.user_wait || self.room_open;
        match hit {
            Some(Hit::Msg(id)) if !sheet_open => {
                self.menu = Some(id);
            }
            Some(Hit::User(uid)) if !sheet_open && !right => {
                self.user_wait = true;
                self.send(json!({ "type": "whois", "uid": uid }));
            }
            Some(Hit::User(_)) if right => {}
            Some(Hit::React(id, e)) if !sheet_open => self.react(id, &e),
            Some(Hit::Jump) => {
                self.goal = 0.0;
            }
            Some(Hit::Unpin) => self.send(json!({ "type": "pin", "id": null })),
            Some(Hit::Room) if !sheet_open && self.admin() => self.room_open = true,
            Some(Hit::ReplyCancel) => self.reply = None,
            Some(Hit::MenuReact(e)) => {
                if let Some(id) = self.menu.take() {
                    self.react(id, &e);
                }
            }
            Some(Hit::Key(k)) => return self.key(k),
            Some(Hit::Sheet) => {}
            _ => {
                // a click beside an open sheet closes it
                if sheet_open {
                    self.menu = None;
                    self.user = None;
                    self.user_wait = false;
                    self.room_open = false;
                }
            }
        }
        None
    }

    fn react(&mut self, id: i64, e: &str) {
        let me = self.my_uid();
        let mine = self.msgs.iter().find(|m| m.id == id).and_then(|m| m.reactions.iter().find(|r| r.1.contains(&me)).map(|r| r.0.clone()));
        // the same emoji again takes it back
        let emoji = if mine.as_deref() == Some(e) { Value::Null } else { Value::String(e.to_string()) };
        self.send(json!({ "type": "react", "id": id, "emoji": emoji }));
    }

    fn key(&mut self, k: &'static str) -> Option<ChatAct> {
        let m = self.menu.and_then(|id| self.msgs.iter().find(|m| m.id == id).cloned());
        let uid = self.user.as_ref().and_then(|u| u.get("uid")).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let close_user = |c: &mut Chat| c.user = None;
        match k {
            "reply" => {
                self.menu = None;
                self.reply = m;
                return Some(ChatAct::Reply);
            }
            "copy" => {
                self.menu = None;
                return m.map(|m| ChatAct::Copy(m.body.replace("**", "")));
            }
            "profile" => {
                self.menu = None;
                if let Some(m) = m {
                    self.user_wait = true;
                    self.send(json!({ "type": "whois", "uid": m.uid }));
                }
            }
            "pin" => {
                self.menu = None;
                if let Some(m) = m {
                    let pinned = self.pinned.as_ref().is_some_and(|p| p.0 == m.id);
                    self.send(json!({ "type": "pin", "id": if pinned { Value::Null } else { m.id.into() } }));
                }
            }
            "delete" => {
                self.menu = None;
                if let Some(m) = m {
                    self.send(json!({ "type": "delete", "id": m.id }));
                }
            }
            "ban_msg" => {
                self.menu = None;
                if let Some(m) = m {
                    self.send(json!({ "type": "ban", "uid": m.uid, "on": true }));
                }
            }
            "mute" => {
                let muted = self.user.as_ref().and_then(|u| u.get("mutedUntil")).and_then(|x| x.as_i64()).unwrap_or(0) > crate::net::now_ms();
                self.send(json!({ "type": "mute", "uid": uid, "minutes": if muted { 0 } else { 60 } }));
                close_user(self);
            }
            "ban" => {
                let banned = self.user.as_ref().and_then(|u| u.get("banned")).and_then(|x| x.as_bool()).unwrap_or(false);
                self.send(json!({ "type": "ban", "uid": uid, "on": !banned }));
                close_user(self);
            }
            "purge" => {
                self.send(json!({ "type": "purge", "uid": uid }));
                close_user(self);
            }
            "setadmin" => {
                let admin = self.user.as_ref().and_then(|u| u.get("admin")).and_then(|x| x.as_bool()).unwrap_or(false);
                self.send(json!({ "type": "setadmin", "uid": uid, "on": !admin }));
                close_user(self);
            }
            "close" => {
                self.menu = None;
                self.user = None;
                self.room_open = false;
            }
            "slow" => {
                let next = match self.slow {
                    0 => 10,
                    10 => 30,
                    30 => 60,
                    _ => 0,
                };
                self.send(json!({ "type": "room", "slow": next }));
            }
            "lock" => self.send(json!({ "type": "room", "locked": !self.locked })),
            "clear" => {
                // twice within 3 s: it wipes the chat for everybody
                if self.clear_armed.is_some_and(|t| t.elapsed().as_secs_f32() < 3.0) {
                    self.send(json!({ "type": "clear" }));
                    self.clear_armed = None;
                    self.room_open = false;
                } else {
                    self.clear_armed = Some(Instant::now());
                    return Some(ChatAct::Toast("YANA BIR BOSING: HAMMASI O'CHADI".into()));
                }
            }
            _ => {}
        }
        None
    }

    /// The CRT: header, the pinned ticker, the messages (newest at the bottom), the sheets, scanlines over it all.
    /// `img` gives the texture of a picture (and starts loading it if it is not there yet).
    pub fn draw(&mut self, cv: &mut Canvas, over: &mut Canvas, tl: &mut text::TextLayer, tracks: &HashMap<String, crate::net::Track>, img: &mut dyn FnMut(&str) -> Option<usize>) {
        let [x0, y0, x1, y1] = TUBE;
        let (w, h) = (x1 - x0, y1 - y0);
        self.hits.clear();
        let me = self.my_uid();
        // the tube: radial(110% 95% at 50% 45%, rgba(40,70,52,.32), rgba(6,10,8,.55) 75%, rgba(0,0,0,.85))
        cv.flat(Quad::flat(x0, y0, w, h, rgb(0x0c120e)).rounded(32.0));
        cv.flat(Quad::radial(x0, y0, w, h, rgb(0x223a2c), rgb(0x070a08)).rounded(32.0));
        let (px, inner) = (x0 + 22.0, w - 42.0);
        // header: a status line (somebody typing, the room's rules for admins)
        cv.light(Quad::glow(px + 60.0, y0 + 30.0, 50.0, rgba(0x6effaa, 0.12)));
        cv.text(Txt::new("CH·01 CHAT", text::VT, 28.0, px, y0 + 14.0, c8(INK, 1.0)).glow([110, 255, 170, 150], 3.0));
        if self.typing.as_ref().is_some_and(|t| t.1.elapsed().as_secs_f32() > 4.0) {
            self.typing = None;
        }
        let status = if !self.up {
            "qayta ulanmoqda…".to_string()
        } else if let Some((n, _)) = &self.typing {
            format!("{n} yozmoqda…")
        } else {
            let mut s = format!("{} onlayn · {} radioda", self.online, self.live);
            if self.locked {
                s += " · YOPIQ";
            } else if self.slow > 0 {
                s += &format!(" · {}s", self.slow);
            }
            s
        };
        cv.text(Txt::new(status, text::VT, 19.0, px + 132.0, y0 + 21.0, c8(DIM, 1.0)).clip([x0, y0, x1 - 70.0, y1]));
        if self.admin() {
            self.hits.push(([px, y0 + 8.0, x1 - 80.0, y0 + 48.0], Hit::Room));
        }
        let blink = (self.started.elapsed().as_secs_f32() / 0.8) as i32 % 2 == 0;
        cv.text(Txt::new("LIVE", text::VT, 19.0, x1 - 20.0, y0 + 21.0, c8(RED, 1.0)).align(Align::Right));
        if blink {
            cv.light(Quad::glow(x1 - 64.0, y0 + 32.0, 9.0, rgba(0xff5a4d, 0.8)));
            cv.flat(Quad::flat(x1 - 68.0, y0 + 28.0, 8.0, 8.0, rgb(RED)).rounded(4.0));
        }
        let mut top = y0 + 52.0;
        // the pinned message runs along like a ticker (an admin can take it off: ×)
        if let Some((_, p)) = &self.pinned {
            cv.flat(Quad::flat(x0, top, w, 26.0, rgba(0x6effaa, 0.07)));
            cv.flat(Quad::flat(x0 + 10.0, top, 102.0, 26.0, rgb(0x6fe3a0)));
            cv.text(Txt::new("QADALGAN", text::VT, 18.0, x0 + 61.0, top + 4.0, c8(0x07140c, 1.0)).align(Align::Center));
            let t = Txt::new(p.clone(), text::MR, 15.0, 0.0, top + 4.0, c8(0xc9f5d8, 1.0)).weight(500);
            let tw = tl.measure(&t) + 60.0;
            let off = (self.started.elapsed().as_secs_f32() * 40.0) % tw;
            let end = if self.admin() { x1 - 34.0 } else { x1 - 8.0 };
            for k in 0..3 {
                let mut tk = t.clone();
                tk.x = x0 + 122.0 - off + k as f32 * tw;
                tk.clip = [x0 + 116.0, top, end, top + 26.0];
                cv.text(tk);
            }
            if self.admin() {
                cv.text(Txt::new("×", text::VT, 22.0, x1 - 20.0, top + 1.0, c8(INK, 1.0)).align(Align::Center));
                self.hits.push(([x1 - 34.0, top, x1 - 6.0, top + 26.0], Hit::Unpin));
            }
            top += 36.0;
        }

        // a sheet over the bottom of the tube (a message's actions) or over all of it (a profile, the room)
        let sheet_top = if self.user.is_some() || self.user_wait || self.room_open {
            top
        } else if let Some(id) = self.menu {
            match self.msgs.iter().find(|m| m.id == id) {
                Some(m) => y1 - 10.0 - self.menu_height(m),
                None => {
                    self.menu = None;
                    y1
                }
            }
        } else {
            y1
        };
        // the reply being written: a strip at the bottom of the tube
        let reply_h = if self.reply.is_some() { 36.0 } else { 0.0 };
        let bottom = (y1 - 8.0 - reply_h).min(sheet_top - 4.0);

        // messages, laid out bottom-up from the newest: first every message's height, so the scroll range is exact
        // and the view never jumps back at the end; the scroll eases toward where the wheel wants it
        let clip = [x0 + 6.0, top, x1 - 6.0, bottom];
        let text_of = |m: &ChatMsg| Txt::new(m.body.replace("**", ""), text::MR, 17.0, px + 42.0, 0.0, c8(0xe2f6e8, 1.0)).weight(500).wrap(inner - 42.0).line(23.5).glow([110, 255, 170, 60], 2.2);
        let heights: Vec<f32> = self
            .msgs
            .iter()
            .rev()
            .map(|m| {
                let body_h = match m.kind.as_str() {
                    "sticker" => 116.0,
                    "track" => 62.0,
                    _ => tl.measure2(&text_of(m)).1 + 2.0,
                };
                let extra = if m.reply.is_some() { 28.0 } else { 0.0 } + if m.reactions.is_empty() { 0.0 } else { 30.0 };
                (22.0 + body_h + extra).max(34.0)
            })
            .collect();
        let total: f32 = heights.iter().map(|h| h + 12.0).sum();
        let max = (total - (bottom - 6.0 - top)).max(0.0);
        self.goal = self.goal.clamp(0.0, max);
        let dt = self.last.elapsed().as_secs_f32().min(0.1);
        self.last = Instant::now();
        self.scroll += (self.goal - self.scroll) * (1.0 - (-dt / 0.07).exp());
        if (self.goal - self.scroll).abs() < 0.3 {
            self.scroll = self.goal;
        }
        self.scroll = self.scroll.clamp(0.0, max);
        // at the top of what is loaded: ask for older messages
        if self.up && max > 0.0 && self.scroll >= max - 2.0 && !self.older_wait && !self.no_older {
            if let Some(first) = self.msgs.first() {
                self.older_wait = true;
                self.send(json!({ "type": "history", "before": first.id }));
            }
        }
        // scrolled up: remember the newest message seen, to count the ones that came since
        if self.scroll > 60.0 {
            if self.seen_upto.is_none() {
                self.seen_upto = self.msgs.last().map(|m| m.id);
            }
        } else {
            self.seen_upto = None;
        }
        cv.clip = Some(clip);
        let mut y = bottom - 6.0 + self.scroll;
        for (m, mh) in self.msgs.iter().rev().zip(heights) {
            y -= mh;
            if y > bottom {
                y -= 12.0;
                continue;
            }
            if y + mh < top - 4.0 {
                break;
            }
            let ink = name_ink(&m.name);
            let open = self.menu == Some(m.id);
            if open {
                cv.flat(Quad::flat(x0 + 8.0, y - 4.0, w - 16.0, mh + 8.0, rgba(INK, 0.08)).rounded(10.0));
            }
            self.hits.push(([x0 + 6.0, (y - 6.0).max(top), x1 - 6.0, (y + mh + 6.0).min(bottom)], Hit::Msg(m.id)));
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
            let [ir, ig, ib, _] = c8(ink, 1.0);
            let nt = Txt::new(m.name.clone(), text::MR, 15.0, px + 42.0, y, c8(ink, 1.0)).weight(700).clip([clip[0], clip[1], x1 - 70.0, clip[3]]).glow([ir, ig, ib, 110], 2.6);
            let nw = tl.measure(&nt);
            cv.text(nt);
            let role = self.role(&m.uid);
            if role > 0 {
                let tag = if role == 2 { "EGA" } else { "ADMIN" };
                cv.text(Txt::new(tag, text::VT, 15.0, px + 50.0 + nw, y + 2.0, c8(0xffcf7a, 0.9)).clip(clip));
            }
            self.hits.push(([px - 2.0, y, px + 46.0 + nw, y + 36.0], Hit::User(m.uid.clone())));
            cv.text(Txt::new(chrono_hm(m.ts), text::VT, 16.0, x1 - 20.0, y + 2.0, c8(0x5f8a73, 1.0)).align(Align::Right).clip(clip));
            let mut by = y + 22.0;
            // the message it answers: a bar and one quoted line
            if let Some(r) = &m.reply {
                cv.flat(Quad::flat(px + 42.0, by + 2.0, 3.0, 22.0, rgba(INK, 0.6)).rounded(1.5));
                let q = format!("{}: {}", r.name, preview(&r.kind, &r.body));
                cv.text(Txt::new(q, text::MR, 13.0, px + 52.0, by + 4.0, c8(0x8fb89f, 1.0)).weight(500).clip([clip[0], clip[1], x1 - 20.0, clip[3]]));
                by += 28.0;
            }
            match m.kind.as_str() {
                "sticker" => {
                    let u = self.sticker_url(&m.body);
                    let tex = if u.ends_with(".webm") || u.ends_with(".mp4") { None } else { img(&u) };
                    if let Some(t) = tex {
                        let q = Quad::plate_cut(0.0, 0.0, 1920.0, 1080.0, 1.0);
                        cv.quad(Some(t), Quad { rect: [px + 42.0, by + 4.0, 110.0, 110.0], uv: [0.0, 0.0, 1.0, 1.0], ..q });
                    }
                    by += 116.0;
                }
                "track" => {
                    let bx = px + 42.0;
                    let t = tracks.get(&m.body).cloned();
                    let cover = t.as_ref().and_then(|t| img(&crate::net::url(&t.cover_art)));
                    cv.flat(Quad::flat(bx, by + 4.0, 330.0, 56.0, rgba(INK, 0.06)).rounded(8.0));
                    if let Some(c) = cover {
                        let q = Quad::plate_cut(0.0, 0.0, 1920.0, 1080.0, 1.0);
                        cv.quad(Some(c), Quad { rect: [bx + 6.0, by + 10.0, 44.0, 44.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 5.0, ..q });
                    }
                    let (ti, ar) = t.map(|t| (t.title, t.artist)).unwrap_or(("qo'shiq".into(), String::new()));
                    let cl = [bx, by.max(clip[1]), bx + 324.0, (by + 60.0).min(clip[3])];
                    cv.text(Txt::new(format!("♪ {ti}"), text::MR, 15.0, bx + 60.0, by + 12.0, c8(0xe2f6e8, 1.0)).weight(700).clip(cl));
                    cv.text(Txt::new(ar, text::MR, 13.0, bx + 60.0, by + 33.0, c8(0x8fb89f, 1.0)).weight(500).clip(cl));
                    by += 62.0;
                }
                _ => {
                    let mut t = text_of(m);
                    t.y = by + 1.0;
                    t.clip = clip;
                    let (_, th) = tl.measure2(&t);
                    cv.text(t);
                    by += th + 2.0;
                }
            }
            // reactions: little chips under the message (mine lit); a click takes / changes mine
            if !m.reactions.is_empty() {
                let mut rx = px + 42.0;
                for (e, who) in &m.reactions {
                    let mine = who.contains(&me);
                    let label = format!("{e} {}", who.len());
                    let t = Txt::new(label, text::MR, 14.0, rx + 9.0, by + 6.0, c8(if mine { 0xe2f6e8 } else { 0x8fb89f }, 1.0)).weight(700).clip(clip);
                    let cw = tl.measure(&t) + 18.0;
                    cv.flat(Quad::flat(rx, by + 3.0, cw, 24.0, rgba(INK, if mine { 0.22 } else { 0.08 })).rounded(12.0));
                    if mine {
                        cv.flat(Quad::rim(rx, by + 3.0, cw, 24.0, 12.0, 1.2, rgba(INK, 0.6), rgba(INK, 0.3)));
                    }
                    cv.text(t);
                    self.hits.push(([rx, by + 3.0, rx + cw, by + 27.0], Hit::React(m.id, e.clone())));
                    rx += cw + 6.0;
                }
            }
            y -= 12.0;
        }
        cv.clip = None;

        // scrolled up: a round key back to the newest message, with how many came since
        if self.scroll > 60.0 && sheet_top >= y1 {
            let (cx, cy) = (x1 - 42.0, bottom - 34.0);
            cv.flat(Quad::flat(cx - 20.0, cy - 20.0, 40.0, 40.0, rgba(0x07140c, 0.92)).rounded(20.0));
            cv.flat(Quad::rim(cx - 20.0, cy - 20.0, 40.0, 40.0, 20.0, 1.5, rgba(INK, 0.7), rgba(INK, 0.3)));
            cv.text(Txt::new("↓", text::MR, 20.0, cx, cy - 13.0, c8(INK, 1.0)).weight(700).align(Align::Center).glow([110, 255, 170, 120], 2.0));
            let unseen = self.seen_upto.map(|s| self.msgs.iter().filter(|m| m.id > s && m.uid != me).count()).unwrap_or(0);
            if unseen > 0 {
                let t = Txt::new(unseen.min(99).to_string(), text::VT, 16.0, cx + 15.0, cy - 27.0, c8(0x07140c, 1.0)).align(Align::Center);
                cv.flat(Quad::flat(cx + 5.0, cy - 28.0, 20.0, 18.0, rgb(INK)).rounded(9.0));
                cv.text(t);
            }
            self.hits.push(([cx - 22.0, cy - 30.0, cx + 26.0, cy + 22.0], Hit::Jump));
        }
        // the reply strip
        if let (Some(r), true) = (&self.reply, sheet_top >= y1) {
            let ry = y1 - 8.0 - reply_h;
            cv.flat(Quad::flat(x0 + 10.0, ry + 2.0, w - 20.0, reply_h - 4.0, rgba(INK, 0.1)).rounded(10.0));
            cv.flat(Quad::flat(x0 + 20.0, ry + 8.0, 3.0, reply_h - 16.0, rgb(INK)).rounded(1.5));
            let q = format!("↩ {}: {}", r.name, r.preview());
            cv.text(Txt::new(q, text::MR, 14.0, x0 + 30.0, ry + 9.0, c8(0xc9f5d8, 1.0)).weight(500).clip([x0, ry, x1 - 44.0, ry + reply_h]));
            cv.text(Txt::new("×", text::VT, 24.0, x1 - 26.0, ry + 4.0, c8(INK, 1.0)).align(Align::Center));
            self.hits.push(([x1 - 44.0, ry, x1 - 8.0, ry + reply_h], Hit::ReplyCancel));
        }

        if sheet_top < y1 {
            self.draw_sheet(cv, tl, img, [x0 + 10.0, sheet_top, x1 - 10.0, y1 - 10.0]);
        }
        // the tube: scanlines, dark corners
        over.flat(Quad::scan(x0, y0, w, h, 0.28).rounded(32.0));
        crate::scene::lens_light(over, TUBE, 32.0);
    }

    fn menu_height(&self, m: &ChatMsg) -> f32 {
        let mine = self.owns(m);
        let row2 = self.admin() || mine;
        66.0 + 52.0 + 50.0 + if row2 { 50.0 } else { 0.0 } + if m.kind == "text" || !mine { 0.0 } else { 0.0 } + 8.0
    }

    /// The sheets: a message's actions, somebody's profile, the room's settings (admins).
    fn draw_sheet(&mut self, cv: &mut Canvas, tl: &mut text::TextLayer, img: &mut dyn FnMut(&str) -> Option<usize>, r: [f32; 4]) {
        let [sx0, sy0, sx1, sy1] = r;
        let (sw, sh) = (sx1 - sx0, sy1 - sy0);
        cv.flat(Quad::flat(sx0, sy0 + 4.0, sw, sh, rgba(0, 0.5)).rounded(16.0));
        cv.flat(Quad::flat(sx0, sy0, sw, sh, rgba(0x06100b, 0.97)).rounded(16.0));
        cv.flat(Quad::rim(sx0, sy0, sw, sh, 16.0, 1.5, rgba(INK, 0.5), rgba(INK, 0.15)));
        self.hits.push((r, Hit::Sheet));
        let pad = 14.0;
        let row = |i: usize, n: usize, y: f32| -> [f32; 4] {
            let kw = (sw - pad * 2.0 - 8.0 * (n as f32 - 1.0)) / n as f32;
            let x = sx0 + pad + i as f32 * (kw + 8.0);
            [x, y, x + kw, y + 40.0]
        };
        let now = crate::net::now_ms();
        if let Some(id) = self.menu {
            let Some(m) = self.msgs.iter().find(|m| m.id == id).cloned() else { return };
            let mine = self.owns(&m);
            // the message itself, like on an LCD
            let mut y = sy0 + 12.0;
            cv.text(Txt::new(m.name.to_uppercase(), text::MR, 14.0, sx0 + pad, y, c8(name_ink(&m.name), 1.0)).weight(700).clip(r));
            cv.text(Txt::new(chrono_hm(m.ts), text::VT, 16.0, sx1 - pad, y, c8(DIM, 1.0)).align(Align::Right));
            cv.text(Txt::new(m.preview(), text::MR, 15.0, sx0 + pad, y + 22.0, c8(0xe2f6e8, 0.9)).weight(500).clip([sx0, sy0, sx1 - pad, y + 44.0]));
            y += 54.0;
            // the reactions dock
            let me = self.my_uid();
            for (i, e) in REACTIONS.iter().enumerate() {
                let k = row(i, 4, y);
                let picked = m.reactions.iter().any(|(x, who)| x == e && who.contains(&me));
                cv.flat(Quad::flat(k[0], k[1], k[2] - k[0], 44.0, rgba(INK, if picked { 0.22 } else { 0.07 })).rounded(10.0));
                cv.text(Txt::new(*e, text::MR, 24.0, (k[0] + k[2]) / 2.0, k[1] + 7.0, [255, 255, 255, 255]).align(Align::Center));
                self.hits.push(([k[0], k[1], k[2], k[1] + 44.0], Hit::MenuReact(e.to_string())));
            }
            y += 52.0;
            let mut keys1: Vec<(&str, &'static str, u32)> = vec![("JAVOB", "reply", INK)];
            if m.kind == "text" {
                keys1.push(("NUSXA", "copy", INK));
            }
            if !mine {
                keys1.push(("PROFIL", "profile", INK));
            }
            for (i, (l, k, ink)) in keys1.iter().enumerate() {
                let kr = row(i, keys1.len(), y);
                key(cv, kr, l, *ink, false);
                self.hits.push((kr, Hit::Key(k)));
            }
            y += 50.0;
            let mut keys2: Vec<(&str, &'static str, u32)> = vec![];
            if self.admin() {
                let pinned = self.pinned.as_ref().is_some_and(|p| p.0 == m.id);
                keys2.push((if pinned { "QADASHNI OLISH" } else { "QADASH" }, "pin", INK));
            }
            if mine || self.admin() {
                keys2.push(("O'CHIRISH", "delete", RED));
            }
            if !mine && self.can_touch(&m.uid) {
                keys2.push(("BLOKLASH", "ban_msg", RED));
            }
            for (i, (l, k, ink)) in keys2.iter().enumerate() {
                let kr = row(i, keys2.len(), y);
                key(cv, kr, l, *ink, false);
                self.hits.push((kr, Hit::Key(k)));
            }
            return;
        }
        if self.room_open {
            let mut y = sy0 + 16.0;
            cv.text(Txt::new("XONA SOZLAMALARI", text::VT, 26.0, sx0 + pad, y, c8(INK, 1.0)).glow([110, 255, 170, 120], 3.0));
            y += 44.0;
            let slow = if self.slow > 0 { format!("SEKIN REJIM: {} S", self.slow) } else { "SEKIN REJIM: YO'Q".into() };
            for (i, (l, k, ink)) in [(slow.as_str(), "slow", INK), (if self.locked { "CHATNI OCHISH" } else { "CHATNI YOPISH" }, "lock", INK)].iter().enumerate() {
                let kr = row(i, 2, y);
                key(cv, kr, l, *ink, false);
                self.hits.push((kr, Hit::Key(k)));
            }
            y += 50.0;
            let armed = self.clear_armed.is_some_and(|t| t.elapsed().as_secs_f32() < 3.0);
            let kr = row(0, 1, y);
            key(cv, kr, if armed { "TASDIQLANG: HAMMASINI O'CHIRISH" } else { "CHATNI TOZALASH" }, RED, armed);
            self.hits.push((kr, Hit::Key("clear")));
            y += 50.0;
            cv.text(Txt::new("Sekin rejim: har bir tinglovchi xabarlar orasida shuncha kutadi. Yopiq chatda faqat adminlar yozadi.", text::MR, 13.0, sx0 + pad, y + 4.0, c8(DIM, 1.0)).wrap(sw - pad * 2.0).line(19.0));
            let kr = [sx1 - pad - 110.0, sy1 - 52.0, sx1 - pad, sy1 - 12.0];
            key(cv, kr, "YOPISH", INK, false);
            self.hits.push((kr, Hit::Key("close")));
            return;
        }
        // a profile
        let Some(u) = self.user.clone() else {
            cv.text(Txt::new("YUKLANMOQDA…", text::VT, 24.0, (sx0 + sx1) / 2.0, sy0 + sh / 2.0 - 12.0, c8(INK, 1.0)).align(Align::Center));
            return;
        };
        let s = |k: &str| u.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let b = |k: &str| u.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
        let n = |k: &str| u.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        let uid = s("uid");
        let name = s("name");
        let mut y = sy0 + 16.0;
        let av = u.get("avatar").and_then(|a| a.as_str()).and_then(avatar_url).and_then(|x| img(&x));
        if let Some(a) = av {
            let q = Quad::plate_cut(0.0, 0.0, 1920.0, 1080.0, 1.0);
            cv.quad(Some(a), Quad { rect: [sx0 + pad, y, 64.0, 64.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 12.0, ..q });
        } else {
            let hsh: u32 = name.chars().map(|c| c as u32).sum();
            cv.flat(Quad::flat(sx0 + pad, y, 64.0, 64.0, rgb(GLYPH[(hsh % 6) as usize])).rounded(12.0));
            let ini: String = name.trim().chars().next().map(|c| c.to_uppercase().collect()).unwrap_or("?".into());
            cv.text(Txt::new(ini, text::VT, 40.0, sx0 + pad + 32.0, y + 10.0, c8(0x07140c, 1.0)).align(Align::Center));
        }
        let tx = sx0 + pad + 78.0;
        cv.text(Txt::new(name.clone(), text::MR, 20.0, tx, y + 2.0, c8(name_ink(&name), 1.0)).weight(800).clip([tx, sy0, sx1 - pad, sy1]).glow([255, 255, 255, 40], 2.0));
        let acc = u.get("account").filter(|a| !a.is_null());
        let handle = acc.and_then(|a| a.get("username")).and_then(|x| x.as_str()).map(|h| format!("@{h} · Kawaii ID")).unwrap_or_else(|| "mehmon".into());
        cv.text(Txt::new(handle, text::MR, 13.0, tx, y + 30.0, c8(0x8fb89f, 1.0)).weight(500).clip([tx, sy0, sx1 - pad, sy1]));
        let mut badges = vec![];
        if b("owner") {
            badges.push(("EGA", 0xffcf7a));
        } else if b("admin") {
            badges.push(("ADMIN", 0xffcf7a));
        }
        if b("banned") {
            badges.push(("BLOKLANGAN", RED));
        }
        if n("mutedUntil") > now {
            badges.push(("OVOZSIZ", RED));
        }
        if b("online") {
            badges.push((if b("live") { "RADIODA" } else { "ONLAYN" }, INK));
        }
        let mut bx = tx;
        for (t, c) in badges {
            let tt = Txt::new(t, text::VT, 16.0, bx + 7.0, y + 48.0, c8(c, 1.0));
            let bw = tl.measure(&tt) + 14.0;
            cv.flat(Quad::flat(bx, y + 48.0, bw, 20.0, rgba(c, 0.14)).rounded(6.0));
            cv.text(tt);
            bx += bw + 6.0;
        }
        y += 84.0;
        // the numbers, like an LCD row
        let lvl = acc.and_then(|a| a.get("level")).and_then(|x| x.as_i64());
        let rank = acc.and_then(|a| a.get("displayLabel").or(a.get("displayRank")).or(a.get("rank"))).and_then(|x| x.as_str()).unwrap_or("—").to_string();
        let joined = n("joined");
        let days = if joined > 0 { ((now - joined) / 86_400_000).max(0) } else { 0 };
        let cells = [
            ("LEVEL", lvl.map(|l| l.to_string()).unwrap_or("—".into())),
            ("RANK", rank),
            ("XABAR", n("messages").to_string()),
            ("KUN", days.to_string()),
        ];
        let cw = (sw - pad * 2.0) / 4.0;
        cv.flat(Quad::flat(sx0 + pad, y, sw - pad * 2.0, 56.0, rgba(0xa8c48a, 0.9)).rounded(10.0));
        for (i, (k, v)) in cells.iter().enumerate() {
            let cx = sx0 + pad + cw * (i as f32 + 0.5);
            cv.text(Txt::new(*k, text::BAR, 11.0, cx, y + 7.0, c8(0x232b1b, 0.7)).weight(600).align(Align::Center));
            cv.text(Txt::new(v.clone(), text::VT, 24.0, cx, y + 22.0, c8(0x232b1b, 1.0)).align(Align::Center).clip([cx - cw / 2.0 + 4.0, y, cx + cw / 2.0 - 4.0, y + 56.0]));
        }
        y += 68.0;
        if let Some(t) = u.get("card").and_then(|c| c.get("now")).and_then(|x| x.as_str()) {
            cv.text(Txt::new(format!("♪ hozir tinglayapti: {t}"), text::MR, 13.0, sx0 + pad, y, c8(0x8fb89f, 1.0)).weight(500).clip([sx0, sy0, sx1 - pad, sy1]));
            y += 24.0;
        } else if !b("online") && n("seen") > 0 {
            let ago = (now - n("seen")) / 60_000;
            let t = if ago < 60 { format!("oxirgi marta: {ago} daqiqa oldin") } else if ago < 1440 { format!("oxirgi marta: {} soat oldin", ago / 60) } else { format!("oxirgi marta: {} kun oldin", ago / 1440) };
            cv.text(Txt::new(t, text::MR, 13.0, sx0 + pad, y, c8(0x8fb89f, 1.0)).weight(500));
            y += 24.0;
        }
        // moderation (who may touch whom: like the server's canTouch)
        let mut keys: Vec<(String, &'static str, u32)> = vec![];
        if self.me("owner") && uid != self.my_uid() && !b("owner") {
            keys.push((if b("admin") { "ADMINLIKDAN OLISH".into() } else { "ADMIN QILISH".into() }, "setadmin", 0xffcf7a));
        }
        if self.can_touch(&uid) {
            keys.push((if n("mutedUntil") > now { "OVOZNI YOQISH".into() } else { "OVOZSIZ 1 SOAT".into() }, "mute", INK));
            keys.push((if b("banned") { "BLOKDAN CHIQARISH".into() } else { "BLOKLASH".into() }, "ban", RED));
            keys.push(("XABARLARINI O'CHIRISH".into(), "purge", RED));
        }
        let mut i = 0;
        while i < keys.len() {
            let n2 = (keys.len() - i).min(2);
            for j in 0..n2 {
                let (l, k, ink) = &keys[i + j];
                let kr = row(j, n2, y);
                key(cv, kr, l, *ink, false);
                self.hits.push((kr, Hit::Key(k)));
            }
            i += n2;
            y += 48.0;
        }
        let kr = [sx1 - pad - 110.0, sy1 - 52.0, sx1 - pad, sy1 - 12.0];
        key(cv, kr, "YOPISH", INK, false);
        self.hits.push((kr, Hit::Key("close")));
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

/// The listeners VFD (#lis: 812,120 552x50): a counter, and a row of pixel people that bob to the music.
pub fn draw_listeners(cv: &mut Canvas, n: i64, t: f32, level: f32) {
    let (x, y, w, h) = (812.0, 120.0, 552.0, 50.0);
    let ink = 0x7dffcf;
    // the smoked window's dot mesh
    cv.mask = Some(([x, y, x + w, y + h], 12.0));
    cv.flat(Quad::dots(x, y, w, h, 4.0, 1.0, rgba(ink, 0.06)));
    cv.mask = None;
    cv.text(Txt::new(format!("{:03}", n.clamp(0, 999)), text::VT, 36.0, x + 14.0, y - 3.0, c8(ink, 1.0)).glow([125, 255, 207, 140], 3.0));
    cv.text(Txt::new("T I N G L O V C H I", text::BAR, 9.0, x + 16.0, y + 38.0, c8(ink, 0.5)).weight(600));
    // a pixel person (7 x 11 cells of 2 px): head, body, arms, legs
    const P: [(f32, f32, f32, f32); 6] = [(2., 0., 3., 3.), (1., 4., 5., 4.), (0., 4., 1., 3.), (6., 4., 1., 3.), (1., 8., 2., 3.), (4., 8., 2., 3.)];
    const N: usize = 20;
    let x_end = x + w - 14.0;
    for k in 0..N {
        let on = (k as i64) < n;
        let px = x_end - (N - k) as f32 * 20.0 + 6.0;
        let bob = if on { ((t / 0.16 + k as f32 * 1.3).sin().max(0.0)) * level * 4.0 } else { 0.0 };
        let py = y + h - 10.0 - 22.0 - bob;
        if on {
            cv.light(Quad::glow(px + 7.0, py + 11.0, 13.0, rgba(ink, 0.22)));
        }
        for (rx, ry, rw, rh) in P {
            cv.flat(Quad::flat(px + rx * 2.0, py + ry * 2.0, rw * 2.0, rh * 2.0, rgba(ink, if on { 1.0 } else { 0.09 })));
        }
    }
}
