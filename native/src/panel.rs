//! The modules that slide over the lyrics display: QIDIRUV (search), KUTUBXONA (library), PROFIL, SOZLAMALAR.
//! Everything is drawn in the lyrics tube; clickable rows are registered as hit zones for this frame.

use crate::gfx::Quad;
use crate::net::Track;
use crate::scene::{rgb, rgba, Canvas, LY};
use crate::text::{self, Align, Txt};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen {
    Home,
    Search,
    Library,
    Profile,
    Settings,
}

#[derive(Clone, Debug)]
pub enum Act {
    Close,
    Play(String),
    OpenList(String),
    Back,
    Login,
    Logout,
    Size(&'static str),
    Full,
    Update,
    FocusSearch,
}

pub struct Panel {
    pub screen: Screen,
    pub query: String,
    pub scroll: f32,
    /// the open list in KUTUBXONA: "liked" or a playlist id
    pub open: Option<String>,
    pub hits: Vec<([f32; 4], Act)>,
}

fn c8(hex: u32, a: f32) -> [u8; 4] {
    [((hex >> 16) & 255) as u8, ((hex >> 8) & 255) as u8, (hex & 255) as u8, (a.clamp(0.0, 1.0) * 255.0) as u8]
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| !matches!(c, '\'' | '`' | '\u{2bb}' | '\u{2bc}' | '\u{2019}')).collect()
}

pub struct Ctx<'a> {
    pub tracks: &'a HashMap<String, Track>,
    pub liked: &'a [String],
    pub playlists: &'a [(String, String, Vec<String>)],
    pub acc: Option<&'a serde_json::Value>,
    pub now: Option<&'a str>,
    pub version: &'a str,
    pub update: Option<&'a str>,
    pub search_focus: bool,
    pub blink: bool,
    pub minutes: u64,
    pub songs: u64,
}

impl Panel {
    pub fn new() -> Self {
        Panel { screen: Screen::Home, query: String::new(), scroll: 0.0, open: None, hits: vec![] }
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<Act> {
        self.hits.iter().rev().find(|(r, _)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]).map(|(_, a)| a.clone())
    }

    /// The songs the panel lists now (search results, or the open list in the library).
    pub fn list(&self, cx: &Ctx) -> Vec<Track> {
        match self.screen {
            Screen::Search => {
                let q = norm(self.query.trim());
                let mut v: Vec<Track> = cx.tracks.values().filter(|t| q.is_empty() || norm(&t.title).contains(&q) || norm(&t.artist).contains(&q)).cloned().collect();
                v.sort_by(|a, b| a.artist.cmp(&b.artist).then(a.title.cmp(&b.title)));
                v.truncate(120);
                v
            }
            Screen::Library => match self.open.as_deref() {
                Some("liked") => cx.liked.iter().rev().filter_map(|id| cx.tracks.get(id).cloned()).collect(),
                Some(p) => cx.playlists.iter().find(|x| x.0 == p).map(|x| x.2.iter().filter_map(|id| cx.tracks.get(id).cloned()).collect()).unwrap_or_default(),
                None => vec![],
            },
            _ => vec![],
        }
    }

    pub fn draw(&mut self, cv: &mut Canvas, over: &mut Canvas, tl: &mut text::TextLayer, cx: &Ctx, img: &mut dyn FnMut(&str) -> Option<usize>) {
        self.hits.clear();
        if self.screen == Screen::Home {
            return;
        }
        let [x0, y0, x1, y1] = LY;
        let (w, h) = (x1 - x0, y1 - y0);
        cv.flat(Quad::flat(x0, y0, w, h, rgb(0x0b0e09)).rounded(37.0));
        cv.light(Quad::glow(x0 + w * 0.46, y0 + h * 0.48, w * 0.62, rgba(0x2b3524, 0.5)));
        let px = x0 + 34.0;
        let (title, sub) = match self.screen {
            Screen::Search => ("QIDIRUV", format!("{} ta qo'shiq efirda", cx.tracks.len())),
            Screen::Library => ("KUTUBXONA", format!("{} ta sevimli · {} ta pleylist", cx.liked.len(), cx.playlists.len())),
            Screen::Profile => ("PROFIL", "KR-01 DESK".into()),
            Screen::Settings => ("SOZLAMALAR", format!("KR-01 DESK · v{}", cx.version)),
            Screen::Home => unreachable!(),
        };
        cv.text(Txt::new(title, text::VT, 34.0, px, y0 + 16.0, c8(0xe6ff9a, 1.0)));
        let tw = tl.measure(&Txt::new(title, text::VT, 34.0, 0.0, 0.0, [0; 4]));
        cv.text(Txt::new(sub, text::MONO, 13.0, px + tw + 16.0, y0 + 30.0, c8(0x8f9a6a, 1.0)));
        cv.text(Txt::new("[ESC]  X", text::VT, 22.0, x1 - 40.0, y0 + 22.0, c8(0xc4e078, 1.0)).align(Align::Right));
        self.hits.push(([x1 - 120.0, y0 + 10.0, x1 - 20.0, y0 + 56.0], Act::Close));
        cv.flat(Quad::flat(px, y0 + 62.0, w - 68.0, 1.0, rgba(0xc4e078, 0.25)));
        let body = [x0 + 20.0, y0 + 70.0, x1 - 20.0, y1 - 14.0];
        cv.clip = Some(body);
        let mut y = body[1] + 8.0 - self.scroll;
        let row_h = 58.0;
        let mut hits = std::mem::take(&mut self.hits);
        match self.screen {
            Screen::Search => {
                // the search field
                let f = [px, body[1] + 4.0 - self.scroll, x1 - 54.0, body[1] + 44.0 - self.scroll];
                cv.flat(Quad::flat(f[0], f[1], f[2] - f[0], f[3] - f[1], rgba(0xc4e078, if cx.search_focus { 0.12 } else { 0.06 })).rounded(6.0));
                let q = if self.query.is_empty() && !cx.search_focus { "QO'SHIQ YOKI IJROCHI".to_string() } else { self.query.clone() };
                let qt = Txt::new(q.clone(), text::VT, 24.0, f[0] + 14.0, f[1] + 8.0, c8(0xe6ff9a, if self.query.is_empty() { 0.45 } else { 1.0 })).clip(body);
                let qw = if self.query.is_empty() { 0.0 } else { tl.measure(&qt) };
                cv.text(qt);
                if cx.search_focus && cx.blink {
                    cv.flat(Quad::flat(f[0] + 16.0 + qw, f[1] + 10.0, 10.0, 20.0, rgba(0xe6ff9a, 0.85)));
                }
                hits.push((f, Act::FocusSearch));
                y += 58.0;
                let list = self.list(cx);
                rows(cv, list, &mut y, &mut hits, "Hech narsa topilmadi.", cx, img, (px, w, x1, body, row_h));
            }
            Screen::Library => {
                if self.open.is_some() {
                    cv.text(Txt::new("‹ ORQAGA", text::VT, 22.0, px, y, c8(0xc4e078, 1.0)).clip(body));
                    hits.push(([px - 8.0, y - 4.0, px + 140.0, y + 28.0], Act::Back));
                    y += 38.0;
                    let list = self.list(cx);
                    rows(cv, list, &mut y, &mut hits, "Bu ro'yxat bo'sh.", cx, img, (px, w, x1, body, row_h));
                } else {
                    let mut items: Vec<(String, String, Vec<String>)> = vec![("liked".into(), "♥ SEVIMLILAR".into(), cx.liked.iter().rev().cloned().collect())];
                    items.extend(cx.playlists.iter().cloned());
                    for (id, name, ids) in items {
                        let cover = ids.iter().filter_map(|i| cx.tracks.get(i)).next().and_then(|t| img(&crate::net::url(&t.cover_art)));
                        match cover {
                            Some(c) => cv.quad(Some(c), Quad { rect: [px, y, 52.0, 52.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 6.0, ..Quad::full() }),
                            None => cv.flat(Quad::flat(px, y, 52.0, 52.0, rgb(0x1d2a20)).rounded(6.0)),
                        }
                        cv.text(Txt::new(name, text::MONO, 17.0, px + 66.0, y + 6.0, c8(0xe6ff9a, 1.0)).weight(700).clip(body));
                        cv.text(Txt::new(format!("{} ta qo'shiq", ids.len()), text::MONO, 13.0, px + 66.0, y + 30.0, c8(0x8f9a6a, 1.0)).clip(body));
                        cv.text(Txt::new("›", text::VT, 30.0, x1 - 50.0, y + 8.0, c8(0x9fb070, 1.0)).clip(body));
                        hits.push(([px - 8.0, y - 4.0, x1 - 30.0, y + 58.0], Act::OpenList(id)));
                        y += 66.0;
                    }
                    if cx.acc.is_none() {
                        cv.text(Txt::new("Telefondagi pleylistlar va sevimlilar Kawaii ID bilan kirgandan keyin shu yerda chiqadi.", text::MONO, 14.0, px, y + 6.0, c8(0x8f9a6a, 1.0)).wrap(w - 90.0).clip(body));
                        y += 50.0;
                    }
                }
            }
            Screen::Profile => {
                match cx.acc {
                    Some(a) => {
                        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let n = |k: &str| a.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let av = a.get("avatar").and_then(|v| v.as_str()).and_then(|u| img(u));
                        match av {
                            Some(t) => cv.quad(Some(t), Quad { rect: [px, y, 104.0, 104.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 16.0, ..Quad::full() }),
                            None => cv.flat(Quad::flat(px, y, 104.0, 104.0, rgb(0x24322a)).rounded(16.0)),
                        }
                        let tx = px + 124.0;
                        let name = if s("name").is_empty() { s("username") } else { s("name") };
                        cv.text(Txt::new(name, text::MONO, 24.0, tx, y + 6.0, c8(0xe6ff9a, 1.0)).weight(700).clip(body));
                        cv.text(Txt::new(format!("@{}", s("username")), text::MONO, 14.0, tx, y + 40.0, c8(0x8f9a6a, 1.0)).clip(body));
                        cv.text(Txt::new(format!("LVL {}", n("level") as i64), text::MONO, 15.0, tx, y + 68.0, c8(0xe6ff9a, 1.0)).weight(700).clip(body));
                        let (into, need) = (n("xpInto"), n("xpNeeded"));
                        if need > 0.0 {
                            let bw = 300.0;
                            cv.flat(Quad::flat(tx + 72.0, y + 74.0, bw, 8.0, rgba(0x9fb070, 0.18)).rounded(4.0));
                            cv.flat(Quad::grad(tx + 72.0, y + 74.0, bw * (into / need).clamp(0.0, 1.0) as f32, 8.0, rgb(0x8fbf5a), rgb(0xe6ff9a), false).rounded(4.0));
                            cv.text(Txt::new(format!("{} / {} XP", into as i64, need as i64), text::MONO, 13.0, tx + 72.0 + bw + 12.0, y + 70.0, c8(0x9fb070, 1.0)).clip(body));
                        }
                        y += 124.0;
                        // the season RANK card
                        let rank = [s("displayLabel"), s("displayRank"), s("rank")].into_iter().find(|v| !v.is_empty()).unwrap_or_default();
                        if !rank.is_empty() {
                            cv.flat(Quad::flat(px, y, w - 88.0, 112.0, rgba(0, 0.28)).rounded(12.0));
                            let ri = a.get("rankImg").and_then(|v| v.as_str()).map(String::from).filter(|v| !v.is_empty()).unwrap_or_else(|| {
                                let r = if s("displayRank").is_empty() { s("rank") } else { s("displayRank") };
                                format!("https://id.kawaii.uz/static/img/ranks/{}.webp", r.to_lowercase())
                            });
                            if let Some(t) = img(&ri) {
                                cv.quad(Some(t), Quad { rect: [px + 12.0, y + 8.0, 96.0, 96.0], uv: [0.0, 0.0, 1.0, 1.0], ..Quad::full() });
                            }
                            let rx = px + 124.0;
                            cv.text(Txt::new("SEZON RANKI", text::MONO, 11.0, rx, y + 18.0, c8(0x7c8456, 1.0)).weight(700).clip(body));
                            cv.text(Txt::new(rank.to_uppercase(), text::MONO, 20.0, rx, y + 36.0, c8(0xf2e49c, 1.0)).weight(700).clip(body));
                            let (sx, nx) = (n("seasonXp"), n("nextXp"));
                            if nx > 0.0 {
                                let bw = w - 88.0 - 150.0;
                                cv.flat(Quad::flat(rx, y + 68.0, bw, 8.0, rgba(0x9fb070, 0.18)).rounded(4.0));
                                cv.flat(Quad::grad(rx, y + 68.0, bw * (sx / nx).clamp(0.0, 1.0) as f32, 8.0, rgb(0x8fbf5a), rgb(0xe6ff9a), false).rounded(4.0));
                                let nl = s("nextLabel");
                                cv.text(Txt::new(format!("{} / {} XP{}", sx as i64, nx as i64, if nl.is_empty() { String::new() } else { format!(" → {}", nl.to_uppercase()) }), text::MONO, 13.0, rx, y + 82.0, c8(0x9fb070, 1.0)).clip(body));
                            }
                            y += 124.0;
                        }
                        button(cv, &mut hits, px, y, "[ CHIQISH ]", Act::Logout, body);
                        y += 52.0;
                    }
                    None => {
                        button(cv, &mut hits, px, y, "[ KAWAII ID BILAN KIRISH ]", Act::Login, body);
                        cv.text(Txt::new("Kirgandan keyin chatda yozish, stikerlar va LEVEL ochiladi.", text::MONO, 14.0, px, y + 50.0, c8(0x8f9a6a, 1.0)).clip(body));
                        y += 90.0;
                    }
                }
                // listening stats
                for (k, (v, l)) in [(cx.minutes, "DAQIQA"), (cx.liked.len() as u64, "SEVIMLI"), (cx.songs, "QO'SHIQ")].into_iter().enumerate() {
                    let bx = px + k as f32 * 220.0;
                    cv.text(Txt::new(v.to_string(), text::VT, 40.0, bx, y, c8(0xe6ff9a, 1.0)).clip(body));
                    cv.text(Txt::new(l, text::MONO, 12.0, bx, y + 42.0, c8(0x8f9a6a, 1.0)).weight(700).clip(body));
                }
                y += 70.0;
            }
            Screen::Settings => {
                let row = |cv: &mut Canvas, y: f32, t: &str, s: &str| {
                    cv.text(Txt::new(t, text::MONO, 17.0, px, y, c8(0xe6ff9a, 1.0)).weight(700).clip(body));
                    cv.text(Txt::new(s, text::MONO, 13.0, px, y + 24.0, c8(0x8f9a6a, 1.0)).clip(body));
                };
                row(cv, y, "Oyna hajmi", "Tez tanlov; chetidan tortib ham o'zgartiriladi");
                for (k, (l, a)) in [("S", Act::Size("s")), ("M", Act::Size("m")), ("L", Act::Size("l")), ("XL", Act::Size("xl"))].into_iter().enumerate() {
                    let bx = x1 - 300.0 + k as f32 * 66.0;
                    seg(cv, &mut hits, bx, y + 2.0, l, a, body);
                }
                y += 66.0;
                row(cv, y, "To'liq ekran", "F11 — kino rejimi kabi");
                seg(cv, &mut hits, x1 - 102.0, y + 2.0, "F11", Act::Full, body);
                y += 66.0;
                match cx.update {
                    Some(v) => {
                        row(cv, y, &format!("Yangi versiya: v{v}"), "Yuklab olinadi, ilova o'zi qayta ishga tushadi");
                        button(cv, &mut hits, x1 - 230.0, y, "[ YANGILASH ]", Act::Update, body);
                    }
                    None => row(cv, y, &format!("Versiya v{}", cx.version), "Eng so'nggi versiya o'rnatilgan"),
                }
                y += 66.0;
            }
            Screen::Home => {}
        }
        self.hits = hits;
        let total = y + self.scroll - body[1];
        self.scroll = self.scroll.clamp(0.0, (total - (body[3] - body[1]) + 20.0).max(0.0));
        cv.clip = None;
        over.flat(Quad::scan(x0, y0, w, h, 0.22).rounded(37.0));
        over.flat(Quad::grad(x0, y0, w, h * 0.16, rgba(0xe1f0ff, 0.07), rgba(0xe1f0ff, 0.0), true).rounded(37.0));
        over.flat(Quad::grad(x0, y1 - 50.0, w, 50.0, rgba(0, 0.0), rgba(0, 0.4), true).rounded(37.0));
    }
}

fn button(cv: &mut Canvas, hits: &mut Vec<([f32; 4], Act)>, x: f32, y: f32, label: &str, a: Act, clip: [f32; 4]) {
    let w = label.chars().count() as f32 * 12.0 + 26.0;
    cv.flat(Quad::flat(x, y, w, 36.0, rgba(0xc4e078, 0.4)).rounded(3.0));
    cv.flat(Quad::flat(x + 1.0, y + 1.0, w - 2.0, 34.0, rgb(0x0e130c)).rounded(3.0));
    cv.text(Txt::new(label, text::VT, 24.0, x + 13.0, y + 5.0, c8(0xe6ff9a, 1.0)).clip(clip));
    hits.push(([x, y, x + w, y + 36.0], a));
}

fn seg(cv: &mut Canvas, hits: &mut Vec<([f32; 4], Act)>, x: f32, y: f32, label: &str, a: Act, clip: [f32; 4]) {
    cv.flat(Quad::flat(x, y, 58.0, 36.0, rgba(0xc4e078, 0.35)).rounded(4.0));
    cv.flat(Quad::flat(x + 1.0, y + 1.0, 56.0, 34.0, rgb(0x0e130c)).rounded(4.0));
    cv.text(Txt::new(label, text::VT, 22.0, x + 29.0, y + 6.0, c8(0xe6ff9a, 1.0)).align(Align::Center).clip(clip));
    hits.push(([x, y, x + 58.0, y + 36.0], a));
}

#[allow(clippy::too_many_arguments)]
fn rows(cv: &mut Canvas, list: Vec<Track>, y: &mut f32, hits: &mut Vec<([f32; 4], Act)>, empty: &str, cx: &Ctx, img: &mut dyn FnMut(&str) -> Option<usize>, g: (f32, f32, f32, [f32; 4], f32)) {
    let (px, w, x1, body, row_h) = g;
            if list.is_empty() {
                cv.text(Txt::new(empty, text::MONO, 15.0, px, *y + 10.0, c8(0x8f9a6a, 1.0)).wrap(w - 80.0).clip(body));
                *y += 60.0;
            }
            for t in list {
                if *y + row_h > body[1] - 10.0 && *y < body[3] + 10.0 {
                    let now = cx.now == Some(t.id.as_str());
                    if now {
                        cv.flat(Quad::flat(px - 8.0, *y - 4.0, w - 52.0, row_h - 4.0, rgba(0xc4e078, 0.08)).rounded(6.0));
                    }
                    if let Some(c) = img(&crate::net::url(&t.cover_art)) {
                        cv.quad(Some(c), Quad { rect: [px, *y, 46.0, 46.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 5.0, ..Quad::full() });
                    } else {
                        cv.flat(Quad::flat(px, *y, 46.0, 46.0, rgb(0x1d2a20)).rounded(5.0));
                    }
                    let cl = [body[0], body[1].max(*y - 4.0), x1 - 110.0, body[3]];
                    cv.text(Txt::new(t.title.clone(), text::MR, 17.0, px + 60.0, *y + 2.0, c8(if now { 0xf2e49c } else { 0xe6ff9a }, 1.0)).weight(700).clip(cl));
                    cv.text(Txt::new(format!("{} · {}", t.artist, t.category), text::MR, 14.0, px + 60.0, *y + 25.0, c8(0x8f9a6a, 1.0)).weight(500).clip(cl));
                    let d = t.duration.max(0.0) as u64;
                    cv.text(Txt::new(format!("{}:{:02}", d / 60, d % 60), text::VT, 22.0, x1 - 52.0, *y + 10.0, c8(0x9fb070, 1.0)).align(Align::Right).clip(body));
                    hits.push(([px - 8.0, (*y - 4.0).max(body[1]), x1 - 30.0, (*y + row_h - 8.0).min(body[3])], Act::Play(t.id.clone())));
                }
                *y += row_h;
            }
        }
