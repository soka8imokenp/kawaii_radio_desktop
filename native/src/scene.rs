//! The live parts of the device, drawn every frame: the lyrics CRT, the LCD, the tuner drum and the trivision
//! cover. Coordinates are stage px (the rendered plates' grid), the same numbers as the Tauri version's CSS.

use crate::gfx::{Batch, Quad};
use crate::net::Line;
use crate::text::{self, Align, Span, Txt};
use std::collections::HashMap;
use std::time::Instant;

/// Quads (batched by texture, in draw order) and texts of one frame.
pub struct Canvas {
    pub batches: Vec<Batch>,
    pub texts: Vec<Txt>,
    /// new quads are drawn only inside this stage rect
    pub clip: Option<[f32; 4]>,
}

impl Canvas {
    pub fn new() -> Self {
        Canvas { batches: vec![], texts: vec![], clip: None }
    }
    pub fn quad(&mut self, tex: Option<usize>, q: Quad) {
        match self.batches.last_mut() {
            Some(b) if b.tex == tex && !b.add && b.clip == self.clip => b.quads.push(q),
            _ => self.batches.push(Batch { tex, quads: vec![q], add: false, clip: self.clip }),
        }
    }
    pub fn light(&mut self, q: Quad) {
        match self.batches.last_mut() {
            Some(b) if b.tex.is_none() && b.add && b.clip == self.clip => b.quads.push(q),
            _ => self.batches.push(Batch { tex: None, quads: vec![q], add: true, clip: self.clip }),
        }
    }
    pub fn flat(&mut self, q: Quad) {
        self.quad(None, q)
    }
    pub fn text(&mut self, t: Txt) {
        self.texts.push(t);
    }
}

pub fn rgb(hex: u32) -> [f32; 4] {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0, 1.0]
}
pub fn rgba(hex: u32, a: f32) -> [f32; 4] {
    let mut c = rgb(hex);
    c[3] = a;
    c
}
fn c8(hex: u32, a: f32) -> [u8; 4] {
    [((hex >> 16) & 255) as u8, ((hex >> 8) & 255) as u8, (hex & 255) as u8, (a.clamp(0.0, 1.0) * 255.0) as u8]
}
fn mix(a: u32, b: u32, f: f32) -> u32 {
    let ch = |s: u32| -> u32 {
        let x = ((a >> s) & 255) as f32;
        let y = ((b >> s) & 255) as f32;
        (x + (y - x) * f).round() as u32
    };
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/* ───────── the lyrics CRT (#lyr: 610,190 760x444; the tube is inset 13, radius 37) */
pub const LY: [f32; 4] = [623.0, 203.0, 1357.0, 621.0];

/// One line's animated place: it rises, grows and dims with CSS-like easing.
#[derive(Clone, Copy)]
struct LineAnim {
    y: f32,
    size: f32,
    color: u32,
    alpha: f32,
    alive: bool,
}

pub struct Lyrics {
    pub lines: Vec<Line>,
    pub trans: Vec<Line>,
    anim: HashMap<usize, LineAnim>,
    last: Instant,
}

impl Lyrics {
    pub fn new() -> Self {
        Lyrics { lines: vec![], trans: vec![], anim: HashMap::new(), last: Instant::now() }
    }
    pub fn set(&mut self, lines: Vec<Line>, trans: Vec<Line>) {
        self.lines = lines;
        self.trans = trans;
        self.anim.clear();
    }

    pub fn draw(&mut self, cv: &mut Canvas, over: &mut Canvas, tl: &mut text::TextLayer, t: f64, title: &str, artist: &str) {
        let dt = self.last.elapsed().as_secs_f32().min(0.1);
        self.last = Instant::now();
        let [x0, y0, x1, y1] = LY;
        let (w, h) = (x1 - x0, y1 - y0);
        // the phosphor backplate: radial(135% 125% at 50% 44%, #20271b, #171d14 42%, #0d110b 72%, #050705)
        cv.flat(Quad::flat(x0, y0, w, h, rgb(0x050705)).rounded(37.0));
        // (every radial quad is exactly the tube, rounded like it: nothing spills over the device)
        cv.flat(Quad::radial(x0, y0, w, h, rgb(0x20271b), rgb(0x0b0f09)).rounded(37.0));
        cv.flat(Quad::radial(x0, y0, w, h, rgba(0x96aa5a, 0.06), rgba(0x96aa5a, 0.0)).rounded(37.0));
        cv.flat(Quad::dots(x0, y0, w, h, 5.0, 1.1, rgba(0x96af5f, 0.07)).rounded(37.0));
        // header: a green dot, the title, the artist, a hairline
        let hx = 610.0 + 34.0;
        cv.light(Quad::glow(hx + 3.0, 190.0 + 34.0, 8.0, rgba(0x4be38f, 0.9)));
        cv.flat(Quad::flat(hx, 190.0 + 31.0, 6.0, 6.0, rgb(0x7dffb8)).rounded(3.0));
        let tt = Txt::new(title, text::MONO, 16.0, hx + 16.0, 190.0 + 23.0, c8(0xeef0e6, 1.0)).weight(600).glow([230, 240, 210, 7], 2.2);
        let tw = tl.measure(&tt).min(430.0);
        cv.text(tt.clip([hx, y0, hx + 16.0 + 430.0, y1]));
        cv.text(Txt::new(artist, text::MONO, 15.0, hx + 28.0 + tw, 190.0 + 24.0, c8(0x7b8263, 1.0)).clip([hx, y0, x1 - 40.0, y1]));
        cv.flat(Quad::grad(640.0, 250.0, (x1 - 30.0 - 640.0) / 2.0, 1.0, rgba(0xa0b46e, 0.03), rgba(0xa0b46e, 0.2), false));
        cv.flat(Quad::grad(640.0 + (x1 - 30.0 - 640.0) / 2.0, 250.0, (x1 - 30.0 - 640.0) / 2.0, 1.0, rgba(0xa0b46e, 0.2), rgba(0xa0b46e, 0.03), false));

        let (bx, by, bw) = (650.0, 266.0, 656.0);
        let clip = [x0 + 8.0, 256.0, x1 - 40.0, y1 - 10.0];
        if self.lines.is_empty() {
            cv.text(Txt::new("♪ MATNSIZ · SPEKTR", text::MONO, 20.0, bx, by + 62.0, c8(0x7c8456, 1.0)).weight(500).clip(clip));
        } else {
            let i = self.lines.iter().rposition(|l| t >= l.t);
            // targets: two before, the sung one (big, warm, its sung part bright), three after
            let mut want: Vec<(usize, f32, f32, u32)> = vec![];
            let ci = i.map(|v| v as i64).unwrap_or(-1);
            let push = |k: i64, y: f32, s: f32, c: u32, want: &mut Vec<(usize, f32, f32, u32)>| {
                if k >= 0 && (k as usize) < self.lines.len() {
                    want.push((k as usize, y, s, c));
                }
            };
            push(ci - 2, 8.0, 19.0, 0x5d6446, &mut want);
            push(ci - 1, 46.0, 21.0, 0x8c9367, &mut want);
            let len = i.map(|k| self.lines[k].s.chars().count()).unwrap_or(0);
            let size = (1980.0 / len.max(1) as f32).clamp(25.0, 42.0);
            let wrap = if len as f32 * size * 0.6 > 630.0 { 2.0 } else { 1.0 };
            let hh = wrap * size * 1.22;
            push(ci, 104.0, size, 0xf2e49c, &mut want);
            let tr = i.and_then(|k| {
                let lt = self.lines[k].t;
                self.trans.iter().rev().find(|l| l.t <= lt + 0.5).map(|l| l.s.clone())
            });
            let nb = 104.0 + if i.is_some() { hh } else { 40.0 } + if tr.is_some() { 52.0 } else { 40.0 };
            push(ci + 1, nb, 22.0, 0xcdc98b, &mut want);
            push(ci + 2, nb + 42.0, 20.0, 0x9a9f72, &mut want);
            push(ci + 3, nb + 78.0, 17.0, 0x5c6247, &mut want);
            // ease every line toward its place (0.65 s, cubic-bezier(.25,.8,.25,1) ~ an exponential approach)
            let k = 1.0 - (-dt / 0.16).exp();
            for a in self.anim.values_mut() {
                a.alive = false;
            }
            for (idx, y, s, c) in &want {
                let a = self.anim.entry(*idx).or_insert(LineAnim { y: y + 36.0, size: *s, color: *c, alpha: 0.0, alive: true });
                a.alive = true;
                a.y += (y - a.y) * k;
                a.size += (s - a.size) * k;
                a.color = mix(a.color, *c, k);
                a.alpha += (1.0 - a.alpha) * k;
            }
            self.anim.retain(|_, a| {
                if !a.alive {
                    a.y -= 34.0 * k;
                    a.alpha -= 2.0 * dt;
                }
                a.alpha > 0.01
            });
            for (idx, a) in &self.anim {
                let s = &self.lines[*idx].s;
                let now = Some(*idx) == i;
                let mut tx = Txt::new(s.clone(), text::MONO, a.size, bx, by + a.y, c8(a.color, a.alpha)).weight(if now { 600 } else { 500 }).clip(clip)
                    .glow([205, 215, 130, if now { 9 } else { 5 }], if now { 3.0 } else { 2.2 });
                if now {
                    // karaoke: the sung part bright, the rest dim (by time within the line)
                    let t0 = self.lines[*idx].t;
                    let t1 = self.lines.get(idx + 1).map(|l| l.t).unwrap_or(t0 + 4.0);
                    let p = ((t - t0) / (t1 - t0).max(0.2)).clamp(0.0, 1.0);
                    let chars: Vec<char> = s.chars().collect();
                    let cut = (p * chars.len() as f64).round() as usize;
                    let (a1, a2): (String, String) = (chars[..cut].iter().collect(), chars[cut..].iter().collect());
                    tx.spans = vec![Span { text: a1, color: c8(0xf6e8a2, a.alpha), weight: 600 }, Span { text: a2, color: c8(0xb4b67c, a.alpha), weight: 600 }];
                    tx = tx.wrap(bw).line(a.size * 1.22);
                    // the phosphor glow behind the sung line
                    cv.light(Quad::glow(bx + bw * 0.3, by + a.y + a.size * 0.8, bw * 0.5, rgba(0xc8d27a, 0.05 * a.alpha)));
                } else {
                    tx = tx.wrap(bw * 1.6).line(a.size * 1.25);
                }
                cv.text(tx);
            }
            if i.is_none() {
                cv.text(Txt::new("♪ ♪ ♪", text::MONO, 34.0, bx, by + 104.0, c8(0x6d7450, 1.0)).clip(clip));
            }
            if let Some(tr) = tr {
                cv.text(Txt::new(tr, text::MONO, 17.0, bx + 2.0, by + 104.0 + hh + 10.0, c8(0xa3a978, 1.0)).clip(clip));
            }
            // the progress rail on the right
            let n = self.lines.len().min(14);
            let pos = i.map(|k| ((k as f32 / self.lines.len().max(1) as f32) * n as f32) as usize).unwrap_or(0);
            for kk in 0..n {
                let ry = 190.0 + 90.0 + kk as f32 * 22.0;
                if kk == pos {
                    cv.light(Quad::glow(1370.0 - 30.0 - 2.5, ry + 6.0, 9.0, rgba(0xf0e7a0, 0.6)));
                    cv.flat(Quad::flat(1370.0 - 30.0 - 5.0, ry, 5.0, 13.0, rgb(0xf0e7a0)).rounded(1.5));
                } else {
                    cv.flat(Quad::flat(1370.0 - 30.0 - 4.5, ry, 4.0, 4.0, rgb(0x3c4430)).rounded(1.0));
                }
            }
        }
        // the lens: scanlines, a deep shadow at the walls, a vignette, one faint reflection, the rim
        over.flat(Quad::scan(x0, y0, w, h, 0.30).rounded(37.0));
        lens(over, LY, 37.0);
    }
}

/// The chat tube's glass: the same, a little lighter (its picture is darker than the lyrics')
pub fn lens_light(over: &mut Canvas, r: [f32; 4], rad: f32) {
    let [x0, y0, x1, y1] = r;
    let (w, h) = (x1 - x0, y1 - y0);
    over.flat(Quad::radial(x0, y0, w, h, rgba(0, 0.0), rgba(0, 0.35)).rounded(rad));
    over.flat(Quad::rim(x0, y0, w, h, rad, 18.0, rgba(0, 0.3), rgba(0, 0.4)));
    over.flat(Quad::grad(x0, y0, w, h * 0.12, rgba(0xe1f0ff, 0.06), rgba(0xe1f0ff, 0.0), true).rounded(rad));
    over.flat(Quad::rim(x0 + 0.5, y0 + 0.5, w - 1.0, h - 1.0, rad, 1.5, rgba(0xeef6ff, 0.36), rgba(0xeef6ff, 0.1)));
    over.light(Quad::glow(x1 - rad * 0.45, y0 + rad * 0.45, rad * 0.9, rgba(0xffffff, 0.1)));
}

/// The glass in front of a picture tube: a shadow at the walls, the light along the top, one curved reflection
/// (CSS: radial-gradient(150% 130% at -14% 122%, transparent 69%, light 69.8%, …)), and the polished rim.
pub fn lens(over: &mut Canvas, r: [f32; 4], rad: f32) {
    let [x0, y0, x1, y1] = r;
    let (w, h) = (x1 - x0, y1 - y0);
    over.flat(Quad::radial(x0, y0, w, h, rgba(0, 0.0), rgba(0, 0.42)).rounded(rad));
    over.flat(Quad::rim(x0, y0, w, h, rad, 20.0, rgba(0, 0.32), rgba(0, 0.42)));
    over.flat(Quad::grad(x0, y0, w, h * 0.12, rgba(0xe1f0ff, 0.07), rgba(0xe1f0ff, 0.0), true).rounded(rad));
    let (rx, ry) = (1.5 * w * 0.69, 1.3 * h * 0.69);
    over.flat(Quad::arc_light(x0, y0, w, h, [x0 - 0.14 * w, y0 + 1.22 * h], rx, 8.0, rx / ry, rgba(0xcddedc, 0.09)).rounded(rad));
    over.light(Quad::glow(x1 - w * 0.16, y0 + h * 0.16, w * 0.2, rgba(0xc8d7e1, 0.035)));
    over.flat(Quad::rim(x0 + 0.5, y0 + 0.5, w - 1.0, h - 1.0, rad, 1.6, rgba(0xeef6ff, 0.42), rgba(0xeef6ff, 0.12)));
    over.light(Quad::glow(x1 - rad * 0.45, y0 + rad * 0.45, rad * 0.9, rgba(0xffffff, 0.12)));
    over.light(Quad::glow(x0 + rad * 0.45, y0 + rad * 0.45, rad * 0.7, rgba(0xffffff, 0.06)));
}

/* ───────── the LCD (#lcd: 595,677 792x152) */
pub fn wave_of(id: &str) -> Vec<f32> {
    let mut h: u32 = 2166136261;
    for c in id.encode_utf16() {
        h = (h ^ c as u32).wrapping_mul(16777619);
    }
    let mut rnd = || {
        h = (h ^ (h >> 15)).wrapping_mul(2246822507) ^ (h ^ (h >> 13)).wrapping_mul(3266489909);
        h as f32 / 4294967296.0
    };
    (0..180)
        .map(|i| {
            let i = i as f32;
            let a = (i * 0.43 + rnd()).sin() * 30.0 + (i * 0.13).sin() * 16.0 + rnd() * 10.0;
            (8.0 + a.abs()).min(54.0)
        })
        .collect()
}

fn fmt(s: f64) -> String {
    let s = s.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn draw_lcd(cv: &mut Canvas, wave: &[f32], line1: &str, foot: &str, pos: f64, dur: f64) {
    let (ox, oy, lw) = (595.0f32, 677.0f32, 792.0f32);
    let wv = lw - 54.0;
    let ink = 0x232b1b;
    // LCD ink sits a hair above its backing: a faint shadow under every glyph
    let lcd_text = |cv: &mut Canvas, t: Txt| {
        let mut sh = t.clone();
        sh.x += 1.6;
        sh.y += 1.6;
        for s in &mut sh.spans {
            s.color = c8(ink, 0.22);
        }
        cv.text(sh);
        cv.text(t);
    };
    let clip = [ox, oy, ox + lw, oy + 152.0];
    lcd_text(cv, Txt::new(line1, text::DOT, 20.0, ox + 28.0, oy + 14.0, c8(ink, 1.0)).clip([ox, oy, ox + lw - 260.0, oy + 152.0]));
    cv.text(Txt::new("8:88", text::VT, 46.0, ox + lw - 88.0, oy + 4.0, c8(ink, 0.18)).align(Align::Right).clip(clip));
    lcd_text(cv, Txt::new(fmt(pos), text::VT, 46.0, ox + lw - 88.0, oy + 4.0, c8(ink, 1.0)).align(Align::Right).clip(clip));
    cv.text(Txt::new(format!("/ {}", fmt(dur)), text::VT, 22.0, ox + lw - 20.0, oy + 26.0, c8(ink, 0.45)).align(Align::Right).clip(clip));
    let frac = if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let n = wave.len().max(1) as f32;
    for (i, h) in wave.iter().enumerate() {
        let x = ox + 27.0 + i as f32 * (wv / n);
        let on = (i as f32 / n) < frac;
        cv.flat(Quad::flat(x + 1.6, oy + 77.0 - h / 2.0 + 1.6, wv / n * 0.62, *h, rgba(ink, if on { 0.2 } else { 0.0 })));
        cv.flat(Quad::flat(x, oy + 77.0 - h / 2.0, wv / n * 0.62, *h, rgba(ink, if on { 1.0 } else { 0.18 })));
    }
    cv.flat(Quad::flat(ox + 27.0 + frac * wv, oy + 47.0, 3.0, 62.0, rgb(ink)));
    cv.flat(Quad::flat(ox + 27.0, oy + 117.0, wv, 1.0, rgba(ink, 0.18)));
    lcd_text(cv, Txt::new(foot, text::VT, 22.0, ox + 29.0, oy + 122.0, c8(ink, 1.0)).clip(clip));
}

/* ───────── the tuner drum (#drum: 258,866 500x92 — a brushed graphite cylinder with a printed scale) */
pub fn draw_drum(cv: &mut Canvas, over: &mut Canvas, angle_deg: f32) {
    let (ox, oy, w, h, r) = (258.0f32, 866.0f32, 494.0f32, 92.0f32, 239.0f32);
    // body: the cylinder's shading across its width
    let st = [0x0c0d0f, 0x25282d, 0x40444b, 0x565b63, 0x3a3e45, 0x212328, 0x0b0c0e];
    let ps = [0.0, 0.1, 0.3, 0.42, 0.62, 0.88, 1.0];
    for k in 0..6 {
        cv.flat(Quad::grad(ox + w * ps[k], oy, w * (ps[k + 1] - ps[k]) + 0.5, h, rgb(st[k]), rgb(st[k + 1]), false));
    }
    // knurled rims, turning with the drum
    for (y0, y1) in [(0.0, 11.0), (h - 11.0, h)] {
        for k in -120..=120 {
            let th = (k as f32 * 1.5 - angle_deg.rem_euclid(1.5)).to_radians();
            if th.abs() > 1.45 {
                continue;
            }
            let x = ox + w / 2.0 + r * th.sin();
            let vis = th.cos();
            let rw = (1.6 * vis).max(0.4);
            cv.flat(Quad::flat(x - rw, oy + y0, rw * 0.6, y1 - y0, [1.0, 1.0, 1.0, 0.22 * vis]));
            cv.flat(Quad::flat(x, oy + y0, rw * 0.7, y1 - y0, [0.0, 0.0, 0.0, 0.55 * vis]));
        }
    }
    // the printed band
    let bs = [(0.0, 0.0), (0.12, 0.75), (0.45, 0.8), (0.88, 0.75), (1.0, 0.0)];
    for k in 0..4 {
        cv.flat(Quad::grad(ox + w * bs[k].0, oy + 12.0, w * (bs[k + 1].0 - bs[k].0) + 0.5, h - 24.0, rgba(0x16171a, bs[k].1), rgba(0x16171a, bs[k + 1].1), false));
    }
    cv.flat(Quad::flat(ox, oy + 12.0, w, 1.0, [1.0, 1.0, 1.0, 0.08]));
    cv.flat(Quad::flat(ox, oy + h - 13.0, w, 1.0, [1.0, 1.0, 1.0, 0.08]));
    // the scale: ticks and numbers squeezed by the curvature, fading as they turn away
    let ink = 0xe8e2d0;
    let base = (angle_deg / 2.0).floor() as i32;
    for k in -64..=64 {
        let th = (k as f32 * 2.0 - (angle_deg - base as f32 * 2.0)).to_radians();
        if th.abs() > 1.42 {
            continue;
        }
        let x = ox + w / 2.0 + r * th.sin();
        let vis = th.cos();
        let n = k + base;
        let long = n.rem_euclid(5) == 0;
        let a = 0.12 + 0.78 * vis * vis;
        let tw = (if long { 2.2 } else { 1.1 }) * (0.35 + 0.65 * vis);
        let (top, bot) = if long { (16.0, 54.0) } else { (22.0, 46.0) };
        cv.flat(Quad::flat(x - tw / 2.0, oy + top, tw, bot - top, rgba(ink, a)));
        if long && vis > 0.35 {
            let v = (n / 5).rem_euclid(100);
            if v % 10 == 0 {
                // every tenth mark: a little pixel heart in KR red
                const HEART: [&str; 6] = [" XX XX ", "XXXXXXX", "XXXXXXX", " XXXXX ", "  XXX  ", "   X   "];
                let al = 0.2 + 0.75 * vis * vis;
                for (ry, row) in HEART.iter().enumerate() {
                    for (rx, ch) in row.chars().enumerate() {
                        if ch == 'X' {
                            cv.flat(Quad::flat(x + (rx as f32 - 3.5) * 1.7 * vis, oy + 76.0 + (ry as f32 - 6.0) * 1.7 - 2.0, 1.7 * vis, 1.7, rgba(0xe2463c, al)));
                        }
                    }
                }
            } else {
                cv.text(Txt::new(format!("{v:02}"), text::BAR, 15.0, x, oy + 62.0, c8(ink, a)).weight(600).align(Align::Center).clip([ox, oy, ox + w, oy + h]));
            }
        }
    }
    cv.flat(Quad::flat(ox, oy + 59.0, w, 1.0, rgba(ink, 0.32)));
    // the drum turning away into the dark at both ends, the window's lip shading top and bottom
    over.flat(Quad::grad(ox, oy, w * 0.14, h, rgba(0, 0.75), rgba(0, 0.0), false));
    over.flat(Quad::grad(ox + w * 0.86, oy, w * 0.14, h, rgba(0, 0.0), rgba(0, 0.8), false));
    over.flat(Quad::grad(ox, oy, w, h * 0.16, rgba(0, 0.6), rgba(0, 0.0), true));
    over.flat(Quad::grad(ox, oy + h * 0.84, w, h * 0.16, rgba(0, 0.0), rgba(0, 0.45), true));
    over.flat(Quad::grad(ox + 130.0, oy, 50.0, h, [1.0, 1.0, 1.0, 0.0], [1.0, 1.0, 1.0, 0.1], false));
    over.flat(Quad::grad(ox + 180.0, oy, 50.0, h, [1.0, 1.0, 1.0, 0.1], [1.0, 1.0, 1.0, 0.0], false));
    // the pointer: red acrylic, its shadow on the drum, a little light inside
    let px = ox + w / 2.0;
    over.flat(Quad::flat(px + 2.0, oy + 8.0, 6.0, h - 14.0, [0.0, 0.0, 0.0, 0.35]).rounded(3.0));
    over.light(Quad::glow(px, oy + h / 2.0, 14.0, rgba(0xe63c28, 0.25)));
    over.flat(Quad::grad(px - 4.0, oy + 6.0, 4.0, h - 12.0, rgb(0x7e1a16), rgb(0xec7466), false).rounded(2.0));
    over.flat(Quad::grad(px, oy + 6.0, 4.0, h - 12.0, rgb(0xec7466), rgb(0x8e1d18), false).rounded(2.0));
}

/* ───────── the trivision (#tv: 156,196 436x432): 15 three-sided slats, a wave of 120° turns */
pub struct Trivision {
    pub cur: Option<usize>,
    pub next: Option<usize>,
    start: Option<Instant>,
}

pub const TV: [f32; 4] = [156.0, 196.0, 592.0, 628.0];
const NS: usize = 15;

impl Trivision {
    pub fn new() -> Self {
        Trivision { cur: None, next: None, start: None }
    }
    /// A new cover: the slats turn to it in a wave (the first one just appears).
    pub fn show(&mut self, tex: usize) {
        if self.cur.is_none() {
            self.cur = Some(tex);
            return;
        }
        if let (Some(n), Some(_)) = (self.next, self.start) {
            self.cur = Some(n);
        }
        self.next = Some(tex);
        self.start = Some(Instant::now());
    }

    pub fn draw(&mut self, cv: &mut Canvas) {
        let [x0, y0, x1, y1] = TV;
        let (w, h) = (x1 - x0, y1 - y0);
        let sw = w / NS as f32;
        cv.flat(Quad::flat(x0, y0, w, h, rgb(0x0c0d0f)).rounded(14.0));
        let el = self.start.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0);
        let mut done = self.start.is_some();
        for i in 0..NS {
            // each slat starts 38 ms after its neighbour and turns in 0.62 s with a small overshoot
            let p = ((el - i as f32 * 0.038) / 0.62).clamp(0.0, 1.0);
            if p < 1.0 {
                done = false;
            }
            let e = if self.start.is_some() { ease_back(p) } else { 0.0 };
            let th = e * 120f32.to_radians();
            // a prism seen from the front: the leaving face shrinks to the right edge, the arriving face grows
            // from the left one; a face is lit by how much it faces us
            let faces: [(Option<usize>, f32, f32); 2] = [(self.cur, th.cos().max(0.0), 0.0), (self.next, (th - 120f32.to_radians()).cos().max(0.0), 1.0)];
            let sx = x0 + i as f32 * sw;
            let mut acc = 0.0;
            for (tex, vis, side) in faces {
                let Some(tex) = tex else { continue };
                if vis <= 0.001 || (side == 1.0 && self.start.is_none()) {
                    continue;
                }
                let fw = sw * vis;
                let fx = if side == 0.0 { sx + sw - fw } else { sx + acc };
                acc += fw;
                let u0 = i as f32 / NS as f32;
                let q = Quad {
                    rect: [fx + 0.7, y0, (fw - 1.4).max(0.0), h],
                    uv: [u0, 0.0, 1.0 / NS as f32, 1.0],
                    color: [0.55 + 0.45 * vis, 0.55 + 0.45 * vis, 0.55 + 0.45 * vis, 1.0],
                    pivot: [0.0, 0.0],
                    rot: 0.0,
                    kind: 0.0,
                    radius: 0.0,
                    _pad: 0.0,
                };
                cv.quad(Some(tex), q);
            }
            // the printed face bulges a little (CSS: rgba(0,0,0,.24) 0, .05 10%, white .05 40%, transparent 70%,
            // black .14 100%), with a dark gap between the slats
            cv.flat(Quad::grad(sx, y0, sw * 0.1, h, rgba(0, 0.24), rgba(0, 0.05), false));
            cv.flat(Quad::grad(sx + sw * 0.1, y0, sw * 0.3, h, rgba(0, 0.05), [1.0, 1.0, 1.0, 0.05], false));
            cv.flat(Quad::grad(sx + sw * 0.4, y0, sw * 0.3, h, [1.0, 1.0, 1.0, 0.05], [1.0, 1.0, 1.0, 0.0], false));
            cv.flat(Quad::grad(sx + sw * 0.7, y0, sw * 0.3, h, rgba(0, 0.0), rgba(0, 0.14), false));
            cv.flat(Quad::flat(sx + sw - 0.7, y0, 1.4, h, rgba(0x08090a, 0.85)));
        }
        if done {
            self.cur = self.next.take();
            self.start = None;
        }
        // deep inside the device: shadow under the housing lips and at the side walls
        cv.flat(Quad::grad(x0, y0, w, 34.0, rgba(0, 0.88), rgba(0, 0.0), true));
        cv.flat(Quad::grad(x0, y1 - 28.0, w, 28.0, rgba(0, 0.0), rgba(0, 0.82), true));
        cv.flat(Quad::grad(x0, y0, 22.0, h, rgba(0, 0.65), rgba(0, 0.0), false));
        cv.flat(Quad::grad(x1 - 22.0, y0, 22.0, h, rgba(0, 0.0), rgba(0, 0.65), false));
    }
}

fn ease_back(t: f32) -> f32 {
    // cubic-bezier(.34, 1.56, .64, 1) ~ easeOutBack
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}
