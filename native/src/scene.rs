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
    /// and only inside this rounded rect (rect, radius)
    pub mask: Option<([f32; 4], f32)>,
}

impl Canvas {
    pub fn new() -> Self {
        Canvas { batches: vec![], texts: vec![], clip: None, mask: None }
    }
    pub fn quad(&mut self, tex: Option<usize>, mut q: Quad) {
        if let Some((m, r)) = self.mask {
            q.mask = m;
            q.mask_r = [r, 0.0];
        }
        match self.batches.last_mut() {
            Some(b) if b.tex == tex && !b.add && b.clip == self.clip => b.quads.push(q),
            _ => self.batches.push(Batch { tex, quads: vec![q], add: false, clip: self.clip }),
        }
    }
    pub fn light(&mut self, mut q: Quad) {
        if let Some((m, r)) = self.mask {
            q.mask = m;
            q.mask_r = [r, 0.0];
        }
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

    pub fn draw(&mut self, cv: &mut Canvas, over: &mut Canvas, tl: &mut text::TextLayer, t: f64, title: &str, artist: &str, spec: &crate::spectrum::Analyser) {
        let dt = self.last.elapsed().as_secs_f32().min(0.1);
        self.last = Instant::now();
        let [x0, y0, x1, y1] = LY;
        let (w, h) = (x1 - x0, y1 - y0);
        // header: a green dot, the title, the artist, a hairline
        let hx = 610.0 + 34.0;
        cv.light(Quad::glow(hx + 3.0, 190.0 + 41.0, 8.0, rgba(0x4be38f, 0.9)));
        cv.flat(Quad::flat(hx + 4.0, 190.0 + 38.0, 6.0, 6.0, rgb(0x7dffb8)).rounded(3.0));
        let tt = Txt::new(title, text::MONO, 16.0, hx + 20.0, 190.0 + 30.0, c8(0xeef0e6, 1.0)).weight(600).glow([230, 240, 210, 70], 2.2);
        let tw = tl.measure(&tt).min(430.0);
        cv.text(tt.clip([hx, y0, hx + 20.0 + 430.0, y1]));
        cv.text(Txt::new(artist, text::MONO, 15.0, hx + 32.0 + tw, 190.0 + 31.0, c8(0x7b8263, 1.0)).clip([hx, y0, x1 - 40.0, y1]));
        cv.flat(Quad::grad(640.0, 256.0, (x1 - 30.0 - 640.0) / 2.0, 1.0, rgba(0xa0b46e, 0.03), rgba(0xa0b46e, 0.2), false));
        cv.flat(Quad::grad(640.0 + (x1 - 30.0 - 640.0) / 2.0, 256.0, (x1 - 30.0 - 640.0) / 2.0, 1.0, rgba(0xa0b46e, 0.2), rgba(0xa0b46e, 0.03), false));

        let (bx, by, bw) = (650.0, 266.0, 656.0);
        let clip = [x0 + 8.0, 256.0, x1 - 40.0, y1 - 10.0];
        if self.lines.is_empty() {
            cv.text(Txt::new("♪ MATNSIZ · SPEKTR", text::MONO, 20.0, bx, by + 6.0, c8(0x7c8456, 1.0)).weight(500).clip(clip).glow([205, 215, 130, 70], 3.0));
            // no lyrics: the display becomes a dot-matrix spectrum of the real signal (64 x 30 round pixels; unlit
            // dots stay faintly visible behind the glass, a falling peak dot on every column)
            let (vx, vy, vw, vh) = (652.0, 300.0, 700.0 - 30.0, 300.0);
            let (cols, rows) = (64usize, 30usize);
            let (cw, rh) = (vw / cols as f32, vh / rows as f32);
            let dot = cw.min(rh) * 0.36;
            for c in 0..cols {
                let hgt = (spec.cols[c] * rows as f32).round() as usize;
                let pk = spec.peaks[c].round() as usize;
                for r in 0..rows {
                    let lit = r < hgt;
                    let col = if lit {
                        if r as f32 > rows as f32 * 0.82 { rgb(0xeaffb0) } else { rgba(0xb2ce68, 0.55 + 0.45 * r as f32 / rows as f32) }
                    } else if r == pk {
                        rgba(0xd6f08c, 0.8)
                    } else {
                        rgba(0x788c46, 0.07)
                    };
                    let (x, y) = (vx + c as f32 * cw + cw / 2.0, vy + vh - (r as f32 + 0.5) * rh);
                    cv.flat(Quad::flat(x - dot, y - dot, dot * 2.0, dot * 2.0, col).rounded(dot));
                }
                if hgt > 0 {
                    let top = vy + vh - hgt as f32 * rh;
                    cv.light(Quad::glow(vx + c as f32 * cw + cw / 2.0, top + 4.0, 9.0, rgba(0xc4e078, 0.12)));
                }
            }
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
                    .glow([230, 220, 130, if now { 150 } else { 90 }], if now { 3.0 } else { 2.2 });
                if now {
                    tx = tx.wrap(bw).line(a.size * 1.22);
                    // the phosphor glow behind the sung line
                    cv.light(Quad::glow(bx + bw * 0.3, by + a.y + a.size * 0.8, bw * 0.5, rgba(0xc8d27a, 0.05 * a.alpha)));
                } else {
                    tx = tx.wrap(bw * 1.6).line(a.size * 1.25);
                }
                cv.text(tx);
            }
            if i.is_none() {
                cv.text(Txt::new("♪ ♪ ♪", text::MONO, 34.0, bx, by + 104.0, c8(0x6d7450, 1.0)).clip(clip).glow([205, 215, 130, 80], 3.0));
            }
            if let Some(tr) = tr {
                cv.text(Txt::new(tr, text::MONO, 17.0, bx + 2.0, by + 104.0 + hh + 10.0, c8(0xa3a978, 1.0)).clip(clip).glow([190, 200, 120, 90], 3.0));
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
    }
}

/// The lyrics tube's phosphor backplate (the same under every page shown on it, so a page change never changes it).
pub fn tube_back(cv: &mut Canvas) {
    let [x0, y0, x1, y1] = LY;
    let (w, h) = (x1 - x0, y1 - y0);
        // the phosphor backplate: radial(135% 125% at 50% 44%, #20271b, #171d14 42%, #0d110b 72%, #050705) and a
        // faint olive bloom (70% 60% at 42% 46%); the big gradients are cut to the tube by its rounded mask
        cv.mask = Some((LY, 37.0));
        cv.flat(Quad::flat(x0, y0, w, h, rgb(0x050705)));
        let (rx, ry) = (w * 1.35, h * 1.25);
        cv.flat(Quad::radial(x0 + w * 0.5 - rx, y0 + h * 0.44 - ry, rx * 2.0, ry * 2.0, rgb(0x20271b), rgb(0x050705)));
        let (rx, ry) = (w * 0.7 * 0.72, h * 0.6 * 0.72);
        cv.flat(Quad::radial(x0 + w * 0.42 - rx, y0 + h * 0.46 - ry, rx * 2.0, ry * 2.0, rgba(0x96aa5a, 0.075), rgba(0x96aa5a, 0.0)));
        cv.flat(Quad::dots(x0, y0, w, h, 5.0, 1.1, rgba(0x96af5f, 0.06)));
        cv.mask = None;
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

/// The glass in front of the lyrics tube (the Tauri CSS #lyr::after and its rim SVG): it bulges out of the bezel,
/// so its edges look sideways and mirror the room (a pale Fresnel band), the middle stays deep, one curved reflection
/// crosses it, scanlines and an aperture grille in device pixels, a dark gasket, a fine rim and glinting corners.
pub fn lens(over: &mut Canvas, r: [f32; 4], rad: f32) {
    let [x0, y0, x1, y1] = r;
    let (w, h) = (x1 - x0, y1 - y0);
    over.mask = Some((r, rad));
    // the middle stays deep: radial(70% 62% at 48% 52%, rgba(0,0,0,.22), transparent)
    let (rx, ry) = (w * 0.7, h * 0.62);
    over.flat(Quad::radial(x0 + w * 0.48 - rx, y0 + h * 0.52 - ry, rx * 2.0, ry * 2.0, rgba(0, 0.22), rgba(0, 0.0)));
    // scanlines .30, aperture grille .10
    over.flat(Quad::scan2(x0, y0, w, h, 0.30, 0.10));
    // the curved reflection: radial(150% 130% at -14% 122%, transparent 69%, .085 69.8%, .05 82%, .02 94%, transparent)
    over.flat(Quad::arc_light(x0, y0, w, h, [x0 - 0.14 * w, y0 + 1.22 * h], 1.5 * w, 0.0, (1.5 * w) / (1.3 * h), rgba(0xcddedc, 0.7)));
    // the light along the top: linear(180deg, rgba(225,240,255,.07), transparent 12%), inset 0 18px 26px -6px .07
    over.flat(Quad::grad(x0, y0, w, h * 0.12, rgba(0xe1f0ff, 0.07), rgba(0xe1f0ff, 0.0), true));
    over.flat(Quad::grad(x0, y0, w, 40.0, rgba(0xd2e1eb, 0.06), rgba(0xd2e1eb, 0.0), true));
    // the Fresnel band at the walls: inset 0 0 26px 8px rgba(160,182,172,.11); the gasket's shadow: inset 5px .6
    over.flat(Quad::inset(x0, y0, w, h, rad, 34.0, rgba(0xa0b6ac, 0.15)));
    // the bulge: the corners fall away into shadow, a soft reflection of a window high on the left
    over.flat(Quad::radial(x0 + w / 2.0 - w * 0.72, y0 + h / 2.0 - h * 0.74, w * 1.44, h * 1.48, rgba(0, 0.0), rgba(0, 0.3)));
    over.light(Quad::radial(x0 - w * 0.02, y0 - h * 0.12, w * 0.62, h * 0.55, rgba(0xdfeaf2, 0.075), rgba(0xdfeaf2, 0.0)));
    over.light(Quad::radial(x0 + w * 0.06, y0 + h * 0.02, w * 0.22, h * 0.16, rgba(0xffffff, 0.06), rgba(0xffffff, 0.0)));
    over.flat(Quad::rim(x0, y0, w, h, rad, 5.0, rgba(0, 0.6), rgba(0, 0.6)));
    over.mask = None;
    // the glass stands out of the bezel: its shadow falls on the bezel below it
    over.flat(Quad::rim(x0 - 5.0, y0 - 2.0, w + 10.0, h + 9.0, rad + 5.0, 6.0, rgba(0, 0.0), rgba(0, 0.5)));
    // the rim (SVG): a 3 px dark gasket, a black hairline, then the glass edge, bright on top, dim below
    over.flat(Quad::rim(x0, y0, w, h, rad, 3.0, rgb(0x050605), rgb(0x050605)));
    over.flat(Quad::rim(x0 + 3.0, y0 + 3.0, w - 6.0, h - 6.0, rad - 3.0, 1.0, rgba(0, 0.7), rgba(0, 0.7)));
    over.flat(Quad::rim(x0 + 3.8, y0 + 3.8, w - 7.6, h - 7.6, rad - 3.8, 1.3, rgba(0xeef6ff, 0.42), rgba(0xeef6ff, 0.12)));
    // glints where the rim turns the corners: top right (strong), top left, bottom right (faint)
    let saved = over.clip;
    for (cx, cy, reach, a) in [(x1, y0, 70.0, 0.85), (x0, y0, 55.0, 0.5), (x1, y1, 45.0, 0.22)] {
        for (k, f) in [(1.0, 0.25), (0.6, 0.3), (0.3, 0.45)] {
            let d = reach * k;
            over.clip = Some([cx - d, cy - d, cx + d, cy + d]);
            over.flat(Quad::rim(x0 + 3.8, y0 + 3.8, w - 7.6, h - 7.6, rad - 3.8, 1.6, rgba(0xffffff, a * f), rgba(0xffffff, a * f)));
        }
    }
    over.clip = saved;
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

/// Seven-segment digits (and ':') like an LCD clock: `w` x `h` per digit, segment thickness `t`.
const SEGS: [u8; 10] = [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f];
fn seg_width(s: &str, w: f32, h: f32) -> f32 {
    s.chars().map(|c| if c == ':' { h * 0.32 } else { w + h * 0.22 }).sum::<f32>() - h * 0.22
}
#[allow(clippy::too_many_arguments)]
fn seg_text(cv: &mut Canvas, s: &str, mut x: f32, y: f32, w: f32, h: f32, t: f32, ink: u32, a: f32) {
    for ch in s.chars() {
        if ch == ':' {
            for cy in [y + h * 0.3, y + h * 0.7] {
                cv.flat(Quad::flat(x + 1.4, cy - t / 2.0 + 1.4, t, t, rgba(ink, 0.2 * a)).rounded(t / 2.0));
                cv.flat(Quad::flat(x, cy - t / 2.0, t, t, rgba(ink, a)).rounded(t / 2.0));
            }
            x += h * 0.32;
            continue;
        }
        let m = ch.to_digit(10).map(|d| SEGS[d as usize]).unwrap_or(0);
        let hh = h / 2.0;
        // a, b, c, d, e, f, g: each a pill with a hair of gap at the joints
        let r = [
            [x + t * 0.7, y, w - t * 1.4, t],
            [x + w - t, y + t * 0.7, t, hh - t * 1.0],
            [x + w - t, y + hh + t * 0.3, t, hh - t * 1.0],
            [x + t * 0.7, y + h - t, w - t * 1.4, t],
            [x, y + hh + t * 0.3, t, hh - t * 1.0],
            [x, y + t * 0.7, t, hh - t * 1.0],
            [x + t * 0.7, y + hh - t / 2.0, w - t * 1.4, t],
        ];
        for (k, q) in r.iter().enumerate() {
            let on = m & (1 << k) != 0;
            if on {
                cv.flat(Quad::flat(q[0] + 1.4, q[1] + 1.4, q[2], q[3], rgba(ink, 0.2 * a)).rounded(t / 2.0));
            }
            cv.flat(Quad::flat(q[0], q[1], q[2], q[3], rgba(ink, if on { a } else { 0.07 })).rounded(t / 2.0));
        }
        x += w + h * 0.22;
    }
}

/// `ghost`: O'Z TANLOVIM, the cursor over the waveform: where a click would jump (a faint head and its time).
pub fn draw_lcd(cv: &mut Canvas, wave: &[f32], line1: &str, foot: &str, pos: f64, dur: f64, ghost: Option<f32>) {
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
    // depth: the glass sits in a well — a pixel grid in the liquid crystal, the backlight brighter in the middle
    // and falling off to the edges, the bezel's shadow inside the rim
    cv.mask = Some((clip, 14.0));
    cv.flat(Quad::dots(ox, oy, lw, 152.0, 3.0, 0.75, rgba(ink, 0.05)));
    let (rx, ry) = (lw * 0.75, 152.0 * 0.95);
    cv.flat(Quad::radial(ox + lw / 2.0 - rx, oy + 70.0 - ry, rx * 2.0, ry * 2.0, rgba(0xf4ffd8, 0.1), rgba(0x1c2614, 0.2)));
    cv.flat(Quad::inset(ox, oy, lw, 152.0, 14.0, 16.0, rgba(0x101a0a, 0.4)));
    cv.flat(Quad::grad(ox, oy, lw, 14.0, rgba(0x0b1207, 0.35), rgba(0x0b1207, 0.0), true));
    cv.mask = None;
    // the top row: the song (title bold, artist lighter) on the left, the clock on the right, both on one line
    let (title, artist) = line1.trim_start_matches(['♫', ' ']).split_once(" · ").unwrap_or((line1, ""));
    let mut t1 = Txt::new(format!("♫ {title}"), text::MR, 18.0, ox + 28.0, oy + 16.0, c8(ink, 0.8)).weight(600);
    t1.spans.push(text::Span { text: format!("   {artist}"), color: c8(ink, 0.48), weight: 500 });
    lcd_text(cv, t1.clip([ox, oy, ox + lw - 250.0, oy + 152.0]));
    // the clock: seven-segment digits (the unlit segments faintly visible, like a real LCD), the length after it
    let total = fmt(dur);
    let tw = seg_width(&total, 9.0, 16.0);
    seg_text(cv, &total, ox + lw - 28.0 - tw, oy + 22.0, 9.0, 16.0, 2.2, ink, 0.4);
    cv.flat(Quad::flat(ox + lw - 28.0 - tw - 14.0, oy + 21.0, 2.0, 18.0, rgba(ink, 0.35)).rotated([ox + lw - 28.0 - tw - 13.0, oy + 30.0], 0.35));
    let cur = fmt(pos);
    let cw = seg_width(&cur, 18.0, 30.0);
    seg_text(cv, &cur, ox + lw - 28.0 - tw - 28.0 - cw, oy + 10.0, 18.0, 30.0, 4.0, ink, 0.82);
    let frac = if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let n = wave.len().max(1) as f32;
    let wc = oy + 82.0;
    for (i, h) in wave.iter().enumerate() {
        let h = h * 0.78;
        let x = ox + 27.0 + i as f32 * (wv / n);
        let on = (i as f32 / n) < frac;
        cv.flat(Quad::flat(x + 1.6, wc - h / 2.0 + 1.6, wv / n * 0.62, h, rgba(ink, if on { 0.2 } else { 0.0 })));
        cv.flat(Quad::flat(x, wc - h / 2.0, wv / n * 0.62, h, rgba(ink, if on { 0.82 } else { 0.13 })));
    }
    cv.flat(Quad::flat(ox + 27.0 + frac * wv, wc - 25.0, 3.0, 50.0, rgb(ink)));
    if let (Some(g), true) = (ghost, dur > 0.0) {
        let gx = ox + 27.0 + g.clamp(0.0, 1.0) * wv;
        cv.flat(Quad::flat(gx, wc - 25.0, 2.0, 50.0, rgba(ink, 0.45)));
        let tx = Txt::new(fmt(g as f64 * dur), text::BAR, 15.0, gx + 6.0, wc - 26.0, c8(ink, 0.85)).weight(600);
        cv.text(if gx > ox + lw - 120.0 { Txt { x: gx - 6.0, ..tx }.align(Align::Right) } else { tx });
    }
    cv.flat(Quad::flat(ox + 27.0, oy + 117.0, wv, 1.0, rgba(ink, 0.18)));
    lcd_text(cv, Txt::new(foot, text::BAR, 15.0, ox + 29.0, oy + 125.0, c8(ink, 0.6)).weight(500).clip(clip));
}

/* ───────── the tuner drum (#drum: 258,866 500x92 — a brushed graphite cylinder with a printed scale) */
pub fn draw_drum(cv: &mut Canvas, over: &mut Canvas, angle_deg: f32, light: bool) {
    let (ox, oy, w, h, r) = (258.0f32, 866.0f32, 494.0f32, 92.0f32, 239.0f32);
    let win = ([ox, oy, ox + w, oy + h], 18.0);
    cv.mask = Some(win);
    over.mask = Some(win);
    // body: the cylinder's shading across its width
    let st = if light { [0x5d636c, 0x9aa0a9, 0xd9dde2, 0xf4f5f7, 0xc9ced5, 0x8a9099, 0x4f545c] } else { [0x0c0d0f, 0x25282d, 0x40444b, 0x565b63, 0x3a3e45, 0x212328, 0x0b0c0e] };
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
    let bs = if light { [(0.0, 0.0), (0.12, 0.85), (0.45, 0.95), (0.88, 0.85), (1.0, 0.0)] } else { [(0.0, 0.0), (0.12, 0.75), (0.45, 0.8), (0.88, 0.75), (1.0, 0.0)] };
    let band = if light { 0xf0ebda } else { 0x16171a };
    for k in 0..4 {
        cv.flat(Quad::grad(ox + w * bs[k].0, oy + 12.0, w * (bs[k + 1].0 - bs[k].0) + 0.5, h - 24.0, rgba(band, bs[k].1), rgba(band, bs[k + 1].1), false));
    }
    cv.flat(Quad::flat(ox, oy + 12.0, w, 1.0, [1.0, 1.0, 1.0, 0.08]));
    cv.flat(Quad::flat(ox, oy + h - 13.0, w, 1.0, [1.0, 1.0, 1.0, 0.08]));
    // the scale: ticks and numbers squeezed by the curvature, fading as they turn away
    let ink = if light { 0x2a2c30 } else { 0xe8e2d0 };
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
    let dk = if light { 0.45 } else { 1.0 };
    over.flat(Quad::grad(ox, oy, w * 0.14, h, rgba(0, 0.75 * dk), rgba(0, 0.0), false));
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
    cv.mask = None;
    over.mask = None;
}

/* ───────── the trivision (#tv: 156,196 436x432): 15 three-sided prisms, a wave of 120° turns */
pub struct Trivision {
    /// the cover printed on each of the three faces
    faces: [Option<usize>; 3],
    /// how many 120° turns the slats have made (the face showing is step % 3)
    step: i32,
    /// the turn in progress started here
    start: Option<Instant>,
}

pub const TV: [f32; 4] = [156.0, 196.0, 592.0, 628.0];
const NS: usize = 15;

/// CSS cubic-bezier(x1, y1, x2, y2) at time `t` (0..1): Newton steps on x, then y.
pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let f = |a: f32, b: f32, u: f32| 3.0 * a * u * (1.0 - u) * (1.0 - u) + 3.0 * b * u * u * (1.0 - u) + u * u * u;
    let df = |a: f32, b: f32, u: f32| 3.0 * a * (1.0 - u) * (1.0 - u) + 6.0 * (b - a) * u * (1.0 - u) + 3.0 * (1.0 - b) * u * u;
    let mut u = t;
    for _ in 0..8 {
        let d = df(x1, x2, u);
        if d.abs() < 1e-5 {
            break;
        }
        u = (u - (f(x1, x2, u) - t) / d).clamp(0.0, 1.0);
    }
    f(y1, y2, u)
}

/// The printed face bulges a little (CSS .face::after): a dark edge, a bright left rim, a soft specular strip down the
/// middle, falling into shadow on the right. `u` 0..1 across the face -> a brightness change.
fn bulge(u: f32) -> f32 {
    const ST: [(f32, f32); 8] = [(0.0, -0.42), (0.05, 0.16), (0.16, 0.05), (0.36, 0.13), (0.5, 0.02), (0.62, 0.0), (0.86, -0.18), (1.0, -0.45)];
    for k in 1..ST.len() {
        if u <= ST[k].0 {
            let (a, b) = (ST[k - 1], ST[k]);
            return a.1 + (b.1 - a.1) * (u - a.0) / (b.0 - a.0);
        }
    }
    -0.45
}

impl Trivision {
    pub fn new() -> Self {
        Trivision { faces: [None; 3], step: 0, start: None }
    }
    /// A new cover: printed on the face that comes next, then the slats turn to it in a wave (the first one is just there).
    pub fn show(&mut self, tex: usize) {
        if self.faces.iter().all(|f| f.is_none()) {
            self.faces[0] = Some(tex);
            return;
        }
        if self.faces[(self.step.rem_euclid(3)) as usize] == Some(tex) && self.start.is_none() {
            return;
        }
        // a turn still running: it ends at once
        if self.start.take().is_some() {
            self.step += 1;
        }
        self.faces[((self.step + 1).rem_euclid(3)) as usize] = Some(tex);
        self.start = Some(Instant::now());
    }

    pub fn draw(&mut self, cv: &mut Canvas) {
        let [x0, y0, x1, y1] = TV;
        let (w, h) = (x1 - x0, y1 - y0);
        let sw = w / NS as f32;
        // apothem of the prism (a face is sw wide, the cross-section an equilateral triangle)
        let ap = sw / (2.0 * 3f32.sqrt());
        cv.mask = Some((TV, 14.0));
        cv.flat(Quad::flat(x0, y0, w, h, rgb(0x0c0d0f)));
        let el = self.start.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0);
        let mut done = self.start.is_some();
        let (cx_tv, cy) = (x0 + w / 2.0, y0 + h / 2.0);
        // a little perspective (the eye ~1500 px in front): a turning face comes forward and grows
        let eye = 1500.0;
        for i in 0..NS {
            // each slat starts 38 ms after its neighbour, turns in 0.62 s, cubic-bezier(.34, 1.56, .64, 1) (overshoots)
            let p = if self.start.is_some() { ((el - i as f32 * 0.038) / 0.62).clamp(0.0, 1.0) } else { 0.0 };
            if p < 1.0 {
                done = false;
            }
            let e = if self.start.is_some() { bezier(0.34, 1.56, 0.64, 1.0, p) } else { 0.0 };
            let rot = -((self.step as f32 + e) * 120.0).to_radians();
            let sx = x0 + (i as f32 + 0.5) * sw;
            for k in 0..3 {
                // face k's normal points at angle phi (0 = at the viewer)
                let phi = rot + (k as f32 * 120.0).to_radians();
                let facing = phi.cos();
                if facing <= 0.02 {
                    continue;
                }
                // light from the upper left front: a face turned to the right darkens
                let light = 0.55 + 0.45 * facing + 0.12 * (-phi.sin()).max(0.0) - 0.1 * phi.sin().max(0.0);
                let strips = 8;
                for j in 0..strips {
                    let (s0, s1) = (j as f32 / strips as f32, (j + 1) as f32 / strips as f32);
                    // a point across the face (-sw/2..sw/2) -> x and depth
                    let at = |u: f32| {
                        let d = (u - 0.5) * sw;
                        let x = sx + ap * phi.sin() + d * phi.cos();
                        let z = ap * phi.cos() - d * phi.sin();
                        let k = eye / (eye - z);
                        (cx_tv + (x - cx_tv) * k, k)
                    };
                    let ((xa, ka), (xb, kb)) = (at(s0), at(s1));
                    if xb - xa < 0.05 {
                        continue;
                    }
                    let kk = (ka + kb) * 0.5;
                    let hh = h * kk;
                    let b = (light * (1.0 + bulge((s0 + s1) * 0.5) * 0.8)).max(0.0);
                    // a dark gap between slats: the face's own edges are inset a little
                    let (ia, ib) = (if j == 0 { 1.3 } else { 0.0 }, if j == strips - 1 { 1.3 } else { 0.0 });
                    let rect = [xa + ia * facing, cy - hh / 2.0, (xb - xa - (ia + ib) * facing).max(0.0), hh];
                    match self.faces[k] {
                        Some(tex) => {
                            let u0 = (i as f32 + s0) / NS as f32;
                            let q = Quad { rect, uv: [u0, 0.0, (s1 - s0) / NS as f32, 1.0], color: [b, b, b, 1.0], ..Quad::full() };
                            cv.quad(Some(tex), q);
                        }
                        None => cv.flat(Quad { rect, ..Quad::flat(0.0, 0.0, 0.0, 0.0, [0.086 * b, 0.09 * b, 0.1 * b, 1.0]) }),
                    }
                }
            }
        }
        if done {
            self.step += 1;
            self.start = None;
        }
        // deep inside the device: shadow under the housing lips and at the side walls
        cv.flat(Quad::grad(x0, y0, w, 34.0, rgba(0, 0.88), rgba(0, 0.0), true));
        cv.flat(Quad::grad(x0, y1 - 28.0, w, 28.0, rgba(0, 0.0), rgba(0, 0.82), true));
        cv.flat(Quad::grad(x0, y0, 22.0, h, rgba(0, 0.65), rgba(0, 0.0), false));
        cv.flat(Quad::grad(x1 - 22.0, y0, 22.0, h, rgba(0, 0.0), rgba(0, 0.65), false));
        cv.mask = None;
    }
}

/// Static over the lyrics tube while a station is being found: snow, a rolling bar, torn lines, the station's name
/// flickering in it. `amt` 0..1 (it fades in and out), `t` = seconds.
pub fn snow(over: &mut Canvas, amt: f32, t: f32, station: &str) {
    let [x0, y0, x1, y1] = LY;
    let (w, h) = (x1 - x0, y1 - y0);
    over.mask = Some((LY, 37.0));
    over.flat(Quad::flat(x0, y0, w, h, rgba(0x0a0f08, 0.6 * amt)));
    let mut s: u32 = ((t * 30.0) as u32).wrapping_mul(2_654_435_761) | 1;
    let mut r = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s as f32 / u32::MAX as f32
    };
    for _ in 0..900 {
        let (x, y, l, a) = (x0 + r() * w, y0 + r() * h, 1.5 + r() * 6.0, r());
        over.light(Quad::flat(x, y, l, 2.0, rgba(0xd8f0a8, a * a * 0.55 * amt)));
    }
    // the rolling bar of a picture that has not locked
    let by = y0 - 90.0 + (t * 0.7).fract() * (h + 90.0);
    over.light(Quad::grad(x0, by, w, 90.0, rgba(0xe6ff9a, 0.0), rgba(0xe6ff9a, 0.09 * amt), true));
    for _ in 0..4 {
        let y = y0 + r() * h;
        over.light(Quad::flat(x0 + r() * w * 0.3, y, w * (0.4 + r() * 0.6), 1.0 + r() * 2.0, rgba(0xe6ff9a, 0.16 * amt)));
    }
    over.mask = None;
    let fl = 0.88 + 0.12 * (t * 23.0).sin() * (t * 7.0).sin();
    let a = (amt * fl * 255.0) as u8;
    let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
    over.text(Txt::new(station, text::VT, 46.0, cx, cy - 44.0, [230, 255, 154, a]).align(Align::Center).glow([230, 255, 154, a / 3], 3.0));
    let dots = ".".repeat(1 + (t * 3.0) as usize % 3);
    over.text(Txt::new(format!("SIGNAL QIDIRILMOQDA{dots:<3}"), text::VT, 26.0, cx, cy + 12.0, [196, 224, 120, (a as f32 * 0.85) as u8]).align(Align::Center));
}
