//! The TAVSIF chart recorder: one continuous tensioned paper tape from the head's slot to the take-up roller.
//! A new receipt: the roller pulls the tape down (stepper steps), then the head burns it line by line.

use crate::gfx::Quad;
use crate::net::Track;
use crate::scene::{rgb, rgba, Canvas};
use crate::text::{self, Align, Txt};
use std::time::Instant;

pub const WIN: [f32; 4] = [1478.0, 104.0, 1836.0, 536.0];
const INK: u32 = 0x161514;

fn c8(hex: u32, a: f32) -> [u8; 4] {
    [((hex >> 16) & 255) as u8, ((hex >> 8) & 255) as u8, (hex & 255) as u8, (a.clamp(0.0, 1.0) * 255.0) as u8]
}

enum Item {
    Text(Txt),
    Rule,
    Energy(u32),
    Barcode(u32),
}

struct Receipt {
    items: Vec<(f32, Item)>,
    height: f32,
    /// when the head started burning it (None = not yet: the tape is still feeding)
    burn: Option<Instant>,
}

pub struct Tape {
    rolls: Vec<Receipt>,
    /// px of tape between the newest receipt's top and the window's top (feeding) + scroll
    pub scroll: f32,
    /// where the mouse wheel wants the tape (the tape eases there)
    pub goal: f32,
    feed: Option<(Instant, f32)>,
    last: Instant,
}

fn layout(t: &Track, tl: &mut text::TextLayer) -> Receipt {
    let w = WIN[2] - WIN[0] - 36.0;
    let x = WIN[0] + 18.0;
    let mut y = 30.0;
    let mut items = vec![];
    let mut put = |items: &mut Vec<(f32, Item)>, y: &mut f32, t: Txt, gap: f32| {
        let (_, h) = tl.measure2(&t);
        items.push((*y, Item::Text(t)));
        *y += h + gap;
    };
    let cx = x + w / 2.0;
    put(&mut items, &mut y, Txt::new("★ KAWAII RADIO ★", text::DELA, 17.0, cx, 0.0, c8(INK, 1.0)).align(Align::Center), 2.0);
    put(&mut items, &mut y, Txt::new("KR-01 · QO'SHIQ TAVSIFI", text::MONO, 12.0, cx, 0.0, c8(0x4a443e, 1.0)).align(Align::Center), 10.0);
    items.push((y, Item::Rule));
    y += 14.0;
    put(&mut items, &mut y, Txt::new(t.title.clone(), text::MONO, 24.0, x, 0.0, c8(INK, 1.0)).weight(700).wrap(w).line(28.0), 2.0);
    put(&mut items, &mut y, Txt::new(t.artist.clone(), text::MONO, 17.0, x, 0.0, c8(0x2a2521, 1.0)).weight(500).wrap(w), 2.0);
    let d = t.duration.max(0.0) as u64;
    put(&mut items, &mut y, Txt::new(format!("{} · {}:{:02} · {} BPM", t.category, d / 60, d % 60, t.bpm.round()), text::MONO, 15.0, x, 0.0, c8(INK, 1.0)), 6.0);
    // ENERGIYA ▮▮▮▮▮▮▮▮▯▯▯▯ 70%: the label, twelve cells, the number after them
    items.push((y, Item::Energy((t.energy / 100.0 * 12.0).round() as u32)));
    items.push((y, Item::Text(Txt::new(format!("{}%", t.energy.round()), text::MONO, 12.0, x + 82.0 + 12.0 * 15.0 + 4.0, 0.0, c8(0x3a3530, 1.0)))));
    put(&mut items, &mut y, Txt::new("ENERGIYA", text::MONO, 12.0, x, 0.0, c8(0x3a3530, 1.0)), 10.0);
    items.push((y, Item::Rule));
    y += 14.0;
    let info = t.info.clone().unwrap_or_default();
    let desc = info.get("description").and_then(|v| v.as_str()).unwrap_or("Bu qo'shiq uchun tavsif hali yo'q.").to_string();
    put(&mut items, &mut y, Txt::new(desc, text::MONO, 15.0, x, 0.0, c8(INK, 1.0)).weight(500).wrap(w).line(22.5), 8.0);
    if let Some(ds) = info.get("details").and_then(|v| v.as_array()) {
        for s in ds {
            let ti = s.get("title").and_then(|v| v.as_str()).unwrap_or("").to_uppercase();
            let tx = s.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            y += 6.0;
            put(&mut items, &mut y, Txt::new(ti, text::MONO, 12.0, x, 0.0, c8(0x2a2522, 1.0)).weight(700), 2.0);
            put(&mut items, &mut y, Txt::new(tx, text::MONO, 15.0, x, 0.0, c8(INK, 1.0)).weight(500).wrap(w).line(22.5), 4.0);
        }
    }
    items.push((y, Item::Rule));
    y += 14.0;
    let h: u32 = t.id.chars().fold(0u32, |a, c| a.wrapping_mul(31).wrapping_add(c as u32));
    put(&mut items, &mut y, Txt::new(format!("#{:04}", h % 10000), text::MONO, 12.0, x, 0.0, c8(0x4a443e, 1.0)), 8.0);
    items.push((y, Item::Barcode(h)));
    y += 40.0;
    Receipt { items, height: y.max(WIN[3] - WIN[1]), burn: None }
}

impl Tape {
    pub fn new() -> Self {
        Tape { rolls: vec![], scroll: 0.0, goal: 0.0, feed: None, last: Instant::now() }
    }

    /// Print a receipt for this track (`animate` = feed + burn, else it is just there).
    pub fn print(&mut self, t: &Track, tl: &mut text::TextLayer, animate: bool) {
        let mut r = layout(t, tl);
        let h = r.height;
        if animate && !self.rolls.is_empty() {
            self.feed = Some((Instant::now(), h));
        } else {
            r.burn = Some(Instant::now() - std::time::Duration::from_secs(60));
        }
        // a print still running finishes at once
        if let Some(r0) = self.rolls.first_mut() {
            if r0.burn.is_none_or(|b| b.elapsed().as_secs_f32() < 30.0) {
                r0.burn = Some(Instant::now() - std::time::Duration::from_secs(60));
            }
        }
        self.rolls.insert(0, r);
        self.rolls.truncate(12);
        self.scroll = 0.0;
        self.goal = 0.0;
    }

    /// The head is burning (or the roller pulling): the printer's lamp is on.
    pub fn busy(&self) -> bool {
        self.feed.is_some() || self.rolls.first().is_some_and(|r| r.burn.is_some_and(|b| b.elapsed().as_secs_f32() / 0.08 * 21.0 < r.height))
    }

    pub fn draw(&mut self, cv: &mut Canvas, over: &mut Canvas) -> (usize, usize) {
        let [x0, y0, x1, y1] = WIN;
        // the take-up roller's stepper pulls the tape down: 3 px steps, 7 ms each
        let mut feed_off = 0.0;
        if let Some((t0, h)) = self.feed {
            let steps = (t0.elapsed().as_secs_f32() / 0.007) as i32;
            feed_off = (h - steps as f32 * 3.0).max(0.0);
            if feed_off <= 0.0 {
                self.feed = None;
                if let Some(r) = self.rolls.first_mut() {
                    r.burn = Some(Instant::now());
                }
            }
        }
        let dt = self.last.elapsed().as_secs_f32().min(0.1);
        self.last = Instant::now();
        if self.feed.is_none() {
            self.scroll += (self.goal - self.scroll) * (1.0 - (-dt / 0.09).exp());
        }
        cv.clip = Some(WIN);
        cv.flat(Quad::flat(x0, y0, x1 - x0, y1 - y0, rgb(0xf3f1ea)));
        let mut y = y0 - feed_off - self.scroll;
        let total: f32 = self.rolls.iter().map(|r| r.height).sum();
        for r in &self.rolls {
            let top = y;
            // what the head has burnt so far: one text line (21 px) every 80 ms
            let rev = match r.burn {
                Some(b) => b.elapsed().as_secs_f32() / 0.08 * 21.0,
                None => -1.0,
            };
            if top + r.height > y0 && top < y1 {
                // the head burns row by row: everything below it is still blank paper; the paper buzzes a little
                let burning = rev >= 0.0 && rev < r.height;
                let cut = if burning { (top + rev).clamp(y0, y1) } else { y1 };
                let jx = if burning { ((rev * 7.0) as i32 % 2) as f32 * 0.4 } else { 0.0 };
                cv.clip = Some([x0, y0, x1, cut]);
                for (iy, it) in &r.items {
                    if *iy > rev {
                        continue;
                    }
                    let yy = top + iy;
                    match it {
                        Item::Text(t) => {
                            let mut t = t.clone();
                            t.y = yy;
                            t.x += jx;
                            t.clip = [x0, y0, x1, cut];
                            cv.text(t);
                        }
                        Item::Rule => {
                            let mut k = x0 + 18.0;
                            while k < x1 - 18.0 {
                                cv.flat(Quad::flat(k, yy + 6.0, 6.0, 1.5, rgba(0x9a9a9c, 1.0)));
                                k += 10.0;
                            }
                        }
                        Item::Energy(n) => {
                            for k in 0..12 {
                                cv.flat(Quad::flat(x0 + 18.0 + 82.0 + k as f32 * 15.0, yy + 2.0, 12.0, 12.0, rgb(if k < *n { 0x3a3530 } else { 0xc9c6be })));
                            }
                        }
                        Item::Barcode(h) => {
                            let mut k = x0 + 18.0;
                            let mut s = *h;
                            while k < x1 - 18.0 {
                                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                                let bw = [1.6, 1.6, 3.2, 4.8][((s >> 16) % 4) as usize];
                                cv.flat(Quad::flat(k, yy, bw, 26.0, rgb(0x2a2622)));
                                k += bw + 1.6;
                            }
                        }
                    }
                }
                cv.clip = Some(WIN);
                // the line just burnt is paler for a moment (thermal paper develops)
                if burning {
                    cv.flat(Quad::grad(x0, top + rev - 26.0, x1 - x0, 26.0, rgba(0xf3f1ea, 0.0), rgba(0xf3f1ea, 0.55), true));
                }
            }
            y += r.height;
            if r.height > 0.0 {
                cv.flat(Quad::flat(x0 + 18.0, y - 1.0, x1 - x0 - 36.0, 1.0, rgba(0x3c3832, 0.18)));
            }
        }
        cv.clip = None;
        let max = (total - (y1 - y0)).max(0.0);
        self.goal = self.goal.clamp(0.0, max);
        self.scroll = self.scroll.clamp(0.0, max);
        // the platen bows the paper: darker at both edges, a soft sheen; shadow under the head and over the roller
        over.flat(Quad::grad(x0, y0, 30.0, y1 - y0, rgba(0, 0.14), rgba(0, 0.0), false));
        over.flat(Quad::grad(x1 - 30.0, y0, 30.0, y1 - y0, rgba(0, 0.0), rgba(0, 0.16), false));
        over.flat(Quad::grad(x0, y0, x1 - x0, 30.0, rgba(0, 0.45), rgba(0, 0.0), true));
        over.flat(Quad::grad(x0, y1 - 26.0, x1 - x0, 26.0, rgba(0, 0.0), rgba(0, 0.5), true));
        let lines = (total / 21.0).max(1.0) as usize;
        ((self.scroll / 21.0) as usize + 1, lines)
    }
}
