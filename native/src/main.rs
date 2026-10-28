// KR-01 DESK, native: no browser inside. The device is a set of rendered plates (Blender), drawn on the GPU with
// wgpu; everything live (LEDs, VU needles, pressed keys, …) is quads on top. Audio is decoded and played by the
// app itself (symphonia + cpal), so it needs no system codecs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod gfx;
mod net;
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
            "st0" | "st1" | "st2" => {
                let st = STATIONS[id[2..].parse::<usize>().unwrap()].to_string();
                if st != self.station {
                    self.station = st.clone();
                    self.playing = true;
                    self.tuning = true;
                    tune(proxy.clone(), st);
                }
            }
            _ => {}
        }
    }

    /// Is this key shown pressed / lit right now (latched keys stay down)?
    fn key_down(&self, i: usize) -> bool {
        let id = self.keys[i].id;
        if self.pressed == Some(i) {
            return true;
        }
        match id {
            "play" => self.playing,
            "nav0" => true,
            "st0" | "st1" | "st2" => STATIONS[id[2..].parse::<usize>().unwrap()] == self.station,
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

        let base = Batch { tex: Some(self.up), quads: vec![Quad::full()], add: false };
        // pressed / latched keys: their own shape cut from the "down" plate, nothing around them changes
        let mut down = Batch { tex: Some(self.down), quads: vec![], add: false };
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
        let mut flat = Batch { tex: None, quads: vec![], add: false };
        for (k, cx) in [240.0f32, 468.0].into_iter().enumerate() {
            let piv = [cx, 793.0];
            let rot = self.vu[k].0.to_radians();
            flat.quads.push(Quad::flat(cx - 1.2 + 3.0, 723.0 + 4.0, 2.4, 70.0, [0.24, 0.12, 0.0, 0.35]).rotated([piv[0] + 3.0, piv[1] + 4.0], rot));
            flat.quads.push(Quad::flat(cx - 1.2, 723.0, 2.4, 70.0, [0.07, 0.055, 0.045, 1.0]).rotated(piv, rot));
            flat.quads.push(Quad::flat(cx - 5.0, 788.0, 10.0, 10.0, [0.1, 0.08, 0.06, 1.0]).rounded(5.0));
        }
        let mut cv = Canvas { batches: vec![base, down], texts: vec![] };
        let mut over = Canvas::new();
        let track = self.now.as_ref().and_then(|id| self.tracks.get(id)).cloned();
        let pos = if self.slot.0 > 0 { ((net::now_ms() + self.slot.2 - self.slot.0) as f64 / 1000.0).max(0.0) } else { 0.0 };
        let dur = self.slot.1 as f64 / 1000.0;
        self.tv.draw(&mut cv);
        let (title, artist) = track.as_ref().map(|t| (t.title.clone(), t.artist.clone())).unwrap_or(("KAWAII RADIO".into(), "ulanmoqda…".into()));
        self.lyrics.draw(&mut cv, &mut over, &mut self.text, pos, &title, &artist);
        let station = match self.station.as_str() { "ost" => "OST FM", "riot" => "RIOT FM", _ => "J-POP FM" };
        let line1 = format!("♫ {title} · {artist}");
        let foot = track.as_ref().map(|t| format!("{station} · {} · {} BPM · ENERGIYA {}%", t.category.to_uppercase(), t.bpm.round(), t.energy.round())).unwrap_or_default();
        scene::draw_lcd(&mut cv, &self.wave, &line1, &foot, pos, dur);
        self.drum += self.drum_kick * 0.02 * 57.3 * (dt * 60.0);
        self.drum_kick *= 0.9f32.powf(dt * 60.0);
        scene::draw_drum(&mut cv, &mut over, self.drum);
        cv.batches.push(flat);
        // the rendered cover glass over everything, except the lyrics tube (it has its own lens)
        let mut glass = Batch { tex: Some(self.glass), quads: vec![], add: false };
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
        });
        window.request_redraw();
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, msg: Msg) {
        let Some(st) = self.st.as_mut() else { return };
        match msg {
            Msg::Tracks(t) => st.tracks = t,
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
                    ElementState::Pressed => match st.key_at(x, y) {
                        Some(i) => st.pressed = Some(i),
                        // any surface that is not a control moves the window (the OS's own move loop: Snap works)
                        None => {
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
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = st.cursor;
                if ((x - 1250.0).powi(2) + (y - 958.0).powi(2)).sqrt() < 110.0 {
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
                        st.tuning = true;
                        tune(self.proxy.clone(), st.station.clone());
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
    let event_loop = EventLoop::<Msg>::with_user_event().build().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { proxy: event_loop.create_proxy(), st: None };
    event_loop.run_app(&mut app).expect("run");
}
