// KR-01 DESK, native: no browser inside. The device is a set of rendered plates (Blender), drawn on the GPU with
// wgpu; everything live (LEDs, VU needles, pressed keys, …) is quads on top. Audio is decoded and played by the
// app itself (symphonia + cpal), so it needs no system codecs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod chat;
mod store;
mod gfx;
mod kid;
mod net;
mod panel;
mod receipt;
mod scene;
mod text;

use gfx::{Batch, Gfx, Quad};
use scene::Canvas;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key as WKey, NamedKey};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

const PLATE_UP: &[u8] = include_bytes!("../../app/ui/assets/dark-up.webp");
const PLATE_DOWN: &[u8] = include_bytes!("../../app/ui/assets/dark-down.webp");
const GLASS: &[u8] = include_bytes!("../../app/ui/assets/glass-dark.webp");

/// What the network threads hand to the window thread.
enum Msg {
    Tracks(HashMap<String, net::Track>),
    /// an mp3 to play now, from `offset`, for this station / track
    Tune { station: String, track: String, mp3: Vec<u8>, offset: Duration, start: i64, dur: i64, skew: i64 },
    Cover { track: String, img: image::RgbaImage },
    Chat(serde_json::Value),
    Image { url: String, img: Option<image::RgbaImage> },
    Stickers(HashMap<String, String>),
    /// a kawaii:// link handed over by another copy (or "focus")
    Link(String),
    /// the signed-in account (None = the token is no good any more)
    Account(Option<serde_json::Value>),
    Toast(String),
    /// an mp3 of the listener's own choice (O'Z TANLOVIM), played from the start
    Pick { track: String, mp3: Vec<u8> },
    /// the account's progress after a sync: likes (id -> time), unliked, playlists
    Synced(serde_json::Value),
    Error(String),
}

const STATIONS: [&str; 3] = ["jpop", "ost", "riot"];

/// A clickable part of the device (stage px) and how its pressed look is cut from the "down" plate.
struct Key {
    id: &'static str,
    rect: [f32; 4],
    radius: f32,
}

fn keys() -> Vec<Key> {
    let mut k = vec![];
    for i in 0..4 {
        let y = 28.0 + i as f32 * 104.0;
        k.push(Key { id: ["nav0", "nav1", "nav2", "nav3"][i], rect: [26.0, y, 102.0, y + 84.0], radius: 20.0 });
    }
    k.push(Key { id: "gear", rect: [26.0, 984.0, 102.0, 1052.0], radius: 20.0 });
    for i in 0..3 {
        let x = 156.0 + i as f32 * 216.0;
        k.push(Key { id: ["st0", "st1", "st2"][i], rect: [x, 118.0, x + 200.0, 172.0], radius: 15.0 });
    }
    k.push(Key { id: "play", rect: [140.0, 866.0, 232.0, 958.0], radius: 46.0 });
    k.push(Key { id: "heart", rect: [773.0, 865.0, 867.0, 959.0], radius: 47.0 });
    k.push(Key { id: "tavsif", rect: [128.0, 982.0, 498.0, 1058.0], radius: 24.0 });
    k.push(Key { id: "saqlash", rect: [508.0, 982.0, 878.0, 1058.0], radius: 24.0 });
    k.push(Key { id: "prev", rect: [1036.0, 862.0, 1096.0, 956.0], radius: 18.0 });
    k.push(Key { id: "next", rect: [1036.0, 962.0, 1096.0, 1056.0], radius: 18.0 });
    k.push(Key { id: "slider", rect: [843.0, 41.0, 1361.0, 95.0], radius: 15.0 });
    // the chat monitor's input row: the round ID key, the LCD strip, the round smile key
    k.push(Key { id: "cid", rect: [1440.0, 1007.0, 1480.0, 1047.0], radius: 20.0 });
    k.push(Key { id: "compose", rect: [1494.0, 1010.0, 1822.0, 1046.0], radius: 6.0 });
    k.push(Key { id: "csm", rect: [1834.0, 1007.0, 1874.0, 1047.0], radius: 20.0 });
    for (id, cx) in [("min", 1592.0), ("max", 1628.0), ("close", 1664.0)] {
        k.push(Key { id, rect: [cx - 13.0, 45.0, cx + 13.0, 71.0], radius: 13.0 });
    }
    k
}

struct State {
    gfx: Gfx,
    up: usize,
    down: usize,
    glass: usize,
    player: Option<audio::Player>,
    keys: Vec<Key>,
    pressed: Option<usize>,
    cursor: (f32, f32),
    station: String,
    playing: bool,
    volume: f32,
    tracks: HashMap<String, net::Track>,
    now: Option<String>,
    /// smoothed meter values and the VU needles' spring state (angle, speed)
    level: f32,
    leds: [f32; 18],
    vu: [(f32, f32); 2],
    last: Instant,
    /// a tune request in flight (so the poller does not ask twice)
    tuning: bool,
    next_check: Instant,
    text: text::TextLayer,
    lyrics: scene::Lyrics,
    tv: scene::Trivision,
    wave: Vec<f32>,
    /// the slot on air: its start (server ms), length (ms), and the server clock's skew
    slot: (i64, i64, i64),
    drum: f32,
    drum_kick: f32,
    over: Vec<Batch>,
    texts: Vec<text::Txt>,
    covers: HashMap<String, usize>,
    store: store::Store,
    chat: chat::Chat,
    /// pictures by URL: Some(texture) once loaded, None while loading (or failed)
    images: HashMap<String, Option<usize>>,
    started: Instant,
    proxy: EventLoopProxy<Msg>,
    token: Option<String>,
    acc: Option<serde_json::Value>,
    login_state: Option<String>,
    /// the message being typed (Some = the field has the keyboard)
    compose: Option<String>,
    tray: bool,
    toast: Option<(String, Instant)>,
    uid: (String, String),
    panel: panel::Panel,
    tape: receipt::Tape,
    /// O'Z TANLOVIM: the listener's own choice instead of the live station
    pick: bool,
    pick_list: Vec<String>,
    search_focus: bool,
    liked: Vec<String>,
    playlists: Vec<(String, String, Vec<String>)>,
    syncing: bool,
    last_sync: Instant,
}

struct App {
    proxy: EventLoopProxy<Msg>,
    st: Option<State>,
}

fn tune(proxy: EventLoopProxy<Msg>, station: String) {
    std::thread::spawn(move || {
        let res = (|| -> Result<Msg, String> {
            let sch = net::schedule(&station)?;
            let (slot, into) = sch.on_air().ok_or("efir bo'sh")?;
            let tracks = net::tracks()?;
            let t = tracks.get(&slot.id).ok_or("trek topilmadi")?;
            let t0 = Instant::now();
            let mp3 = net::bytes(&t.audio_src)?;
            // the download took a while: the station moved on, so start that much later
            let offset = Duration::from_millis(into.max(0) as u64) + t0.elapsed();
            let cover = t.cover_art.clone();
            let (id, p2) = (slot.id.clone(), proxy.clone());
            // the cover for the trivision, decoded off the window thread
            std::thread::spawn(move || {
                if let Ok(b) = net::bytes(&cover) {
                    if let Ok(img) = image::load_from_memory(&b) {
                        let img = image::imageops::resize(&img.to_rgba8(), 436, 432, image::imageops::FilterType::Triangle);
                        let _ = p2.send_event(Msg::Cover { track: id, img });
                    }
                }
            });
            Ok(Msg::Tune { station, track: slot.id.clone(), mp3, offset, start: slot.start, dur: slot.duration, skew: sch.skew })
        })();
        let _ = proxy.send_event(res.unwrap_or_else(Msg::Error));
    });
}

impl State {
    fn toast(&mut self, t: &str) {
        self.toast = Some((t.to_string(), Instant::now()));
    }
    fn login(&mut self) {
        let s = kid::new_state();
        self.login_state = Some(s.clone());
        kid::open_url(&format!("{}/auth/start?s={s}&d=1", kid::CLOUD));
        self.toast("KAWAII ID · BRAUZERDA KIRING");
    }
    /// /auth/me with this computer's chat identity (that also links the chat to the account)
    fn refresh_account(&self) {
        let (Some(t), p) = (self.token.clone(), self.proxy.clone()) else { return };
        let (u, k) = self.uid.clone();
        std::thread::spawn(move || {
            let r = kid::api("/auth/me", &t, &u, &k, None);
            let acc = match r {
                Ok((200, b)) => serde_json::from_str::<serde_json::Value>(&b).ok().and_then(|j| j.get("account").cloned()),
                Ok((401, _)) => None,
                _ => return,
            };
            let _ = p.send_event(Msg::Account(acc));
        });
    }
    /// tell the chat who we are now
    fn chat_profile(&self) {
        let Some(out) = &self.chat.out else { return };
        let (name, avatar) = match &self.acc {
            Some(a) => (
                a.get("name").and_then(|v| v.as_str()).or(a.get("username").and_then(|v| v.as_str())).unwrap_or("Mehmon").to_string(),
                a.get("avatar").and_then(|v| v.as_str()).map(|u| format!("k:{u}")).unwrap_or("p:0".into()),
            ),
            None => ("Mehmon".into(), "p:0".into()),
        };
        let _ = out.send(serde_json::json!({ "type": "profile", "name": name, "avatar": avatar, "card": { "now": self.now, "likes": 0, "plays": 0, "minutes": 0 } }).to_string());
    }
    fn send_chat(&mut self, kind: &str, body: &str) {
        if !self.chat.up {
            self.toast("CHAT · ULANMOQDA");
            return;
        }
        if let Some(out) = &self.chat.out {
            let nonce = format!("{:x}", net::now_ms());
            let _ = out.send(serde_json::json!({ "type": "send", "kind": kind, "body": body, "nonce": nonce }).to_string());
        }
        self.chat.scroll = 0.0;
    }

    fn has_kp(&self) -> bool {
        let Some(a) = &self.acc else { return false };
        a.get("kpUntil").and_then(|v| v.as_i64()).is_some_and(|t| t > net::now_ms())
            || a.get("adm").and_then(|v| v.as_bool()).unwrap_or(false)
            || a.get("roles").and_then(|v| v.as_array()).is_some_and(|r| !r.is_empty())
    }
    fn toggle_like(&mut self) {
        let Some(id) = self.now.clone() else { return };
        let mut la: serde_json::Map<String, serde_json::Value> = self.store.get("likesAt").and_then(|v| v.as_object().cloned()).unwrap_or_default();
        let mut ua: serde_json::Map<String, serde_json::Value> = self.store.get("unlikedAt").and_then(|v| v.as_object().cloned()).unwrap_or_default();
        if let Some(i) = self.liked.iter().position(|x| *x == id) {
            self.liked.remove(i);
            la.remove(&id);
            ua.insert(id.clone(), net::now_ms().into());
            self.toast("SEVIMLILARDAN OLINDI");
        } else {
            self.liked.push(id.clone());
            la.insert(id.clone(), net::now_ms().into());
            ua.remove(&id);
            self.toast("SEVIMLILARGA QO'SHILDI");
        }
        self.store.set("likesAt", la.into());
        self.store.set("unlikedAt", ua.into());
        self.store.set("liked", serde_json::to_value(&self.liked).unwrap());
        self.last_sync = Instant::now() - Duration::from_secs(297);
    }
    /// The account sync (like the phone's Sync.kt): GET the account's copy, merge likes by time, keep its
    /// playlists, write only this computer's own listening slot, POST the whole thing back.
    fn sync(&mut self) {
        let Some(token) = self.token.clone() else { return };
        if self.syncing {
            return;
        }
        self.syncing = true;
        self.last_sync = Instant::now();
        let (u, k) = self.uid.clone();
        let mut la: serde_json::Map<String, serde_json::Value> = self.store.get("likesAt").and_then(|v| v.as_object().cloned()).unwrap_or_default();
        for id in &self.liked {
            la.entry(id.clone()).or_insert(net::now_ms().into());
        }
        let ua = self.store.get("unlikedAt").and_then(|v| v.as_object().cloned()).unwrap_or_default();
        let slot = format!("desk-{}", u.get(5..13).unwrap_or("native"));
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let r = (|| -> Option<serde_json::Value> {
                let (st, body) = kid::api("/auth/sync", &token, &u, &k, None).ok()?;
                if st != 200 {
                    return None;
                }
                let remote = serde_json::from_str::<serde_json::Value>(&body).ok()?.get("data").cloned().filter(|v| v.is_object()).unwrap_or(serde_json::json!({}));
                let now = net::now_ms();
                let cutoff = now - 180 * 86_400_000;
                let max = |a: &serde_json::Map<String, serde_json::Value>, b: Option<&serde_json::Value>| {
                    let mut o = a.clone();
                    if let Some(b) = b.and_then(|b| b.as_object()) {
                        for (k, v) in b {
                            let x = v.as_i64().unwrap_or(0).max(o.get(k).and_then(|y| y.as_i64()).unwrap_or(0));
                            o.insert(k.clone(), x.into());
                        }
                    }
                    o
                };
                let liked_at = max(&la, remote.get("likes"));
                let un_at: serde_json::Map<_, _> = max(&ua, remote.get("unliked")).into_iter().filter(|(_, t)| t.as_i64().unwrap_or(0) > cutoff).collect();
                let mut likes: Vec<(String, i64)> = liked_at.iter().map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(0))).filter(|(k, t)| *t > un_at.get(k).and_then(|x| x.as_i64()).unwrap_or(0)).collect();
                likes.sort_by_key(|x| x.1);
                let likes_m: serde_json::Map<String, serde_json::Value> = likes.iter().map(|(k, t)| (k.clone(), (*t).into())).collect();
                let unliked: serde_json::Map<String, serde_json::Value> = un_at.into_iter().filter(|(k, _)| !likes_m.contains_key(k)).collect();
                let mut devices = remote.get("devices").and_then(|d| d.as_object().cloned()).unwrap_or_default();
                let mut mine = devices.get(&slot).cloned().unwrap_or(serde_json::json!({}));
                mine["seen"] = now.into();
                if mine.get("stats").is_none() {
                    mine["stats"] = serde_json::json!({});
                }
                devices.insert(slot.clone(), mine);
                let mut merged = remote.clone();
                merged["likes"] = likes_m.into();
                merged["unliked"] = unliked.into();
                if merged.get("playlists").is_none() {
                    merged["playlists"] = serde_json::json!([]);
                }
                merged["devices"] = devices.into();
                let (st2, _) = kid::api("/auth/sync", &token, &u, &k, Some(&merged.to_string())).ok()?;
                (st2 == 200).then_some(merged)
            })();
            if let Some(m) = r {
                let _ = p.send_event(Msg::Synced(m));
            }
        });
    }
    fn play_pick(&mut self, id: &str) {
        let Some(t) = self.tracks.get(id).cloned() else { return };
        self.pick = true;
        self.playing = true;
        self.tuning = true;
        let p = self.proxy.clone();
        let id = id.to_string();
        std::thread::spawn(move || match net::bytes(&t.audio_src) {
            Ok(mp3) => {
                let _ = p.send_event(Msg::Pick { track: id, mp3 });
            }
            Err(e) => {
                let _ = p.send_event(Msg::Error(e));
            }
        });
    }
    fn pick_step(&mut self, d: i64) {
        let list = if self.pick_list.is_empty() { self.tracks.keys().cloned().collect::<Vec<_>>() } else { self.pick_list.clone() };
        if list.is_empty() {
            return;
        }
        let i = self.now.as_ref().and_then(|n| list.iter().position(|x| x == n)).map(|i| i as i64).unwrap_or(-1);
        let n = (i + d).rem_euclid(list.len() as i64) as usize;
        let id = list[n].clone();
        self.play_pick(&id);
    }
    fn act(&mut self, a: panel::Act) {
        use panel::Act;
        match a {
            Act::Close => self.panel.screen = panel::Screen::Home,
            Act::Back => {
                self.panel.open = None;
                self.panel.scroll = 0.0;
            }
            Act::OpenList(id) => {
                self.panel.open = Some(id);
                self.panel.scroll = 0.0;
            }
            Act::FocusSearch => self.search_focus = true,
            Act::Login => self.login(),
            Act::Logout => {
                if let Some(t) = self.token.take() {
                    let (u, k) = self.uid.clone();
                    std::thread::spawn(move || {
                        let _ = kid::api("/auth/logout", &t, &u, &k, None);
                    });
                }
                self.acc = None;
                self.store.set("kidToken", serde_json::Value::Null);
                self.store.set("kidAcc", serde_json::Value::Null);
                self.chat_profile();
                self.toast("KAWAII ID · CHIQILDI");
            }
            Act::Size(s) => {
                let w = match s { "s" => 800.0, "m" => 1120.0, "l" => 1440.0, _ => 1920.0 };
                let _ = self.gfx.window.request_inner_size(LogicalSize::new(w, w * 9.0 / 16.0));
            }
            Act::Full => {
                let w = &self.gfx.window;
                w.set_fullscreen(if w.fullscreen().is_some() { None } else { Some(winit::window::Fullscreen::Borderless(None)) });
            }
            Act::Update => {}
            Act::Play(id) => {
                if self.has_kp() {
                    let cx_list = self.panel_list_ids();
                    self.pick_list = cx_list;
                    self.play_pick(&id);
                } else {
                    if let Some(t) = self.tracks.get(&id).cloned() {
                        self.tape.print(&t, &mut self.text, true);
                    }
                    self.toast(if self.now.as_deref() == Some(id.as_str()) { "HOZIR EFIRDA" } else { "O'Z TANLOVIM · KAWAII PASS KERAK" });
                }
            }
        }
    }
    fn panel_list_ids(&self) -> Vec<String> {
        let cx = self.panel_ctx();
        self.panel.list(&cx).into_iter().map(|t| t.id).collect()
    }
    fn panel_ctx(&self) -> panel::Ctx<'_> {
        panel::Ctx {
            tracks: &self.tracks,
            liked: &self.liked,
            playlists: &self.playlists,
            acc: self.acc.as_ref(),
            now: self.now.as_deref(),
            version: env!("CARGO_PKG_VERSION"),
            update: None,
            search_focus: self.search_focus,
            blink: (self.started.elapsed().as_secs_f32() / 0.55) as i32 % 2 == 0,
            minutes: self.store.get("listened").and_then(|v| v.as_u64()).unwrap_or(0) / 60,
            songs: self.store.get("songs").and_then(|v| v.as_u64()).unwrap_or(0),
        }
    }

    fn key_at(&self, x: f32, y: f32) -> Option<usize> {
        self.keys.iter().position(|k| x >= k.rect[0] && x <= k.rect[2] && y >= k.rect[1] && y <= k.rect[3])
    }

    fn on_click(&mut self, id: &str, proxy: &EventLoopProxy<Msg>, el: &ActiveEventLoop) {
        match id {
            "close" => el.exit(),
            "min" => self.gfx.window.set_minimized(true),
            "max" => {
                let w = &self.gfx.window;
                w.set_maximized(!w.is_maximized());
            }
            "play" => {
                self.playing = !self.playing;
                if let Some(p) = &self.player {
                    if self.playing && p.idle() {
                        self.tuning = true;
                        tune(proxy.clone(), self.station.clone());
                    }
                    p.set_paused(!self.playing);
                }
            }
            "nav0" | "nav1" | "nav2" | "nav3" | "gear" => {
                let s = match id {
                    "nav1" => panel::Screen::Search,
                    "nav2" => panel::Screen::Library,
                    "nav3" => panel::Screen::Profile,
                    "gear" => panel::Screen::Settings,
                    _ => panel::Screen::Home,
                };
                self.panel.screen = if self.panel.screen == s { panel::Screen::Home } else { s };
                self.panel.scroll = 0.0;
                self.search_focus = self.panel.screen == panel::Screen::Search;
            }
            "heart" => self.toggle_like(),
            "tavsif" => {
                if let Some(t) = self.now.as_ref().and_then(|i| self.tracks.get(i)).cloned() {
                    self.tape.print(&t, &mut self.text, true);
                }
            }
            "saqlash" => self.toast("SAQLASH · TEZ ORADA"),
            "prev" | "next" => {
                if self.pick {
                    self.pick_step(if id == "next" { 1 } else { -1 });
                } else {
                    self.toast("JONLI EFIR · O'TKAZIB BO'LMAYDI");
                }
            }
            "slider" => {
                if self.pick {
                    self.pick = false;
                    self.tuning = true;
                    tune(proxy.clone(), self.station.clone());
                    self.toast("RADIO");
                } else if self.has_kp() {
                    self.panel.screen = panel::Screen::Search;
                    self.search_focus = true;
                    self.toast("O'Z TANLOVIM · QO'SHIQ TANLANG");
                } else {
                    self.toast("O'Z TANLOVIM · KAWAII PASS BILAN");
                }
            }
            "cid" | "compose" => {
                if self.acc.is_some() {
                    self.compose.get_or_insert_with(String::new);
                } else {
                    self.login();
                }
            }
            "csm" => {
                if self.acc.is_some() {
                    self.tray = !self.tray;
                } else {
                    self.login();
                }
            }
            "st0" | "st1" | "st2" => {
                let st = STATIONS[id[2..].parse::<usize>().unwrap()].to_string();
                if st != self.station || self.pick {
                    self.pick = false;
                    self.station = st.clone();
                    self.playing = true;
                    self.tuning = true;
                    tune(proxy.clone(), st);
                }
            }
            _ => {}
        }
    }

    fn tray_stickers(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self.chat.stickers.iter().filter(|(_, f)| !(f.ends_with(".webm") || f.ends_with(".mp4"))).map(|(a, b)| (a.clone(), b.clone())).collect();
        v.sort();
        v.truncate(30);
        v
    }
    /// The sticker under (x, y) in the tray (6 per row over the bottom of the tube).
    fn tray_pick(&self, x: f32, y: f32) -> Option<String> {
        let (tx, ty) = (1452.0, 994.0 - 8.0 - 5.0 * 66.0);
        if x < tx || y < ty || x > tx + 6.0 * 68.0 || y > 994.0 - 8.0 {
            return None;
        }
        let (c, r) = (((x - tx) / 68.0) as usize, ((y - ty) / 66.0) as usize);
        self.tray_stickers().get(r * 6 + c).map(|s| s.0.clone())
    }

    /// Is this key shown pressed / lit right now (latched keys stay down)?
    fn key_down(&self, i: usize) -> bool {
        let id = self.keys[i].id;
        if self.pressed == Some(i) {
            return true;
        }
        match id {
            "play" => self.playing,
            "nav0" => self.panel.screen == panel::Screen::Home,
            "nav1" => self.panel.screen == panel::Screen::Search,
            "nav2" => self.panel.screen == panel::Screen::Library,
            "nav3" => self.panel.screen == panel::Screen::Profile,
            "gear" => self.panel.screen == panel::Screen::Settings,
            "heart" => self.now.as_ref().is_some_and(|n| self.liked.contains(n)),
            "slider" => self.pick,
            "st0" | "st1" | "st2" => !self.pick && STATIONS[id[2..].parse::<usize>().unwrap()] == self.station,
            _ => false,
        }
    }

    fn frame(&mut self) -> Vec<Batch> {
        let dt = self.last.elapsed().as_secs_f32().min(0.1);
        self.last = Instant::now();
        let (l, r) = self.player.as_ref().map(|p| p.levels.get()).unwrap_or((0.0, 0.0));
        let (l, r) = if self.playing { (l, r) } else { (0.0, 0.0) };

        // LEVEL: 18 LEDs, the top one glows partly, each fades with a short afterglow
        let v = ((l + r) * 0.5 * 3.2).min(1.0);
        self.level = if v > self.level { v } else { self.level * 0.9 + v * 0.1 };
        let x = self.level * 18.0;
        for j in 0..18 {
            let want = (x - (17 - j) as f32).clamp(0.0, 1.0);
            self.leds[j] = if want > self.leds[j] { want } else { self.leds[j] * (0.82f32).powf(dt * 60.0) };
        }
        // VU needles: dB -> angle on the printed scale, a damped spring (the needle has mass)
        const VU_DB: [(f32, f32); 11] = [(-20., -48.), (-10., -28.), (-7., -17.), (-5., -8.), (-3., 3.), (-2., 10.), (-1., 17.), (0., 25.), (1., 32.), (2., 40.), (3., 48.)];
        let ang = |rms: f32| -> f32 {
            if rms <= 1e-5 {
                return -54.0;
            }
            let db = 20.0 * rms.log10() + 14.0;
            if db <= -20.0 {
                return (-48.0 - (-20.0 - db) * 0.4).max(-54.0);
            }
            for w in VU_DB.windows(2) {
                if db <= w[1].0 {
                    return w[0].1 + (w[1].1 - w[0].1) * (db - w[0].0) / (w[1].0 - w[0].0);
                }
            }
            50.0
        };
        for (k, rms) in [l, r].into_iter().enumerate() {
            let want = ang(rms);
            let (a, s) = &mut self.vu[k];
            let steps = (dt * 60.0).max(1.0) as i32;
            for _ in 0..steps {
                *s += (want - *a) * 0.045;
                *s *= 0.78;
                *a += *s;
            }
        }

        let base = Batch { tex: Some(self.up), quads: vec![Quad::full()], add: false, clip: None };
        // pressed / latched keys: their own shape cut from the "down" plate, nothing around them changes
        let mut down = Batch { tex: Some(self.down), quads: vec![], add: false, clip: None };
        for i in 0..self.keys.len() {
            if self.key_down(i) {
                let k = &self.keys[i];
                down.quads.push(Quad::plate_cut(k.rect[0], k.rect[1], k.rect[2], k.rect[3], 1.0).rounded(k.radius));
            }
        }
        // LEDs: the lit lens from the down plate, with its brightness
        for j in 0..18 {
            let b = self.leds[j];
            if b > 0.02 {
                let y = 548.0 + j as f32 * 22.0;
                down.quads.push(Quad::plate_cut(54.0, y - 10.0, 74.0, y + 10.0, 0.25 + 0.75 * b).rounded(10.0));
            }
        }
        // the volume ring around the knob
        let lit = (self.volume * 21.0).round() as i32;
        for i in 0..21 {
            if i < lit {
                let a = (225.0 - i as f32 * 13.5f32).to_radians();
                let (x, y) = (1250.0 + a.cos() * 86.0, 958.0 - a.sin() * 86.0);
                down.quads.push(Quad::plate_cut(x - 9.0, y - 9.0, x + 9.0, y + 9.0, 1.0).rounded(9.0));
            }
        }
        // VU needles: a thin dark blade over each dial, a soft shadow a little off it
        let mut flat = Batch { tex: None, quads: vec![], add: false, clip: None };
        for (k, cx) in [240.0f32, 468.0].into_iter().enumerate() {
            let piv = [cx, 793.0];
            let rot = self.vu[k].0.to_radians();
            flat.quads.push(Quad::flat(cx - 1.2 + 3.0, 723.0 + 4.0, 2.4, 70.0, [0.24, 0.12, 0.0, 0.35]).rotated([piv[0] + 3.0, piv[1] + 4.0], rot));
            flat.quads.push(Quad::flat(cx - 1.2, 723.0, 2.4, 70.0, [0.07, 0.055, 0.045, 1.0]).rotated(piv, rot));
            flat.quads.push(Quad::flat(cx - 5.0, 788.0, 10.0, 10.0, [0.1, 0.08, 0.06, 1.0]).rounded(5.0));
        }
        let mut cv = Canvas { batches: vec![base, down], texts: vec![], clip: None };
        let mut over = Canvas::new();
        let track = self.now.as_ref().and_then(|id| self.tracks.get(id)).cloned();
        let (pos, dur) = if self.pick {
            (self.player.as_ref().map(|p| p.pos().as_secs_f64()).unwrap_or(0.0), track.as_ref().map(|t| t.duration).unwrap_or(0.0))
        } else {
            let pos = if self.slot.0 > 0 { ((net::now_ms() + self.slot.2 - self.slot.0) as f64 / 1000.0).max(0.0) } else { 0.0 };
            (pos, self.slot.1 as f64 / 1000.0)
        };
        self.tv.draw(&mut cv);
        let (title, artist) = track.as_ref().map(|t| (t.title.clone(), t.artist.clone())).unwrap_or(("KAWAII RADIO".into(), "ulanmoqda…".into()));
        if self.panel.screen == panel::Screen::Home {
            self.lyrics.draw(&mut cv, &mut over, &mut self.text, pos, &title, &artist);
        }
        let station = if self.pick { "O'Z TANLOVIM" } else { match self.station.as_str() { "ost" => "OST FM", "riot" => "RIOT FM", _ => "J-POP FM" } };
        let line1 = format!("♫ {title} · {artist}");
        let foot = track.as_ref().map(|t| format!("{station} · {} · {} BPM · ENERGIYA {}%", t.category.to_uppercase(), t.bpm.round(), t.energy.round())).unwrap_or_default();
        scene::draw_lcd(&mut cv, &self.wave, &line1, &foot, pos, dur);
        self.drum += self.drum_kick * 0.02 * 57.3 * (dt * 60.0);
        self.drum_kick *= 0.9f32.powf(dt * 60.0);
        scene::draw_drum(&mut cv, &mut over, self.drum);
        chat::draw_listeners(&mut cv, self.chat.live, self.started.elapsed().as_secs_f32(), self.level);
        {
            let proxy = self.proxy.clone();
            let images = &mut self.images;
            let mut img = |u: &str| -> Option<usize> {
                match images.get(u) {
                    Some(t) => *t,
                    None => {
                        images.insert(u.to_string(), None);
                        let (u2, p) = (u.to_string(), proxy.clone());
                        std::thread::spawn(move || {
                            let im = net::bytes(&u2).ok().and_then(|b| image::load_from_memory(&b).ok()).map(|i| {
                                let i = i.to_rgba8();
                                let (w, h) = i.dimensions();
                                let k = (220.0 / w.max(h) as f32).min(1.0);
                                image::imageops::resize(&i, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1), image::imageops::FilterType::Triangle)
                            });
                            let _ = p.send_event(Msg::Image { url: u2, img: im });
                        });
                        None
                    }
                }
            };
            self.chat.draw(&mut cv, &mut over, &mut self.text, &self.tracks, &mut img);
            // the module over the lyrics display
            let mut panel = std::mem::replace(&mut self.panel, panel::Panel::new());
            {
                let cx = panel::Ctx {
                    tracks: &self.tracks,
                    liked: &self.liked,
                    playlists: &self.playlists,
                    acc: self.acc.as_ref(),
                    now: self.now.as_deref(),
                    version: env!("CARGO_PKG_VERSION"),
                    update: None,
                    search_focus: self.search_focus,
                    blink: (self.started.elapsed().as_secs_f32() / 0.55) as i32 % 2 == 0,
                    minutes: self.store.get("listened").and_then(|v| v.as_u64()).unwrap_or(0) / 60,
                    songs: self.store.get("songs").and_then(|v| v.as_u64()).unwrap_or(0),
                };
                panel.draw(&mut cv, &mut over, &mut self.text, &cx, &mut img);
            }
            self.panel = panel;
        }
        let (line, lines) = self.tape.draw(&mut cv, &mut over);
        over.text(text::Txt::new(format!("QATOR {line:02}/{lines:02}"), text::VT, 18.0, 1773.0, 48.0, [35, 43, 27, 255]).align(text::Align::Center).clip([1703.0, 43.0, 1843.0, 73.0]));
        // the compose strip under the tube (an LCD): what is typed, a blinking block cursor
        let blink = (self.started.elapsed().as_secs_f32() / 0.55) as i32 % 2 == 0;
        let ink = [35u8, 43, 27, 255];
        let (line, faint) = match (&self.acc, &self.compose) {
            (None, _) => ("KAWAII ID BILAN KIRING".to_string(), true),
            (Some(_), Some(c)) if !c.is_empty() => (c.clone(), false),
            (Some(_), Some(_)) => (String::new(), true),
            (Some(_), None) => ("XABAR YOZING".to_string(), true),
        };
        let t = text::Txt::new(line.clone(), text::VT, 19.0, 1508.0, 1016.0, if faint { [35, 43, 27, 140] } else { ink }).clip([1500.0, 1008.0, 1816.0, 1046.0]);
        let tw = if line.is_empty() { 0.0 } else { self.text.measure(&t) };
        // a long message scrolls left so the cursor stays in view
        let shift = (tw - 290.0).max(0.0);
        let mut t = t;
        t.x -= shift;
        cv.text(t);
        if (self.compose.is_some() || self.acc.is_none()) && blink {
            let cx = if self.compose.as_deref().map(|c| c.is_empty()).unwrap_or(true) && self.acc.is_some() { 1508.0 } else { 1508.0 + tw - shift + 3.0 };
            cv.flat(Quad::flat(cx.min(1812.0), 1019.0, 9.0, 16.0, [0.137, 0.169, 0.106, if self.compose.is_some() { 0.85 } else { 0.6 }]));
        }
        // the sticker tray slides up over the bottom of the tube
        if self.tray {
            let list = self.tray_stickers();
            let (tx, ty) = (1452.0, 994.0 - 8.0 - 5.0 * 66.0);
            over.flat(Quad::flat(tx - 8.0, ty - 8.0, 6.0 * 68.0 + 12.0, 5.0 * 66.0 + 12.0, [0.02, 0.05, 0.035, 0.94]).rounded(14.0));
            let mut want = vec![];
            for (i, (_, f)) in list.iter().enumerate() {
                want.push((i, net::url(f)));
            }
            for (i, u) in want {
                let tex = match self.images.get(&u) {
                    Some(t) => *t,
                    None => {
                        self.images.insert(u.clone(), None);
                        let p = self.proxy.clone();
                        let u2 = u.clone();
                        std::thread::spawn(move || {
                            let im = net::bytes(&u2).ok().and_then(|b| image::load_from_memory(&b).ok()).map(|i| image::imageops::resize(&i.to_rgba8(), 128, 128, image::imageops::FilterType::Triangle));
                            let _ = p.send_event(Msg::Image { url: u2, img: im });
                        });
                        None
                    }
                };
                if let Some(t) = tex {
                    let q = Quad::full();
                    over.quad(Some(t), Quad { rect: [tx + (i % 6) as f32 * 68.0 + 2.0, ty + (i / 6) as f32 * 66.0 + 1.0, 62.0, 62.0], uv: [0.0, 0.0, 1.0, 1.0], ..q });
                }
            }
        }
        // a toast: a little LCD tag that appears over the deck for a moment
        if let Some((t, at)) = &self.toast {
            let a = at.elapsed().as_secs_f32();
            if a < 2.6 {
                let tt = text::Txt::new(t.clone(), text::VT, 24.0, 960.0, 646.0, [35, 43, 27, 255]).align(text::Align::Center);
                let tw = self.text.measure(&tt) + 40.0;
                over.flat(Quad::flat(960.0 - tw / 2.0, 640.0, tw, 38.0, [0.62, 0.70, 0.52, 0.96]).rounded(8.0));
                over.text(tt);
            }
        }
        cv.batches.push(flat);
        // the rendered cover glass over everything, except the lyrics tube (it has its own lens)
        let mut glass = Batch { tex: Some(self.glass), quads: vec![], add: false, clip: None };
        let [gx0, gy0, gx1, gy1] = [618.0, 198.0, 1362.0, 626.0];
        glass.quads.push(Quad::plate_cut(0.0, 0.0, 1920.0, gy0, 1.0));
        glass.quads.push(Quad::plate_cut(0.0, gy1, 1920.0, 1080.0, 1.0));
        glass.quads.push(Quad::plate_cut(0.0, gy0, gx0, gy1, 1.0));
        glass.quads.push(Quad::plate_cut(gx1, gy0, 1920.0, gy1, 1.0));
        over.batches.push(glass);
        self.over = over.batches;
        self.texts = cv.texts.into_iter().chain(over.texts).collect();
        cv.batches
    }
}

impl ApplicationHandler<Msg> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.st.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Kawaii Radio")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(480.0, 270.0))
            .with_decorations(false);
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let mut gfx = pollster::block_on(Gfx::new(window.clone()));
        let load = |g: &mut Gfx, b: &[u8]| g.texture_image(b).expect("plate");
        let up = load(&mut gfx, PLATE_UP);
        let down = load(&mut gfx, PLATE_DOWN);
        let glass = load(&mut gfx, GLASS);
        let tl = text::TextLayer::new(&gfx.device, &gfx.queue, gfx.config.format);
        // the chat: one socket, a hello with this computer's identity
        let mut store = store::Store::open();
        let (uid, key) = store.chat_id();
        window.set_ime_allowed(true);
        let mut chat = chat::Chat::new();
        let (tx, rx) = std::sync::mpsc::channel();
        chat.out = Some(tx);
        let hello = serde_json::json!({ "type": "hello", "uid": uid, "key": key, "name": "Mehmon", "avatar": "p:0", "live": true, "fg": true,
            "card": { "now": null, "likes": 0, "plays": 0, "minutes": 0 } });
        let p3 = self.proxy.clone();
        chat::connect(hello, rx, move |v| {
            let _ = p3.send_event(Msg::Chat(v));
        });
        let p4 = self.proxy.clone();
        std::thread::spawn(move || {
            let r = (|| -> Option<HashMap<String, String>> {
                let v: serde_json::Value = serde_json::from_slice(&net::bytes("/files/stickers/pack.json").ok()?).ok()?;
                Some(v.get("stickers")?.as_array()?.iter().filter_map(|s| Some((s.get("id")?.as_str()?.to_string(), s.get("file")?.as_str()?.to_string()))).collect())
            })();
            if let Some(m) = r {
                let _ = p4.send_event(Msg::Stickers(m));
            }
        });
        let player = audio::Player::new().map_err(|e| eprintln!("audio: {e}")).ok();
        let volume = 0.8;
        if let Some(p) = &player {
            p.set_volume(volume * volume);
        }
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            if let Ok(t) = net::tracks() {
                let _ = proxy.send_event(Msg::Tracks(t));
            }
        });
        tune(self.proxy.clone(), "jpop".into());
        self.st = Some(State {
            gfx,
            up,
            down,
            glass,
            player,
            keys: keys(),
            pressed: None,
            cursor: (0.0, 0.0),
            station: "jpop".into(),
            playing: true,
            volume,
            tracks: HashMap::new(),
            now: None,
            level: 0.0,
            leds: [0.0; 18],
            vu: [(-54.0, 0.0); 2],
            last: Instant::now(),
            tuning: true,
            next_check: Instant::now() + Duration::from_secs(5),
            text: tl,
            lyrics: scene::Lyrics::new(),
            tv: scene::Trivision::new(),
            wave: scene::wave_of("kawaii"),
            slot: (0, 0, 0),
            drum: 0.0,
            drum_kick: 0.0,
            over: vec![],
            texts: vec![],
            covers: HashMap::new(),
            store,
            chat,
            images: HashMap::new(),
            started: Instant::now(),
            proxy: self.proxy.clone(),
            token: None,
            acc: None,
            login_state: None,
            compose: None,
            tray: false,
            toast: None,
            uid: (uid.clone(), key.clone()),
            panel: panel::Panel::new(),
            tape: receipt::Tape::new(),
            pick: false,
            pick_list: vec![],
            search_focus: false,
            liked: vec![],
            playlists: vec![],
            syncing: false,
            last_sync: Instant::now() - Duration::from_secs(290),
        });
        // sign-in state from last time; links from later copies; the kawaii:// scheme
        if let Some(st) = self.st.as_mut() {
            st.token = st.store.get("kidToken").and_then(|v| v.as_str()).map(String::from);
            st.acc = st.store.get("kidAcc").cloned().filter(|v| !v.is_null());
            st.liked = st.store.get("liked").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
            st.playlists = st.store.get("playlists").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
            if st.token.is_some() {
                st.refresh_account();
            }
        }
        let p5 = self.proxy.clone();
        kid::listen(move |s| {
            let _ = p5.send_event(Msg::Link(s));
        });
        std::thread::spawn(kid::register_scheme);
        if let Some(l) = std::env::args().find(|a| a.starts_with("kawaii://")) {
            let _ = self.proxy.send_event(Msg::Link(l));
        }
        window.request_redraw();
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, msg: Msg) {
        let Some(st) = self.st.as_mut() else { return };
        match msg {
            Msg::Tracks(t) => st.tracks = t,
            Msg::Chat(v) => {
                if v.get("type").and_then(|t| t.as_str()) == Some("welcome") && st.acc.is_some() {
                    st.chat_profile();
                }
                if v.get("type").and_then(|t| t.as_str()) == Some("error") {
                    let t = v.get("text").and_then(|t| t.as_str()).unwrap_or("XATO").to_uppercase();
                    st.toast(&t);
                }
                st.chat.on_server(&v)
            }
            Msg::Toast(t) => st.toast(&t),
            Msg::Synced(m) => {
                st.syncing = false;
                let likes = m.get("likes").and_then(|v| v.as_object()).cloned().unwrap_or_default();
                let mut l: Vec<(String, i64)> = likes.iter().map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(0))).collect();
                l.sort_by_key(|x| x.1);
                st.liked = l.into_iter().map(|x| x.0).collect();
                st.store.set("liked", serde_json::to_value(&st.liked).unwrap());
                st.store.set("likesAt", likes.into());
                st.store.set("unlikedAt", m.get("unliked").cloned().unwrap_or_default());
                st.playlists = m.get("playlists").and_then(|v| v.as_array()).map(|a| {
                    a.iter().filter_map(|p| {
                        Some((p.get("id")?.as_str()?.to_string(), p.get("name")?.as_str()?.to_string(),
                            p.get("tracks")?.as_array()?.iter().filter_map(|x| x.as_str().map(String::from)).collect()))
                    }).collect()
                }).unwrap_or_default();
                st.store.set("playlists", serde_json::to_value(&st.playlists).unwrap());
            }
            Msg::Pick { track, mp3 } => {
                st.tuning = false;
                if !st.pick {
                    return;
                }
                if let Some(p) = &st.player {
                    let _ = p.play(mp3, Duration::ZERO);
                    p.set_paused(false);
                }
                st.playing = true;
                if let Some(t) = st.tracks.get(&track).cloned() {
                    st.gfx.window.set_title(&format!("{} · {} — Kawaii Radio", t.title, t.artist));
                    let ly = t.lyrics.clone().unwrap_or_default();
                    let rom = net::parse_lrc(ly.romaji.as_deref());
                    let uz = net::parse_lrc(ly.uzbek.as_deref());
                    let jp = net::parse_lrc(ly.kanji.as_deref());
                    let main = if !rom.is_empty() { rom } else if !uz.is_empty() { uz.clone() } else { jp };
                    st.lyrics.set(main, uz);
                    st.tape.print(&t, &mut st.text, true);
                }
                if let Some(&tex) = st.covers.get(&track) {
                    st.tv.show(tex);
                } else if let Some(t) = st.tracks.get(&track) {
                    let (cover, id, p2) = (t.cover_art.clone(), track.clone(), st.proxy.clone());
                    std::thread::spawn(move || {
                        if let Ok(b) = net::bytes(&cover) {
                            if let Ok(img) = image::load_from_memory(&b) {
                                let img = image::imageops::resize(&img.to_rgba8(), 436, 432, image::imageops::FilterType::Triangle);
                                let _ = p2.send_event(Msg::Cover { track: id, img });
                            }
                        }
                    });
                }
                st.wave = scene::wave_of(&track);
                st.drum_kick += 40.0;
                st.now = Some(track);
            }
            Msg::Link(l) => {
                st.gfx.window.focus_window();
                if let Some((t, s)) = kid::parse_link(&l) {
                    if st.login_state.as_deref() == Some(s.as_str()) && t.starts_with("krs_") {
                        st.login_state = None;
                        st.token = Some(t.clone());
                        st.store.set("kidToken", serde_json::Value::String(t));
                        st.refresh_account();
                    }
                }
            }
            Msg::Account(acc) => {
                match acc {
                    Some(a) => {
                        let first = st.acc.is_none();
                        st.store.set("kidAcc", a.clone());
                        st.acc = Some(a.clone());
                        if first {
                            let n = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_uppercase();
                            st.toast(&format!("SALOM, {n}"));
                        }
                        st.chat_profile();
                        st.sync();
                    }
                    None => {
                        st.token = None;
                        st.acc = None;
                        st.store.set("kidToken", serde_json::Value::Null);
                        st.store.set("kidAcc", serde_json::Value::Null);
                        st.toast("KAWAII ID · QAYTA KIRING");
                    }
                }
            }
            Msg::Stickers(m) => st.chat.stickers = m,
            Msg::Image { url, img } => {
                let tex = img.map(|i| {
                    let (w, h) = i.dimensions();
                    st.gfx.texture_rgba(w, h, &i)
                });
                st.images.insert(url, tex);
            }
            Msg::Cover { track, img } => {
                let (w, h) = img.dimensions();
                let tex = st.gfx.texture_rgba(w, h, &img);
                st.covers.insert(track.clone(), tex);
                if st.now.as_deref() == Some(track.as_str()) {
                    st.tv.show(tex);
                }
            }
            Msg::Tune { station, track, mp3, offset, start, dur, skew } => {
                st.tuning = false;
                if station != st.station {
                    return;
                }
                if let Some(p) = &st.player {
                    if let Err(e) = p.play(mp3, offset) {
                        eprintln!("play: {e}");
                    }
                    p.set_paused(!st.playing);
                }
                if let Some(t) = st.tracks.get(&track) {
                    st.gfx.window.set_title(&format!("{} · {} — Kawaii Radio", t.title, t.artist));
                }
                st.slot = (start, dur, skew);
                if let Some(t) = st.tracks.get(&track) {
                    let ly = t.lyrics.clone().unwrap_or_default();
                    let rom = net::parse_lrc(ly.romaji.as_deref());
                    let uz = net::parse_lrc(ly.uzbek.as_deref());
                    let jp = net::parse_lrc(ly.kanji.as_deref());
                    let main = if !rom.is_empty() { rom } else if !uz.is_empty() { uz.clone() } else { jp };
                    st.lyrics.set(main, uz);
                }
                if let Some(&tex) = st.covers.get(&track) {
                    st.tv.show(tex);
                }
                st.wave = scene::wave_of(&track);
                if let Some(t) = st.tracks.get(&track).cloned() {
                    let first = st.now.is_none();
                    st.tape.print(&t, &mut st.text, !first);
                }
                if st.now.is_some() {
                    st.drum_kick += 40.0;
                }
                st.now = Some(track);
            }
            Msg::Error(e) => {
                st.tuning = false;
                eprintln!("tune: {e}");
                st.next_check = Instant::now() + Duration::from_secs(5);
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        let Some(st) = self.st.as_mut() else { return };
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => st.gfx.resize(s.width, s.height),
            WindowEvent::CursorMoved { position, .. } => st.cursor = st.gfx.to_stage(position.x, position.y),
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let (x, y) = st.cursor;
                match state {
                    ElementState::Pressed if st.tray && st.tray_pick(x, y).is_some() => {
                        if let Some(id) = st.tray_pick(x, y) {
                            st.send_chat("sticker", &id);
                            st.tray = false;
                        }
                    }
                    ElementState::Pressed if st.panel.hit(x, y).is_some() => {
                        let a = st.panel.hit(x, y).unwrap();
                        if !matches!(a, panel::Act::FocusSearch) {
                            st.search_focus = false;
                        }
                        st.act(a);
                    }
                    ElementState::Pressed if st.pick && (595.0..1387.0).contains(&x) && (700.0..800.0).contains(&y) => {
                        let f = ((x - 622.0) / 738.0).clamp(0.0, 1.0) as f64;
                        let dur = st.now.as_ref().and_then(|n| st.tracks.get(n)).map(|t| t.duration).unwrap_or(0.0);
                        if let Some(p) = &st.player {
                            p.seek(Duration::from_secs_f64(f * dur));
                        }
                    }
                    ElementState::Pressed => match st.key_at(x, y) {
                        Some(i) => {
                            if st.keys[i].id != "compose" && st.keys[i].id != "cid" {
                                st.compose = None;
                            }
                            st.pressed = Some(i)
                        }
                        // any surface that is not a control moves the window (the OS's own move loop: Snap works)
                        None => {
                            st.compose = None;
                            st.tray = false;
                            let _ = st.gfx.window.drag_window();
                        }
                    },
                    ElementState::Released => {
                        if let Some(i) = st.pressed.take() {
                            if st.key_at(x, y) == Some(i) {
                                let id = st.keys[i].id;
                                st.on_click(id, &self.proxy, el);
                            }
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && st.search_focus && st.panel.screen == panel::Screen::Search => {
                match &event.logical_key {
                    WKey::Named(NamedKey::Escape) => st.search_focus = false,
                    WKey::Named(NamedKey::Backspace) => {
                        st.panel.query.pop();
                    }
                    WKey::Named(NamedKey::Enter) => st.search_focus = false,
                    _ => {
                        if let Some(t) = &event.text {
                            st.panel.query.extend(t.chars().filter(|c| !c.is_control()));
                            st.panel.scroll = 0.0;
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && event.logical_key == WKey::Named(NamedKey::Escape) && st.compose.is_none() => {
                st.panel.screen = panel::Screen::Home;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && event.logical_key == WKey::Named(NamedKey::F11) => {
                st.act(panel::Act::Full);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && st.compose.is_some() => {
                match &event.logical_key {
                    WKey::Named(NamedKey::Enter) => {
                        let msg = st.compose.as_ref().map(|s| s.trim().to_string()).unwrap_or_default();
                        if !msg.is_empty() {
                            st.send_chat("text", &msg);
                            st.compose = Some(String::new());
                        }
                    }
                    WKey::Named(NamedKey::Escape) => st.compose = None,
                    WKey::Named(NamedKey::Backspace) => {
                        if let Some(c) = st.compose.as_mut() {
                            c.pop();
                        }
                    }
                    _ => {
                        if let Some(t) = &event.text {
                            if let Some(c) = st.compose.as_mut() {
                                if c.chars().count() < 300 {
                                    c.extend(t.chars().filter(|ch| !ch.is_control()));
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(t)) => {
                if let Some(c) = st.compose.as_mut() {
                    c.push_str(&t);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = st.cursor;
                let [cx0, cy0, cx1, cy1] = chat::TUBE;
                let wd = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 60.0,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                let [lx0, ly0, lx1, ly1] = scene::LY;
                let [px0, py0, px1, py1] = receipt::WIN;
                if st.panel.screen != panel::Screen::Home && x >= lx0 && x <= lx1 && y >= ly0 && y <= ly1 {
                    st.panel.scroll -= wd;
                } else if x >= px0 && x <= px1 && y >= py0 && y <= py1 {
                    st.tape.scroll -= wd * 0.6;
                } else if x >= cx0 && x <= cx1 && y >= cy0 && y <= cy1 {
                    let d = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y * 60.0,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    st.chat.scroll += d;
                } else if ((x - 1250.0).powi(2) + (y - 958.0).powi(2)).sqrt() < 110.0 {
                    let d = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y * 0.05,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.002,
                    };
                    st.volume = (st.volume + d).clamp(0.0, 1.0);
                    if let Some(p) = &st.player {
                        p.set_volume(st.volume * st.volume);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // the next track: when the current one ran out (or nothing plays yet), ask the schedule again
                if st.playing && !st.tuning && Instant::now() >= st.next_check {
                    st.next_check = Instant::now() + Duration::from_secs(2);
                    if st.player.as_ref().is_some_and(|p| p.idle()) {
                        if st.pick {
                            st.pick_step(1);
                        } else {
                            st.tuning = true;
                            tune(self.proxy.clone(), st.station.clone());
                        }
                    }
                    if st.token.is_some() && st.last_sync.elapsed() > Duration::from_secs(300) {
                        st.sync();
                    }
                }
                let batches = st.frame();
                let over = std::mem::take(&mut st.over);
                let texts = std::mem::take(&mut st.texts);
                st.gfx.render(&batches, Some((&mut st.text, &texts)), &over);
                st.text.trim();
                st.gfx.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let link = std::env::args().find(|a| a.starts_with("kawaii://"));
    if kid::hand_over(link.as_deref()) {
        return;
    }
    let event_loop = EventLoop::<Msg>::with_user_event().build().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { proxy: event_loop.create_proxy(), st: None };
    event_loop.run_app(&mut app).expect("run");
}
