// KR-01 DESK, native: no browser inside. The device is a set of rendered plates (Blender), drawn on the GPU with
// wgpu; everything live (LEDs, VU needles, pressed keys, …) is quads on top. Audio is decoded and played by the
// app itself (symphonia + cpal), so it needs no system codecs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod chat;
mod store;
mod spectrum;
mod stream;
mod gfx;
mod kid;
mod net;
mod panel;
mod receipt;
mod scene;
mod text;
#[cfg(windows)]
mod win;

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
/// the logo: the bunny alone (white on the dark device), and the chassis that was under the green plate
const LOGO: &[u8] = include_bytes!("../assets/logo-bunny.png");
const LOGO_BG: &[u8] = include_bytes!("../assets/logo-patch.png");
const LOGO_BG_LIGHT: &[u8] = include_bytes!("../assets/logo-patch-light.png");
const LIGHT_UP: &[u8] = include_bytes!("../../app/ui/assets/light-up.webp");
const LIGHT_DOWN: &[u8] = include_bytes!("../../app/ui/assets/light-down.webp");
const LIGHT_GLASS: &[u8] = include_bytes!("../../app/ui/assets/glass-light.webp");

/// Surface textures the app makes itself (white, the pattern in alpha; tinted by the quad's colour):
/// `grain` = fine noise, `brushed` = metal brushed along x, `paper` = thermal paper fibres and clouds.
fn gen_textures(g: &mut Gfx) -> (usize, usize, usize) {
    let mut seed: u32 = 0x2545_f491;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    let mk = |w: u32, h: u32, a: &dyn Fn(u32, u32) -> f32| -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                v.extend([255, 255, 255, (a(x, y).clamp(0.0, 1.0) * 255.0) as u8]);
            }
        }
        v
    };
    let n1: Vec<f32> = (0..512 * 256).map(|_| rnd()).collect();
    let grain = mk(512, 256, &|x, y| n1[(y * 512 + x) as usize]);
    let rows: Vec<f32> = (0..128).map(|_| rnd()).collect();
    let streak: Vec<f32> = (0..512 * 128).map(|_| rnd()).collect();
    let brushed = mk(512, 128, &|x, y| {
        // a row's own brightness, long streaks along it, a little grain
        let s = (0..24).map(|k| streak[(y as usize * 512 + ((x as usize + k * 7) % 512)) % streak.len()]).sum::<f32>() / 24.0;
        0.35 * rows[y as usize] + 0.45 * s + 0.2 * n1[(y * 512 + x) as usize]
    });
    let blot: Vec<f32> = (0..32 * 32).map(|_| rnd()).collect();
    let paper = mk(512, 512, &|x, y| {
        // soft clouds (bilinear 32x32 noise) and short fibres along y
        let (fx, fy) = (x as f32 / 16.0, y as f32 / 16.0);
        let (ix, iy) = (fx as usize % 32, fy as usize % 32);
        let (tx, ty) = (fx.fract(), fy.fract());
        let b = |i: usize, j: usize| blot[(j % 32) * 32 + (i % 32)];
        let c = b(ix, iy) * (1.0 - tx) * (1.0 - ty) + b(ix + 1, iy) * tx * (1.0 - ty) + b(ix, iy + 1) * (1.0 - tx) * ty + b(ix + 1, iy + 1) * tx * ty;
        let f = n1[((y / 3) * 512 + x) as usize % n1.len()];
        0.55 * c + 0.3 * f + 0.15 * n1[((y * 512 + x) as usize) % n1.len()]
    });
    (g.texture_tiled(512, 256, &grain), g.texture_tiled(512, 128, &brushed), g.texture_tiled(512, 512, &paper))
}

/// What the network threads hand to the window thread.
enum Msg {
    Tracks(HashMap<String, net::Track>),
    /// the song on air, streaming from its place now, for this station / track
    Tune { station: String, track: String, src: audio::Prepared, start: i64, dur: i64, skew: i64 },
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
    Pick { track: String, src: audio::Prepared },
    /// a theme's plates, decoded (up, down, glass, logo patch)
    Plates { light: bool, imgs: Vec<image::RgbaImage> },
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
    // the band switch: two halves, one of them always in
    k.push(Key { id: "sl_radio", rect: [843.0, 41.0, 1101.0, 95.0], radius: 15.0 });
    k.push(Key { id: "sl_pick", rect: [1103.0, 41.0, 1361.0, 95.0], radius: 15.0 });
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
    /// the plates of each theme (dark, light), once loaded
    plates: [Option<(usize, usize, usize, usize)>; 2],
    light: bool,
    /// grain, brushed metal, paper
    tex: (usize, usize, usize),
    /// the bunny and the chassis patch under it
    logo: (usize, usize),
    /// the band switch's sliding light: x and speed
    sw: (f32, f32),
    /// a page change running: since when, from which page
    screen_anim: Option<(Instant, panel::Screen)>,
    /// when each key was let go (for its spring back)
    released: Vec<Option<Instant>>,
    player: Option<audio::Player>,
    keys: Vec<Key>,
    pressed: Option<usize>,
    cursor: (f32, f32),
    station: String,
    playing: bool,
    volume: f32,
    tracks: Arc<HashMap<String, net::Track>>,
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
    /// the LCD waveform is held: dragging shows where, letting go seeks there (O'Z TANLOVIM)
    scrub: Option<f32>,
    spec: spectrum::Analyser,
    /// the song asked for last (an older one arriving later is dropped)
    want_pick: Option<String>,
}

struct App {
    proxy: EventLoopProxy<Msg>,
    st: Option<State>,
    next_frame: Instant,
}

/// An empty batch that nothing merges into: what is drawn after it can be told apart (batch index, text index).
fn marker(c: &mut Canvas) -> (usize, usize) {
    c.batches.push(Batch { tex: None, quads: vec![], add: false, clip: Some([-9.0, -9.0, -9.0, -9.0]) });
    (c.batches.len(), c.texts.len())
}

fn isect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

/// What was drawn between two markers shows only inside `r`.
fn restrict(c: &mut Canvas, a: (usize, usize), b: (usize, usize), r: [f32; 4]) {
    for bt in &mut c.batches[a.0..b.0 - 1] {
        bt.clip = Some(isect(bt.clip.unwrap_or([0.0, 0.0, 1920.0, 1080.0]), r));
    }
    for t in &mut c.texts[a.1..b.1] {
        t.clip = isect(t.clip, r);
    }
}

/// What was drawn between two markers fades to `k`.
fn fade(c: &mut Canvas, a: (usize, usize), b: (usize, usize), k: f32) {
    for bt in &mut c.batches[a.0..b.0 - 1] {
        for q in &mut bt.quads {
            q.color[3] *= k;
            // gradients and rims keep their second colour in uv
            if matches!(q.kind as i32, 3 | 6 | 8 | 10) {
                q.uv[3] *= k;
            }
        }
    }
    for t in &mut c.texts[a.1..b.1] {
        for sp in &mut t.spans {
            sp.color[3] = (sp.color[3] as f32 * k) as u8;
        }
        if let Some(g) = &mut t.glow {
            g.0[3] = (g.0[3] as f32 * k) as u8;
        }
    }
}

/// KR_DEBUG: a line in %TEMP%/kr_debug.log
fn dlog(s: &str) {
    if std::env::var_os("KR_DEBUG").is_some() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("kr_debug.log")) {
            let _ = writeln!(f, "{s}");
        }
    }
}

fn tune(proxy: EventLoopProxy<Msg>, station: String, known: Arc<HashMap<String, net::Track>>) {
    std::thread::spawn(move || {
        let res = (|| -> Result<Msg, String> {
            let t0 = Instant::now();
            let sch = net::schedule(&station)?;
            let t_sch = Instant::now();
            dlog(&format!("schedule {:?}", t0.elapsed()));
            let (slot, into) = sch.on_air().ok_or("efir bo'sh")?;
            // the track list the app already has (it is big: fetched again only if this song is not in it)
            let fetched;
            let t = match known.get(&slot.id) {
                Some(t) => t,
                None => {
                    fetched = net::tracks()?;
                    fetched.get(&slot.id).ok_or("trek topilmadi")?
                }
            };
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
            // streaming from where the station is now (counted when the stream is open, plus a little for the start)
            let src = audio::prepare(&net::url(&t.audio_src), || Duration::from_millis(into.max(0) as u64) + t_sch.elapsed() + Duration::from_millis(250))?;
            Ok(Msg::Tune { station, track: slot.id.clone(), src, start: slot.start, dur: slot.duration, skew: sch.skew })
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
            let mut m = serde_json::json!({ "type": "send", "kind": kind, "body": body, "nonce": nonce });
            if let Some(r) = self.chat.reply.take() {
                m["reply"] = r.id.into();
            }
            let _ = out.send(m.to_string());
        }
        self.chat.goal = 0.0;
    }

    fn has_kp(&self) -> bool {
        if self.chat.you.as_ref().is_some_and(|y| ["admin", "owner"].iter().any(|k| y.get(*k).and_then(|v| v.as_bool()).unwrap_or(false))) {
            return true;
        }
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
        dlog(&format!("play_pick {id}"));
        let Some(t) = self.tracks.get(id).cloned() else { return };
        self.pick = true;
        self.playing = true;
        self.tuning = true;
        let p = self.proxy.clone();
        let id = id.to_string();
        self.want_pick = Some(id.clone());
        self.toast(&format!("{} · O'Z TANLOVIM", t.title.to_uppercase()));
        std::thread::spawn(move || match audio::prepare(&net::url(&t.audio_src), || Duration::ZERO) {
            Ok(src) => {
                let _ = p.send_event(Msg::Pick { track: id, src });
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
            Act::Close => self.go(panel::Screen::Home),
            Act::Back => {
                self.panel.open = None;
                self.panel.scroll = 0.0;
                self.panel.goal = 0.0;
            }
            Act::OpenList(id) => {
                self.panel.open = Some(id);
                self.panel.scroll = 0.0;
                self.panel.goal = 0.0;
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
            Act::Theme(l) => self.set_theme(l),
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
            light: self.light,
        }
    }

    /// O'Z TANLOVIM: the waveform (x 622..1360) is the timeline.
    fn seek_at(&mut self, x: f32) {
        let f = ((x - 622.0) / 738.0).clamp(0.0, 1.0) as f64;
        let dur = self.now.as_ref().and_then(|n| self.tracks.get(n)).map(|t| t.duration).unwrap_or(0.0);
        if let (Some(p), true) = (&self.player, dur > 0.0) {
            p.seek(Duration::from_secs_f64(f * dur));
        }
    }

    /// Change the page on the display: the old one dims away, the new one is written in by a sweep (see frame()).
    fn go(&mut self, s: panel::Screen) {
        if s == self.panel.screen {
            return;
        }
        self.screen_anim = Some((Instant::now(), self.panel.screen));
        self.panel.screen = s;
    }

    /// KUN (light chassis) / TUN (graphite): the plates are swapped (decoded in the background the first time).
    fn set_theme(&mut self, light: bool) {
        self.store.set("theme", (if light { "light" } else { "dark" }).into());
        match self.plates[light as usize] {
            Some((u, d, g, _)) => {
                self.up = u;
                self.down = d;
                self.glass = g;
                self.light = light;
            }
            None => {
                let p = self.proxy.clone();
                std::thread::spawn(move || {
                    let imgs = [LIGHT_UP, LIGHT_DOWN, LIGHT_GLASS, LOGO_BG_LIGHT].iter().filter_map(|b| image::load_from_memory(b).ok().map(|i| i.to_rgba8())).collect::<Vec<_>>();
                    if imgs.len() == 4 {
                        let _ = p.send_event(Msg::Plates { light, imgs });
                    }
                });
            }
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
                        tune(proxy.clone(), self.station.clone(), self.tracks.clone());
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
                self.go(if self.panel.screen == s { panel::Screen::Home } else { s });
                self.panel.scroll = 0.0;
                self.panel.goal = 0.0;
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
                    // back: the start of this song first (after 3 s), then the song before
                    if id == "prev" && self.player.as_ref().is_some_and(|p| p.pos().as_secs_f32() > 3.0) {
                        if let Some(p) = &self.player {
                            p.seek(Duration::ZERO);
                        }
                        self.toast("BOSHIDAN BOSHLANDI");
                    } else {
                        self.pick_step(if id == "next" { 1 } else { -1 });
                    }
                } else {
                    self.toast(if id == "next" { "JONLI EFIR · OLDINGA YO'Q" } else { "JONLI EFIR · ORQAGA YO'Q" });
                }
            }
            "sl_radio" => {
                if self.pick {
                    self.pick = false;
                    self.tuning = true;
                    tune(proxy.clone(), self.station.clone(), self.tracks.clone());
                    self.toast("RADIO");
                }
            }
            "sl_pick" => {
                if self.pick {
                    self.go(panel::Screen::Search);
                    self.search_focus = true;
                } else if self.has_kp() {
                    // the band is O'Z TANLOVIM from now on (the radio plays on until a song is chosen)
                    self.pick = true;
                    self.go(panel::Screen::Search);
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
                    tune(proxy.clone(), st, self.tracks.clone());
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
            "sl_radio" | "sl_pick" => false,
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

        let edge = |r: [f32; 4], uv: [f32; 4]| Quad { rect: r, uv, ..Quad::full() };
        let px = 1.0 / 3840.0;
        let py = 1.0 / 2160.0;
        let mut base = Batch {
            tex: Some(self.up),
            quads: vec![
                edge([-1500.0, 0.0, 1500.0, 1080.0], [px, 0.0, 0.0, 1.0]),
                edge([1920.0, 0.0, 1500.0, 1080.0], [1.0 - px * 2.0, 0.0, 0.0, 1.0]),
                edge([-1500.0, -1500.0, 4920.0, 1500.0], [0.5, py, 0.0, 0.0]),
                edge([-1500.0, 1080.0, 4920.0, 1500.0], [0.5, 1.0 - py * 2.0, 0.0, 0.0]),
                Quad::full(),
            ],
            add: false,
            clip: None,
        };
        // the TAVSIF key has no lamp: its corner is covered with the key's own surface from beside it
        const LAMP: ([f32; 4], [f32; 4]) = ([452.0, 992.0, 467.0, 1008.0], [465.0, 992.0, 480.0, 1008.0]);
        let lamp_patch = |src: [f32; 4], dst: [f32; 4]| Quad { rect: [dst[0], dst[1], dst[2] - dst[0], dst[3] - dst[1]], ..Quad::plate_cut(src[0], src[1], src[2], src[3], 1.0) }.rounded(7.0);
        base.quads.push(lamp_patch(LAMP.0, LAMP.1));
        // physical keys, each cut by its own shape: held = sunk 1.4 px and a hair smaller, let go = a short spring
        // with an overshoot (the web version's keyframes); pressed / latched ones from the "down" plate
        let mut upk = Batch { tex: Some(self.up), quads: vec![], add: false, clip: None };
        let mut down = Batch { tex: Some(self.down), quads: vec![], add: false, clip: None };
        for i in 0..self.keys.len() {
            let k = &self.keys[i];
            if k.id.starts_with("sl_") {
                continue;
            }
            let (off, sc) = if self.pressed == Some(i) && k.id != "play" {
                (1.4, 0.992)
            } else if let Some(t) = self.released.get(i).copied().flatten().map(|t| t.elapsed().as_secs_f32() / 0.28).filter(|t| *t < 1.0) {
                let e = scene::bezier(0.2, 0.8, 0.3, 1.0, t) * 3.0;
                const KF: [(f32, f32); 4] = [(1.4, 0.992), (-0.7, 1.003), (0.25, 1.0), (0.0, 1.0)];
                let (a, b) = (KF[(e as usize).min(2)], KF[(e as usize + 1).min(3)]);
                let f = e.fract();
                (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
            } else {
                (0.0, 1.0)
            };
            let lit = self.key_down(i);
            if !lit && off == 0.0 {
                continue;
            }
            let r = k.rect;
            let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
            let map = |x: f32, y: f32| (cx + (x - cx) * sc, cy + (y - cy) * sc + off);
            let (a, b) = (map(r[0], r[1]), map(r[2], r[3]));
            let q = Quad { rect: [a.0, a.1, b.0 - a.0, b.1 - a.1], ..Quad::plate_cut(r[0], r[1], r[2], r[3], 1.0) }.rounded(k.radius * sc);
            let batch = if lit { &mut down } else { &mut upk };
            batch.quads.push(q);
            if k.id == "tavsif" {
                let (d0, d1) = (map(LAMP.1[0], LAMP.1[1]), map(LAMP.1[2], LAMP.1[3]));
                batch.quads.push(lamp_patch(LAMP.0, [d0.0, d0.1, d1.0, d1.1]));
            }
        }
        // the logo: the chassis over the old green plate, the bunny in white
        let logo = vec![
            Batch { tex: Some(self.plates[self.light as usize].map(|p| p.3).unwrap_or(self.logo.1)), quads: vec![Quad { rect: [148.0, 36.0, 51.5, 52.0], uv: [0.0, 0.0, 1.0, 1.0], ..Quad::full() }], add: false, clip: None },
            Batch { tex: Some(self.logo.0), quads: vec![Quad { rect: [152.0, 40.0, 43.5, 44.0], uv: [0.0, 0.0, 1.0, 1.0], color: [0.96, 0.97, 0.96, 1.0], ..Quad::full() }], add: false, clip: None },
        ];
        let mut halos: Vec<Quad> = vec![];
        let mut switch: Vec<Quad> = vec![];
        let mut lens_glints: Vec<Quad> = vec![];
        let mut cv_texts: Vec<text::Txt> = vec![];
        // LEDs: the lit lens from the down plate, with its brightness
        for j in 0..18 {
            let b = self.leds[j];
            if b > 0.02 {
                let y = 548.0 + j as f32 * 22.0;
                down.quads.push(Quad::plate_cut(54.0, y - 10.0, 74.0, y + 10.0, 0.25 + 0.75 * b).rounded(10.0));
                let col = if j < 2 { [1.0, 0.27, 0.16, 0.30 * b] } else if j < 5 { [1.0, 0.67, 0.25, 0.26 * b] } else { [0.27, 0.9, 0.47, 0.24 * b] };
                let c = [col[0], col[1], col[2], col[3] * 0.55];
                halos.push(Quad::glow(64.0, y, 15.0, c));
                halos.push(Quad::radial(64.0 - 6.5, y - 6.5, 13.0, 13.0, [1.0, 0.95, 0.85, 0.42 * b], [col[0], col[1], col[2], 0.0]));
                lens_glints.push(Quad::glow(61.5, y - 3.0, 2.6, [1.0, 1.0, 1.0, 0.5 + 0.3 * b]));
            }
        }
        // the band switch: a recessed panel over the two plate keys, RADIO printed on the left, O'Z TANLOVIM on
        // the right, between them a slot with a metal slider knob that snaps across (a little spring)
        let want = if self.pick { 1.0 } else { 0.0 };
        let (sx, sv) = &mut self.sw;
        for _ in 0..(dt * 60.0).round().max(1.0) as i32 {
            *sv += (want - *sx) * 0.11;
            *sv *= 0.68;
            *sx += *sv;
        }
        // the knob stops hard at the slot's ends (the spring only shows as a little bounce back)
        let f = sx.clamp(0.0, 1.0);
        let [px0, py0, px1, py1] = [843.0f32, 41.0, 1361.0, 95.0];
        let (pw, ph) = (px1 - px0, py1 - py0);
        switch.push(Quad::flat(px0 - 2.0, py0 - 2.0, pw + 4.0, ph + 4.0, [0.05, 0.055, 0.06, 1.0]).rounded(18.0));
        switch.push(Quad::grad(px0, py0, pw, ph, scene::rgb(0x15171a), scene::rgb(0x1d1f23), true).rounded(16.0));
        let grain = |r: [f32; 4], c: [f32; 4], rad: f32| Batch { tex: Some(self.tex.0), quads: vec![Quad { rect: r, uv: [0.0, 0.0, r[2] / 512.0, r[3] / 256.0], color: c, ..Quad::full() }.rounded(rad)], add: false, clip: None };
        let mut textured: Vec<Batch> = vec![grain([px0, py0, pw, ph], [0.0, 0.0, 0.0, 0.22], 16.0), grain([px0, py0, pw, ph], [1.0, 1.0, 1.0, 0.035], 16.0)];
        switch.push(Quad::inset(px0, py0, pw, ph, 16.0, 12.0, [0.0, 0.0, 0.0, 0.55]));
        // two countersunk screws at the ends
        for sxp in [px0 + 20.0, px1 - 20.0] {
            switch.push(Quad::flat(sxp - 7.0, 61.0, 14.0, 14.0, [0.0, 0.0, 0.0, 0.55]).rounded(7.0));
            switch.push(Quad::radial(sxp - 6.0, 62.0, 12.0, 12.0, scene::rgb(0x7d838c), scene::rgb(0x2e3136)).rounded(6.0));
            switch.push(Quad::rim(sxp - 6.0, 62.0, 12.0, 12.0, 6.0, 1.0, [1.0, 1.0, 1.0, 0.3], [0.0, 0.0, 0.0, 0.4]));
            switch.push(Quad::flat(sxp - 4.5, 67.3, 9.0, 1.6, [0.08, 0.08, 0.09, 0.9]).rotated([sxp, 68.0], 0.6));
            switch.push(Quad::flat(sxp - 0.8, 63.5, 1.6, 9.0, [0.08, 0.08, 0.09, 0.9]).rotated([sxp, 68.0], 0.6));
        }
        switch.push(Quad::rim(px0 - 2.0, py0 - 2.0, pw + 4.0, ph + 4.0, 18.0, 1.0, [0.0, 0.0, 0.0, 0.5], [1.0, 1.0, 1.0, 0.07]));
        // the slot
        let (tc, tw2, ty) = (1102.0f32, 84.0f32, 68.0f32);
        switch.push(Quad::flat(tc - tw2, ty - 11.0, tw2 * 2.0, 22.0, scene::rgb(0x070809)).rounded(11.0));
        switch.push(Quad::inset(tc - tw2, ty - 11.0, tw2 * 2.0, 22.0, 11.0, 7.0, [0.0, 0.0, 0.0, 0.8]));
        switch.push(Quad::rim(tc - tw2, ty - 11.0, tw2 * 2.0, 22.0, 11.0, 1.0, [0.0, 0.0, 0.0, 0.6], [1.0, 1.0, 1.0, 0.09]));
        // a phosphor glow inside the slot on the chosen side
        let gx = tc - tw2 + 24.0 + f * (tw2 * 2.0 - 48.0);
        halos.push(Quad::glow(gx, ty, 30.0, [0.3, 1.0, 0.6, 0.16]));
        // the knob: brushed metal, lit from above, three grip ridges, its shadow on the panel
        let kx = tc - tw2 + 4.0 + f * (tw2 * 2.0 - 8.0 - 76.0);
        let (kw, kh, ky) = (76.0f32, 36.0f32, ty - 18.0);
        switch.push(Quad::flat(kx + 2.0, ky + 4.0, kw, kh, [0.0, 0.0, 0.0, 0.5]).rounded(11.0));
        switch.push(Quad::grad(kx, ky, kw, kh, scene::rgb(0x8c929b), scene::rgb(0x3a3e45), true).rounded(10.0));
        switch.push(Quad::grad(kx + 2.0, ky + 2.0, kw - 4.0, kh * 0.45, [1.0, 1.0, 1.0, 0.22], [1.0, 1.0, 1.0, 0.0], true).rounded(8.0));
        switch.push(Quad::rim(kx, ky, kw, kh, 10.0, 1.2, [1.0, 1.0, 1.0, 0.35], [0.0, 0.0, 0.0, 0.5]));
        let km = Quad { rect: [kx, ky, kw, kh], uv: [0.0, (f * 0.3).fract(), kw / 512.0, kh / 128.0], color: [1.0, 1.0, 1.0, 0.16], ..Quad::full() }.rounded(10.0);
        let kd = Quad { rect: [kx, ky, kw, kh], uv: [0.3, 0.5, kw / 512.0, kh / 128.0], color: [0.0, 0.0, 0.0, 0.22], ..Quad::full() }.rounded(10.0);
        let knob_metal = Batch { tex: Some(self.tex.1), quads: vec![kd, km], add: false, clip: None };
        for g in [-10.0f32, 0.0, 10.0] {
            let cx = kx + kw / 2.0 + g;
            switch.push(Quad::flat(cx - 1.5, ky + 9.0, 2.0, kh - 18.0, [0.1, 0.11, 0.12, 0.85]).rounded(1.0));
            switch.push(Quad::flat(cx + 0.5, ky + 9.0, 1.0, kh - 18.0, [1.0, 1.0, 1.0, 0.25]));
        }
        // the printed labels: the chosen one lit
        let lab = |t: &str, x: f32, on: f32| {
            let c = |a: u32, b: u32| {
                let m = |sh: u32| (((a >> sh) & 255) as f32 + ((((b >> sh) & 255) as f32) - ((a >> sh) & 255) as f32) * on) as u8;
                [m(16), m(8), m(0), 255]
            };
            let mut t = text::Txt::new(t, text::BAR, 16.0, x, 59.0, c(0x6b727b, 0xc9f5d8)).weight(600).align(text::Align::Center);
            if on > 0.5 {
                t = t.glow([110, 255, 170, (120.0 * on) as u8], 3.0);
            }
            // engraved into the panel: a dark edge above the letters, a light one below
            let mut hi = t.clone();
            hi.glow = None;
            hi.y += 1.0;
            for sp in &mut hi.spans {
                sp.color = [255, 255, 255, 22];
            }
            let mut lo = hi.clone();
            lo.y -= 2.0;
            for sp in &mut lo.spans {
                sp.color = [0, 0, 0, 170];
            }
            [lo, hi, t]
        };
        cv_texts.extend(lab("R A D I O", 952.0, (1.0 - f).clamp(0.0, 1.0)));
        cv_texts.extend(lab("O ' Z   T A N L O V I M", 1258.0, f.clamp(0.0, 1.0)));
        // the printer's lamp while the head burns a receipt
        if self.tape.busy() {
            down.quads.push(Quad::plate_cut(1857.0, 41.0, 1875.0, 59.0, 1.0).rounded(9.0));
            halos.push(Quad::glow(1866.0, 50.0, 15.0, [1.0, 0.69, 0.27, 0.22]));
        }
        // the volume ring around the knob
        let lit = (self.volume * 21.0).round() as i32;
        for i in 0..21 {
            if i < lit {
                let a = (225.0 - i as f32 * 13.5f32).to_radians();
                let (x, y) = (1250.0 + a.cos() * 86.0, 958.0 - a.sin() * 86.0);
                down.quads.push(Quad::plate_cut(x - 9.0, y - 9.0, x + 9.0, y + 9.0, 1.0).rounded(9.0));
                let col = if i >= 18 { [1.0, 0.27, 0.16] } else { [1.0, 0.69, 0.27] };
                halos.push(Quad::glow(x, y, 13.0, [col[0], col[1], col[2], 0.11]));
                halos.push(Quad::radial(x - 6.0, y - 6.0, 12.0, 12.0, [1.0, 0.95, 0.85, 0.4], [col[0], col[1], col[2], 0.0]));
                lens_glints.push(Quad::glow(x - 2.5, y - 3.0, 2.4, [1.0, 1.0, 1.0, 0.7]));
            }
        }
        // VU needles: a thin dark blade over each dial, a soft shadow a little off it
        let mut flat = Batch { tex: None, quads: vec![], add: false, clip: None };
        let mut lamps: Vec<Quad> = vec![];
        for (k, cx) in [240.0f32, 468.0].into_iter().enumerate() {
            let piv = [cx, 793.0];
            let rot = self.vu[k].0.to_radians();
            let sh = [60.0 / 255.0, 30.0 / 255.0, 0.0, 0.45];
            flat.quads.push(Quad::needle(cx - 1.5 + 3.0, 723.0 + 4.0, 3.0, 70.0, 0.24, sh, sh, 1.6).rotated([piv[0] + 3.0, piv[1] + 4.0], rot));
            flat.quads.push(Quad::needle(cx - 1.5, 723.0, 3.0, 70.0, 0.24, scene::rgb(0x100d0a), scene::rgb(0x2b2520), 0.0).rotated(piv, rot));
            // the counterweight: a little dark cap at the pivot, lit from the upper left
            flat.quads.push(Quad::glow(cx + 3.0, 797.0, 8.0, [0.24, 0.12, 0.0, 0.4]));
            flat.quads.push(Quad::flat(cx - 5.5, 787.5, 11.0, 11.0, [0.0, 0.0, 0.0, 0.5]).rounded(5.5));
            flat.quads.push(Quad::radial(cx - 5.0, 788.0, 10.0, 10.0, scene::rgb(0x3a322b), scene::rgb(0x15110d)).rounded(5.0));
            flat.quads.push(Quad::glow(cx - 1.5, 791.0, 2.6, [0.55, 0.48, 0.42, 0.7]));
            // the lamp behind the dial: a warm bloom that breathes very slightly like a filament
            let ph = (self.started.elapsed().as_secs_f32() / 6.5).fract();
            const FIL: [(f32, f32); 7] = [(0.0, 0.92), (0.17, 1.0), (0.31, 0.9), (0.46, 0.97), (0.63, 0.88), (0.8, 1.0), (1.0, 0.92)];
            let fl = FIL.windows(2).find(|w| ph <= w[1].0).map(|w| w[0].1 + (w[1].1 - w[0].1) * (ph - w[0].0) / (w[1].0 - w[0].0)).unwrap_or(0.92);
            lamps.push(Quad::glow(cx, 765.0, 64.0, [1.0, 0.6, 0.27, 0.12 * fl]));
            lamps.push(Quad::glow(cx, 765.0, 36.0, [1.0, 0.5, 0.18, 0.05 * fl]));
        }
        let mut batches = vec![base];
        batches.extend(logo);
        // the switch: its panel under the grain, the knob over it, the knob's metal on top
        let knob_at = switch.iter().rposition(|q| q.kind == 3.0 && q.rect[2] == 76.0).unwrap_or(switch.len());
        let knob: Vec<Quad> = switch.split_off(knob_at);
        let (knob_a, knob_b): (Vec<Quad>, Vec<Quad>) = (knob.clone(), vec![]);
        let _ = knob_b;
        batches.extend([upk, down, Batch { tex: None, quads: switch, add: false, clip: None }]);
        batches.extend(textured);
        batches.extend([Batch { tex: None, quads: knob_a, add: false, clip: None }, knob_metal, Batch { tex: None, quads: halos, add: true, clip: None }, Batch { tex: None, quads: lens_glints, add: true, clip: None }]);
        let mut cv = Canvas { batches, texts: cv_texts, clip: None, mask: None };
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
        // the page change (the Tauri "shutter"): 130 ms the old page dims and flares, then 600 ms the new one is
        // written in from the left by a glowing sweep and settles from bright
        let mut phase: Option<(bool, f32)> = None;
        let mut shown = self.panel.screen;
        if let Some((t0, from)) = self.screen_anim {
            let t = t0.elapsed().as_secs_f32();
            if t < 0.13 {
                shown = from;
                phase = Some((false, t / 0.13));
            } else if t < 0.73 {
                phase = Some((true, scene::bezier(0.3, 0.75, 0.25, 1.0, (t - 0.13) / 0.6)));
            } else {
                self.screen_anim = None;
            }
        }
        scene::tube_back(&mut cv);
        let m_ly = (marker(&mut cv), marker(&mut over));
        if shown == panel::Screen::Home {
            if self.lyrics.lines.is_empty() {
                let samples = self.player.as_ref().map(|p| p.levels.samples()).unwrap_or_default();
                self.spec.update(&samples, self.playing, self.started.elapsed().as_secs_f32());
            }
            self.lyrics.draw(&mut cv, &mut over, &mut self.text, pos, &title, &artist, &self.spec);
        }
        let m_ly_end = (marker(&mut cv), marker(&mut over));
        let station = if self.pick { "O'Z TANLOVIM" } else { match self.station.as_str() { "ost" => "OST FM", "riot" => "RIOT FM", _ => "J-POP FM" } };
        let line1 = format!("♫ {title} · {artist}");
        let foot = track.as_ref().map(|t| format!("{station} · {} · {} BPM · ENERGIYA {}%", t.category.to_uppercase(), t.bpm.round(), t.energy.round())).unwrap_or_default();
        let (mx, my) = self.cursor;
        let ghost = match self.scrub {
            Some(x) => Some(x),
            None if self.pick && (595.0..1387.0).contains(&mx) && (677.0..829.0).contains(&my) => Some(mx),
            None => None,
        }
        .map(|x| (x - 622.0) / 738.0);
        scene::draw_lcd(&mut cv, &self.wave, &line1, &foot, pos, dur, ghost);
        self.drum += self.drum_kick * 0.02 * 57.3 * (dt * 60.0);
        self.drum_kick *= 0.9f32.powf(dt * 60.0);
        scene::draw_drum(&mut cv, &mut over, self.drum, self.light);
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
                    light: self.light,
                };
                let real = panel.screen;
                panel.screen = shown;
                let m0 = (marker(&mut cv), marker(&mut over));
                panel.draw(&mut cv, &mut over, &mut self.text, &cx, &mut img);
                let m1 = (marker(&mut cv), marker(&mut over));
                panel.screen = real;
                if let Some((reveal, k)) = phase {
                    for (a, b) in [(m_ly, m_ly_end), (m0, m1)] {
                        if reveal {
                            let [x0, y0, x1, y1] = scene::LY;
                            let r = [x0, y0, x0 + (x1 - x0) * k, y1];
                            restrict(&mut cv, a.0, b.0, r);
                            restrict(&mut over, a.1, b.1, r);
                        } else {
                            fade(&mut cv, a.0, b.0, 1.0 - k);
                            fade(&mut over, a.1, b.1, 1.0 - k);
                        }
                    }
                    let [x0, y0, x1, y1] = scene::LY;
                    let (w, h) = (x1 - x0, y1 - y0);
                    over.mask = Some((scene::LY, 37.0));
                    if reveal {
                        // the glowing head of the sweep, the written part still bright, the rest of the tube empty glass
                        let hx = x0 + w * k;
                        over.light(Quad::flat(x0, y0, hx - x0, h, scene::rgba(0xe6ff9a, 0.1 * (1.0 - k))));
                        over.light(Quad::grad(hx - 46.0, y0, 46.0, h, scene::rgba(0xc4e078, 0.0), scene::rgba(0xc4e078, 0.35 * (1.0 - k * 0.6)), false));
                        over.light(Quad::flat(hx - 2.0, y0, 3.0, h, scene::rgba(0xeaffb0, 0.55 * (1.0 - k * 0.6))));
                    } else {
                        // the old page flares a little as it goes
                        over.light(Quad::flat(x0, y0, w, h, scene::rgba(0xe6ff9a, 0.08 * k)));
                    }
                    over.mask = None;
                }
                scene::lens(&mut over, scene::LY, 37.0);
            }
            self.panel = panel;
        }
        let (line, lines) = self.tape.draw(&mut cv, &mut over, self.tex.2);
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
                // (the tag is drawn with the device, under the texts, so its words are on top of it)
                cv.flat(Quad::flat(960.0 - tw / 2.0 + 2.0, 643.0, tw, 38.0, [0.0, 0.0, 0.0, 0.35]).rounded(8.0));
                cv.flat(Quad::flat(960.0 - tw / 2.0, 640.0, tw, 38.0, [0.62, 0.70, 0.52, 0.97]).rounded(8.0));
                over.text(tt);
            }
        }
        cv.batches.push(flat);
        cv.batches.push(Batch { tex: None, quads: lamps, add: true, clip: None });
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
        // Windows: the DWM shadow on, so Windows 11 also rounds the corners of the borderless window
        #[cfg(windows)]
        let attrs = winit::platform::windows::WindowAttributesExtWindows::with_undecorated_shadow(attrs, true);
        let window = Arc::new(el.create_window(attrs).expect("window"));
        #[cfg(windows)]
        win::setup(&window);
        let mut gfx = pollster::block_on(Gfx::new(window.clone()));
        let load = |g: &mut Gfx, b: &[u8]| g.texture_image(b).expect("plate");
        let up = load(&mut gfx, PLATE_UP);
        let down = load(&mut gfx, PLATE_DOWN);
        let glass = load(&mut gfx, GLASS);
        let logo = (load(&mut gfx, LOGO), load(&mut gfx, LOGO_BG));
        let tex = gen_textures(&mut gfx);
        // the convex tube glasses: the lyrics tube and the chat monitor
        gfx.lenses = [(scene::LY, [0.045, 0.0035]), (chat::TUBE, [0.035, 0.003])];
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
        let cached = Arc::new(net::tracks_cached().unwrap_or_default());
        tune(self.proxy.clone(), "jpop".into(), cached.clone());
        self.st = Some(State {
            gfx,
            up,
            down,
            glass,
            logo,
            plates: [Some((up, down, glass, logo.1)), None],
            light: false,
            tex,
            released: vec![None; keys().len()],
            screen_anim: None,
            sw: (0.0, 0.0),
            player,
            keys: keys(),
            pressed: None,
            cursor: (0.0, 0.0),
            station: "jpop".into(),
            playing: true,
            volume,
            tracks: cached,
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
            scrub: None,
            want_pick: None,
            spec: spectrum::Analyser::new(),
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
            if st.store.get("theme").and_then(|v| v.as_str()) == Some("light") {
                st.set_theme(true);
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
            Msg::Tracks(t) => st.tracks = Arc::new(t),
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
            Msg::Pick { track, src } => {
                if !st.pick || st.want_pick.as_deref() != Some(track.as_str()) {
                    return;
                }
                st.tuning = false;
                if let Some(p) = &st.player {
                    let r = p.play(src);
                    dlog(&format!("pick {track} -> {r:?}"));
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
            Msg::Plates { light, imgs } => {
                let t: Vec<usize> = imgs.iter().map(|i| st.gfx.texture_rgba(i.width(), i.height(), i)).collect();
                st.plates[light as usize] = Some((t[0], t[1], t[2], t[3]));
                st.set_theme(light);
            }
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
            Msg::Tune { station, track, src, start, dur, skew } => {
                st.tuning = false;
                if station != st.station || st.pick {
                    return;
                }
                if let Some(p) = &st.player {
                    if let Err(e) = p.play(src) {
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

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // ask for the next frame every turn of the loop (~60 fps): a paint Windows skipped while the window was
        // minimized or covered must not stop the device for good
        let now = Instant::now();
        if now >= self.next_frame {
            if let Some(st) = &self.st {
                st.gfx.window.request_redraw();
            }
            // 60 fps: the next frame is due 1/60 s after this one (never falling behind more than a frame)
            self.next_frame = (self.next_frame + Duration::from_micros(16_667)).max(now);
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
        // while an edge is dragged Windows sends sizes one after another: draw each at once (no stretched or
        // black frame between the new size and the next paint)
        let resized = matches!(ev, WindowEvent::Resized(_));
        self.window_event_inner(el, ev);
        if resized {
            self.window_event_inner(el, WindowEvent::RedrawRequested);
        }
        let _ = id;
    }
}

impl App {
    fn window_event_inner(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        let Some(st) = self.st.as_mut() else { return };
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => st.gfx.resize(s.width, s.height),
            WindowEvent::CursorMoved { position, .. } => {
                st.cursor = st.gfx.to_stage(position.x, position.y);
                if st.scrub.is_some() {
                    st.scrub = Some(st.cursor.0);
                }
            }
            // the chat tube: its own messages, sheets and keys (right button = a message's actions at once)
            WindowEvent::MouseInput { state: ElementState::Pressed, button: b @ (MouseButton::Left | MouseButton::Right), .. }
                if st.chat.wants(st.cursor.0, st.cursor.1) && !(st.tray && st.tray_pick(st.cursor.0, st.cursor.1).is_some()) =>
            {
                let (x, y) = st.cursor;
                st.tray = false;
                match st.chat.click(x, y, b == MouseButton::Right) {
                    Some(chat::ChatAct::Reply) => {
                        if st.acc.is_some() {
                            st.compose.get_or_insert_with(String::new);
                        } else {
                            st.login();
                        }
                    }
                    Some(chat::ChatAct::Copy(t)) => {
                        let ok = arboard::Clipboard::new().and_then(|mut c| c.set_text(t)).is_ok();
                        st.toast(if ok { "NUSXA OLINDI" } else { "NUSXA OLIB BO'LMADI" });
                    }
                    Some(chat::ChatAct::Toast(t)) => st.toast(&t),
                    None => {}
                }
            }
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
                    ElementState::Pressed if (595.0..1387.0).contains(&x) && (677.0..829.0).contains(&y) => {
                        if st.pick {
                            st.scrub = Some(x);
                        } else {
                            st.toast("JONLI EFIR · O'TKAZIB BO'LMAYDI");
                        }
                    }
                    ElementState::Pressed => match st.key_at(x, y) {
                        Some(i) => {
                            if st.keys[i].id != "compose" && st.keys[i].id != "cid" {
                                st.compose = None;
                            }
                            st.pressed = Some(i);
                            if let Some(p) = &st.player {
                                p.sfx(if st.keys[i].id == "play" { audio::Sfx::Latch } else { audio::Sfx::Down });
                            }
                        }
                        // any surface that is not a control moves the window (the OS's own move loop: Snap works)
                        None => {
                            st.compose = None;
                            st.tray = false;
                            let _ = st.gfx.window.drag_window();
                        }
                    },
                    ElementState::Released => {
                        if let Some(sx) = st.scrub.take() {
                            st.seek_at(sx);
                        }
                        if let Some(i) = st.pressed.take() {
                            st.released[i] = Some(Instant::now());
                            if let (Some(p), false) = (&st.player, st.keys[i].id == "play") {
                                p.sfx(audio::Sfx::Up);
                            }
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
                st.panel.goal = 0.0;
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && event.logical_key == WKey::Named(NamedKey::Escape) && st.compose.is_none() => {
                st.go(panel::Screen::Home);
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
                    st.panel.goal -= wd;
                } else if x >= px0 && x <= px1 && y >= py0 && y <= py1 {
                    st.tape.goal -= wd * 0.6;
                } else if x >= cx0 && x <= cx1 && y >= cy0 && y <= cy1 {
                    let d = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y * 60.0,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    st.chat.goal += d;
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
                            dlog("idle -> next");
                            st.pick_step(1);
                        } else {
                            st.tuning = true;
                            tune(self.proxy.clone(), st.station.clone(), st.tracks.clone());
                        }
                    }
                    if st.token.is_some() && st.last_sync.elapsed() > Duration::from_secs(300) {
                        st.sync();
                    }
                }
                let t0 = Instant::now();
                let batches = st.frame();
                let t1 = Instant::now();
                let over = std::mem::take(&mut st.over);
                let texts = std::mem::take(&mut st.texts);
                let nq: usize = batches.iter().chain(over.iter()).map(|b| b.quads.len()).sum();
                st.gfx.render(&batches, Some((&mut st.text, &texts)), &over);
                st.text.trim();
                if std::env::var_os("KR_DEBUG").is_some() {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("kr_prof.log")) {
                        let _ = writeln!(f, "frame {:.2}ms render {:.2}ms quads {} batches {} texts {}", (t1 - t0).as_secs_f64() * 1e3, t1.elapsed().as_secs_f64() * 1e3, nq, batches.len() + over.len(), texts.len());
                    }
                }
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
    let mut app = App { proxy: event_loop.create_proxy(), st: None, next_frame: Instant::now() };
    event_loop.run_app(&mut app).expect("run");
}
