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
    Theme(bool),
    OpenArtist(String),
    OpenAlbum(String, String),
    Unsave(String),
    AllArtists,
    /// the artist page's shelf (0 Mashhur, 1 Albomlar, 2 Singl va EP), "more songs", the long story
    Shelf(u8),
    More,
    Bio,
    PlayAll(Vec<String>, bool),
    Discography(String),
    ArtistSongs(String),
    Fav(String),
    Settings,
    Admin,
    Check,
    OpenId,
    Sleep(Option<u64>),
}

/// A page opened over a screen (an artist, one of their releases); ‹ ORQAGA goes back.
#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    /// every artist, A to Z
    AllArtists,
    Artist(String),
    Album(String, String),
    /// every release of an artist
    Disco(String),
    /// every song of an artist on Kawaii Radio
    Songs(String),
}

pub struct Panel {
    pub screen: Screen,
    pub query: String,
    pub scroll: f32,
    /// where the wheel wants the list, and how far it can go (from the last frame's layout)
    pub goal: f32,
    max: f32,
    last: std::time::Instant,
    /// the open list in KUTUBXONA: "liked" or a playlist id
    pub open: Option<String>,
    pub hits: Vec<([f32; 4], Act)>,
    /// artist / album pages opened over the screen
    pub page: Vec<Page>,
    /// a discography the page needs (main.rs fetches it)
    pub want_disco: Option<i64>,
    /// search results and artists' songs (ids), worked out once per query instead of every frame
    memo: std::cell::RefCell<(usize, usize, HashMap<String, Vec<String>>)>,
    /// on an artist page: the shelf, ten songs instead of five, the whole story
    pub shelf: u8,
    pub more: bool,
    pub bio: bool,
    /// an artist whose monthly listeners the page wants (main.rs asks the cloud)
    pub want_listeners: Option<String>,
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
    pub light: bool,
    pub artists: &'a [crate::artists::Artist],
    pub discos: &'a HashMap<i64, Option<crate::artists::Disco>>,
    pub saved: &'a [String],
    /// how often each song was played by all listeners (cloud), and an artist's monthly listeners
    pub plays: &'a HashMap<String, u64>,
    pub listeners: &'a HashMap<String, Option<u64>>,
    /// the favourite artist (synced with the phone)
    pub fav: Option<&'a str>,
    /// the profile: Kawaii Pass, chat staff, listening (all devices of the account), update check
    pub pass: bool,
    pub admin: Option<bool>,
    pub stats: &'a [(String, u64, u64)],
    pub checking: Option<&'a str>,
    pub sleep: Option<u64>,
    pub gpu: &'a str,
    pub soft: bool,
}

impl Panel {
    pub fn new() -> Self {
        Panel { screen: Screen::Home, query: String::new(), scroll: 0.0, goal: 0.0, max: 0.0, last: std::time::Instant::now(), open: None, hits: vec![], page: vec![], want_disco: None, memo: Default::default(), shelf: 0, more: false, bio: false, want_listeners: None }
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<Act> {
        self.hits.iter().rev().find(|(r, _)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]).map(|(_, a)| a.clone())
    }

    /// `f` once per key while the catalogue stays the same
    fn memo(&self, cx: &Ctx, key: String, f: impl FnOnce() -> Vec<String>) -> Vec<String> {
        let mut m = self.memo.borrow_mut();
        if (m.0, m.1) != (cx.tracks.len(), cx.artists.len()) {
            *m = (cx.tracks.len(), cx.artists.len(), HashMap::new());
        }
        if m.2.len() > 200 {
            m.2.clear();
        }
        m.2.entry(key).or_insert_with(f).clone()
    }

    fn tracks<'a>(cx: &Ctx<'a>, ids: Vec<String>) -> Vec<&'a Track> {
        ids.iter().filter_map(|i| cx.tracks.get(i)).collect()
    }

    /// An artist's songs on Kawaii Radio, the most played by everybody first.
    fn artist_songs<'a>(&self, cx: &Ctx<'a>, id: &str) -> Vec<&'a Track> {
        let ids = self.memo(cx, format!("s:{id}:{}", cx.plays.len()), || {
            let Some(a) = cx.artists.iter().find(|a| a.id == id) else { return vec![] };
            let mut v = a.songs(cx.tracks);
            v.sort_by_key(|t| std::cmp::Reverse(cx.plays.get(&t.id).copied().unwrap_or(0)));
            v.into_iter().map(|t| t.id).collect()
        });
        Self::tracks(cx, ids)
    }

    /// Artists matching the query (all of them, most songs first, when nothing is typed).
    fn found_artists<'a>(&self, cx: &Ctx<'a>, q: &str) -> Vec<&'a crate::artists::Artist> {
        let ids = self.memo(cx, format!("a:{q}"), || {
            let found: Vec<&crate::artists::Artist> = cx.artists.iter().filter(|a| q.is_empty() || norm(&a.name).contains(q) || a.aliases.iter().any(|x| norm(x).contains(q))).collect();
            if !q.is_empty() {
                return found.iter().map(|a| a.id.clone()).collect();
            }
            let credits: Vec<Vec<String>> = cx.tracks.values().map(|t| crate::artists::credit_keys(&t.artist)).collect();
            let mut sc: Vec<(usize, &crate::artists::Artist)> = found
                .iter()
                .map(|a| {
                    let k = a.keys();
                    (credits.iter().filter(|c| c.iter().any(|c| k.contains(c))).count(), *a)
                })
                .collect();
            sc.sort_by(|a, b| b.0.cmp(&a.0));
            sc.into_iter().map(|x| x.1.id.clone()).collect()
        });
        ids.iter().filter_map(|i| cx.artists.iter().find(|a| &a.id == i)).collect()
    }

    /// The songs the panel lists now (search results, or the open list in the library).
    pub fn list<'a>(&self, cx: &Ctx<'a>) -> Vec<&'a Track> {
        match self.page.last() {
            Some(Page::Artist(id)) => return self.artist_songs(cx, id).into_iter().take(if self.more { 10 } else { 5 }).collect(),
            Some(Page::Album(aid, rid)) => {
                let a = cx.artists.iter().find(|a| &a.id == aid);
                let r = a.and_then(|a| a.deezer).and_then(|d| cx.discos.get(&d)).and_then(|d| d.as_ref()).and_then(|d| d.releases.iter().find(|r| &r.id == rid));
                return r.map(|r| r.ours().iter().filter_map(|i| cx.tracks.get(i)).collect()).unwrap_or_default();
            }
            Some(Page::Songs(id)) => return self.artist_songs(cx, id),
            Some(Page::AllArtists) | Some(Page::Disco(_)) | None => {}
        }
        match self.screen {
            Screen::Search => {
                let q = norm(self.query.trim());
                let ids = self.memo(cx, format!("q:{q}"), || {
                    let mut v: Vec<&Track> = cx.tracks.values().filter(|t| q.is_empty() || norm(&t.title).contains(&q) || norm(&t.artist).contains(&q)).collect();
                    v.sort_by(|a, b| a.artist.cmp(&b.artist).then(a.title.cmp(&b.title)));
                    v.truncate(120);
                    v.into_iter().map(|t| t.id.clone()).collect()
                });
                Self::tracks(cx, ids)
            }
            Screen::Library => match self.open.as_deref() {
                Some("liked") => cx.liked.iter().rev().filter_map(|id| cx.tracks.get(id)).collect(),
                Some("saved") => cx.saved.iter().filter_map(|id| cx.tracks.get(id)).collect(),
                Some(p) => cx.playlists.iter().find(|x| x.0 == p).map(|x| x.2.iter().filter_map(|id| cx.tracks.get(id)).collect()).unwrap_or_default(),
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
        // the backplate and the glass are the tube's own (main.rs), the same for every page
        let _ = (w, h);
        let dt = self.last.elapsed().as_secs_f32().min(0.1);
        self.last = std::time::Instant::now();
        self.goal = self.goal.clamp(0.0, self.max);
        self.scroll += (self.goal - self.scroll) * (1.0 - (-dt / 0.07).exp());
        if (self.goal - self.scroll).abs() < 0.3 {
            self.scroll = self.goal;
        }
        self.scroll = self.scroll.clamp(0.0, self.max);
        let px = x0 + 34.0;
        let (title, sub) = match self.screen {
            Screen::Search => ("QIDIRUV", format!("{} ta qo'shiq efirda", cx.tracks.len())),
            Screen::Library => ("KUTUBXONA", format!("{} ta sevimli · {} ta pleylist", cx.liked.len(), cx.playlists.len())),
            Screen::Profile => ("PROFIL", "KR-01 DESK".into()),
            Screen::Settings => ("SOZLAMALAR", format!("KR-01 DESK · v{}", cx.version)),
            Screen::Home => unreachable!(),
        };
        let (title, sub) = match self.page.last() {
            Some(Page::Artist(_)) => ("IJROCHI", "Kawaii Radio · ijrochilar".to_string()),
            Some(Page::AllArtists) => ("IJROCHILAR", format!("{} ta ijrochi", cx.artists.len())),
            Some(Page::Album(..)) => ("RELIZ", "Ma'lumot: Deezer".to_string()),
            Some(Page::Disco(_)) => ("DISKOGRAFIYA", "Albomlar, singl va EP".to_string()),
            Some(Page::Songs(_)) => ("QO'SHIQLARI", "Kawaii Radio'da".to_string()),
            None => (title, sub),
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
        if let Some(pg) = self.page.last().cloned() {
            cv.text(Txt::new("‹ ORQAGA", text::VT, 22.0, px, y, c8(0xc4e078, 1.0)).clip(body));
            hits.push(([px - 8.0, y - 4.0, px + 140.0, y + 28.0], Act::Back));
            y += 40.0;
            match pg {
                Page::AllArtists => {
                    // a grid of every artist, A to Z, six to a row
                    let mut all: Vec<&crate::artists::Artist> = cx.artists.iter().collect();
                    all.sort_by_key(|a| a.name.to_lowercase());
                    for row in all.chunks(7) {
                        if y + 110.0 > body[1] - 10.0 && y < body[3] + 10.0 {
                            artist_strip(cv, img, &mut hits, px, y, row.to_vec(), body);
                        }
                        y += 116.0;
                    }
                }
                Page::Artist(id) => self.artist_page(cv, tl, cx, img, &mut hits, &mut y, &id, (px, w, x1, body, row_h)),
                Page::Album(aid, rid) => self.album_page(cv, cx, img, &mut hits, &mut y, &aid, &rid, (px, w, x1, body, row_h)),
                Page::Disco(aid) => self.disco_page(cv, tl, cx, img, &mut hits, &mut y, &aid, (px, w, x1, body, row_h)),
                Page::Songs(aid) => {
                    let list = self.artist_songs(cx, &aid);
                    let ids: Vec<String> = list.iter().map(|t| t.id.clone()).collect();
                    if !ids.is_empty() {
                        pill(cv, &mut hits, tl, px, y, "► HAMMASINI IJRO ETISH", true, Act::PlayAll(ids.clone(), false), body);
                        pill(cv, &mut hits, tl, px + 250.0, y, "⤮ ARALASH", false, Act::PlayAll(ids, true), body);
                        y += 50.0;
                    }
                    numbered(cv, list, &mut y, &mut hits, cx, img, (px, w, x1, body, row_h));
                }
            }
            self.hits = hits;
            let total = y + self.scroll - body[1];
            self.max = (total - (body[3] - body[1]) + 20.0).max(0.0);
            cv.clip = None;
            let _ = over;
            return;
        }
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
                // IJROCHILAR: the artists whose name matches (all of them, most songs first, when nothing is typed)
                let found = self.found_artists(cx, &norm(self.query.trim()));
                if !found.is_empty() {
                    cv.text(Txt::new("IJROCHILAR", text::MONO, 12.0, px, y, c8(0xa9b97a, 1.0)).weight(700).clip(body));
                    // all of them, A to Z
                    let all = format!("BARCHASI ({}) ›", cx.artists.len());
                    cv.text(Txt::new(all, text::VT, 20.0, x1 - 50.0, y - 4.0, c8(0xc4e078, 1.0)).align(Align::Right).clip(body));
                    hits.push(([x1 - 230.0, y - 8.0, x1 - 40.0, y + 20.0], Act::AllArtists));
                    y += 20.0;
                    artist_strip(cv, img, &mut hits, px, y, found.into_iter().take(7).collect(), body);
                    y += 118.0;
                    cv.text(Txt::new("QO'SHIQLAR", text::MONO, 12.0, px, y, c8(0xa9b97a, 1.0)).weight(700).clip(body));
                    y += 22.0;
                }
                let list = self.list(cx);
                rows(cv, list, &mut y, &mut hits, "Hech narsa topilmadi.", cx, img, (px, w, x1, body, row_h));
            }
            Screen::Library => {
                if self.open.is_some() {
                    cv.text(Txt::new("‹ ORQAGA", text::VT, 22.0, px, y, c8(0xc4e078, 1.0)).clip(body));
                    hits.push(([px - 8.0, y - 4.0, px + 140.0, y + 28.0], Act::Back));
                    y += 38.0;
                    let list = self.list(cx);
                    let saved_list = self.open.as_deref() == Some("saved");
                    let y_list = y;
                    let ids: Vec<String> = list.iter().map(|t| t.id.clone()).collect();
                    rows(cv, list, &mut y, &mut hits, if saved_list { "Hali hech narsa yuklanmagan. Qo'shiq chalayotganda SAQLASH tugmasini bosing (Kawaii Pass)." } else { "Bu ro'yxat bo'sh." }, cx, img, (px, w, x1, body, row_h));
                    if saved_list {
                        for (k, id) in ids.into_iter().enumerate() {
                            let ry = y_list + k as f32 * row_h;
                            if ry + row_h > body[1] && ry < body[3] {
                                cv.text(Txt::new("×", text::VT, 26.0, x1 - 112.0, ry + 8.0, c8(0xff8a7a, 0.9)).clip(body));
                                hits.push(([x1 - 122.0, ry, x1 - 86.0, ry + 40.0], Act::Unsave(id)));
                            }
                        }
                    }
                } else {
                    let mut items: Vec<(String, String, Vec<String>)> = vec![
                        ("liked".into(), "♥ SEVIMLILAR".into(), cx.liked.iter().rev().cloned().collect()),
                        ("saved".into(), "⬇ YUKLANGANLAR".into(), cx.saved.to_vec()),
                    ];
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
                    // SEVIMLI IJROCHILAR: the artists of the liked songs, the most liked first
                    let ids = self.memo(cx, format!("fav:{}:{}", cx.liked.len(), cx.liked.last().map(String::as_str).unwrap_or("")), || {
                        let credits: Vec<Vec<String>> = cx.liked.iter().filter_map(|id| cx.tracks.get(id)).map(|t| crate::artists::credit_keys(&t.artist)).collect();
                        let mut v: Vec<(usize, &crate::artists::Artist)> = cx
                            .artists
                            .iter()
                            .map(|a| {
                                let k = a.keys();
                                (credits.iter().filter(|c| c.iter().any(|c| k.contains(c))).count(), a)
                            })
                            .filter(|x| x.0 > 0)
                            .collect();
                        v.sort_by(|a, b| b.0.cmp(&a.0));
                        v.into_iter().map(|x| x.1.id.clone()).collect()
                    });
                    let fav: Vec<(usize, &crate::artists::Artist)> = ids.iter().filter_map(|i| cx.artists.iter().find(|a| &a.id == i)).map(|a| (0, a)).collect();
                    if !fav.is_empty() {
                        y += 6.0;
                        cv.text(Txt::new("SEVIMLI IJROCHILAR", text::MONO, 12.0, px, y, c8(0xa9b97a, 1.0)).weight(700).clip(body));
                        cv.text(Txt::new(format!("BARCHA IJROCHILAR ({}) ›", cx.artists.len()), text::VT, 20.0, x1 - 50.0, y - 4.0, c8(0xc4e078, 1.0)).align(Align::Right).clip(body));
                        hits.push(([x1 - 300.0, y - 8.0, x1 - 40.0, y + 20.0], Act::AllArtists));
                        y += 20.0;
                        artist_strip(cv, img, &mut hits, px, y, fav.into_iter().take(7).map(|x| x.1).collect(), body);
                        y += 124.0;
                    }
                    if cx.acc.is_none() {
                        cv.text(Txt::new("Telefondagi pleylistlar va sevimlilar Kawaii ID bilan kirgandan keyin shu yerda chiqadi.", text::MONO, 14.0, px, y + 6.0, c8(0x8f9a6a, 1.0)).wrap(w - 90.0).clip(body));
                        y += 50.0;
                    }
                }
            }
            Screen::Profile => {
                let s = |k: &str| cx.acc.and_then(|a| a.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let n = |k: &str| cx.acc.and_then(|a| a.get(k)).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let cw = w - 68.0;
                // ── who: the Kawaii ID picture, the name, Kawaii Pass; the gear opens SOZLAMALAR
                cv.flat(Quad::flat(px - 8.0, y - 8.0, cw + 16.0, 128.0, rgba(0xc4e078, 0.05)).rounded(18.0));
                cv.flat(Quad::flat(px - 3.0, y - 3.0, 110.0, 110.0, rgba(0xc4e078, 0.3)).rounded(55.0));
                match cx.acc.and_then(|a| a.get("avatar")).and_then(|v| v.as_str()).filter(|u| u.starts_with("https://")).and_then(|u| img(u)) {
                    Some(t) => cv.quad(Some(t), Quad { rect: [px, y, 104.0, 104.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 52.0, ..Quad::full() }),
                    None => {
                        cv.flat(Quad::flat(px, y, 104.0, 104.0, rgb(0x1d2a20)).rounded(52.0));
                        cv.text(Txt::new("?", text::VT, 56.0, px + 52.0, y + 22.0, c8(0x5d6b45, 1.0)).align(Align::Center).clip(body));
                    }
                }
                hits.push(([px, y, px + 104.0, y + 104.0], if cx.acc.is_some() { Act::OpenId } else { Act::Login }));
                let tx = px + 128.0;
                let name = if cx.acc.is_none() { "MEHMON".to_string() } else if s("name").is_empty() { s("username") } else { s("name") };
                cv.text(Txt::new(name, text::MR, 26.0, tx, y + 8.0, c8(0xe6ff9a, if cx.acc.is_some() { 1.0 } else { 0.5 })).weight(800).clip([body[0], body[1], x1 - 210.0, body[3]]));
                let sub = if cx.acc.is_some() { format!("@{} · Kawaii ID", s("username")) } else { "Kawaii ID bilan kiring: ism, LEVEL va Kawaii Pass".into() };
                cv.text(Txt::new(sub, text::MR, 14.0, tx, y + 44.0, c8(0x8f9a6a, 1.0)).weight(500).clip([body[0], body[1], x1 - 60.0, body[3]]));
                if cx.pass {
                    cv.flat(Quad::flat(tx, y + 72.0, 132.0, 26.0, rgba(0xf5b93b, 0.16)).rounded(8.0));
                    cv.text(Txt::new("★ KAWAII PASS", text::VT, 20.0, tx + 10.0, y + 74.0, c8(0xf5c85b, 1.0)).clip(body));
                } else if cx.acc.is_none() {
                    pill(cv, &mut hits, tl, tx, y + 68.0, "KAWAII ID BILAN KIRISH", true, Act::Login, body);
                }
                let gw = tl.measure(&Txt::new("⚙ SOZLAMALAR", text::VT, 22.0, 0.0, 0.0, [0; 4])) + 34.0;
                pill(cv, &mut hits, tl, x1 - 48.0 - gw, y, "⚙ SOZLAMALAR", false, Act::Settings, body);
                y += 136.0;

                // ── LEVEL and the season RANK (ID opens the card on id.kawaii.uz)
                if cx.acc.is_some() {
                    let rank = [s("displayLabel"), s("displayRank"), s("rank")].into_iter().find(|v| !v.is_empty()).unwrap_or_default();
                    let h = if rank.is_empty() { 64.0 } else { 150.0 };
                    cv.flat(Quad::flat(px - 8.0, y - 8.0, cw + 16.0, h, rgba(0, 0.28)).rounded(16.0));
                    cv.text(Txt::new(format!("LEVEL {}", n("level") as i64), text::VT, 30.0, px + 6.0, y - 2.0, c8(0xe6ff9a, 1.0)).clip(body));
                    let (into, need) = (n("xpInto"), n("xpNeeded"));
                    let bw = cw - 300.0;
                    if need > 0.0 {
                        cv.flat(Quad::flat(px + 150.0, y + 12.0, bw, 10.0, rgba(0x9fb070, 0.18)).rounded(5.0));
                        cv.flat(Quad::grad(px + 150.0, y + 12.0, bw * (into / need).clamp(0.0, 1.0) as f32, 10.0, rgb(0x8fbf5a), rgb(0xe6ff9a), false).rounded(5.0));
                        cv.text(Txt::new(format!("{} / {} XP", into as i64, need as i64), text::MONO, 12.0, px + 150.0, y + 26.0, c8(0x9fb070, 1.0)).clip(body));
                    }
                    chip(cv, &mut hits, tl, x1 - 112.0, y + 2.0, "ID ›", false, Some(Act::OpenId), body);
                    if !rank.is_empty() {
                        let ry = y + 52.0;
                        let ri = cx.acc.and_then(|a| a.get("rankImg")).and_then(|v| v.as_str()).map(String::from).filter(|v| !v.is_empty()).unwrap_or_else(|| {
                            let r = if s("displayRank").is_empty() { s("rank") } else { s("displayRank") };
                            format!("https://id.kawaii.uz/static/img/ranks/{}.webp", r.to_lowercase())
                        });
                        if let Some(t) = img(&ri) {
                            cv.quad(Some(t), Quad { rect: [px + 4.0, ry - 6.0, 88.0, 88.0], uv: [0.0, 0.0, 1.0, 1.0], ..Quad::full() });
                        }
                        let rx = px + 110.0;
                        cv.text(Txt::new("SEZON RANKI", text::MONO, 11.0, rx, ry + 2.0, c8(0x7c8456, 1.0)).weight(700).clip(body));
                        cv.text(Txt::new(rank.to_uppercase(), text::MR, 22.0, rx, ry + 18.0, c8(0xf2e49c, 1.0)).weight(800).clip(body));
                        let (sx, nx) = (n("seasonXp"), n("nextXp"));
                        if nx > 0.0 {
                            let bw = cw - 130.0;
                            cv.flat(Quad::flat(rx, ry + 54.0, bw, 8.0, rgba(0x9fb070, 0.18)).rounded(4.0));
                            cv.flat(Quad::grad(rx, ry + 54.0, bw * (sx / nx).clamp(0.0, 1.0) as f32, 8.0, rgb(0xd9a93a), rgb(0xf2e49c), false).rounded(4.0));
                            let nl = s("nextLabel");
                            cv.text(Txt::new(format!("{} / {} XP{}", sx as i64, nx as i64, if nl.is_empty() { String::new() } else { format!(" → {}", nl.to_uppercase()) }), text::MONO, 12.0, rx, ry + 66.0, c8(0x9fb070, 1.0)).clip(body));
                        }
                    }
                    y += h + 8.0;
                }

                // ── the favourite artist: an LCD strip (click: their page; ► plays their songs mixed)
                let fav = cx.fav.and_then(|f| cx.artists.iter().find(|a| a.name.eq_ignore_ascii_case(f)));
                cv.flat(Quad::flat(px - 8.0, y - 2.0, cw + 16.0, 64.0, rgb(0x2a3320)).rounded(12.0));
                cv.flat(Quad::grad(px - 6.0, y, cw + 12.0, 60.0, rgb(0xd3e3a2), rgb(0xb4c886), true).rounded(10.0));
                cv.text(Txt::new("SEVIMLI IJROCHI", text::MONO, 11.0, px + 10.0, y + 8.0, c8(0x26321a, 0.6)).weight(700).clip(body));
                let fname = cx.fav.map(|f| f.to_uppercase()).unwrap_or_else(|| "IJROCHI SAHIFASIDA ☆ NI BOSING".into());
                cv.text(Txt::new(fname, text::VT, 28.0, px + 10.0, y + 24.0, c8(0x26321a, if cx.fav.is_some() { 1.0 } else { 0.45 })).clip([body[0], body[1], x1 - 110.0, body[3]]));
                hits.push(([px - 6.0, y, x1 - 110.0, y + 60.0], fav.map(|a| Act::OpenArtist(a.id.clone())).unwrap_or(Act::AllArtists)));
                if let Some(a) = fav {
                    let ids = self.artist_songs(cx, &a.id).iter().map(|t| t.id.clone()).collect::<Vec<_>>();
                    if !ids.is_empty() {
                        let (bx, by) = (x1 - 92.0, y + 10.0);
                        cv.flat(Quad::flat(bx, by, 40.0, 40.0, rgb(0x26321a)).rounded(20.0));
                        cv.text(Txt::new("►", text::MR, 18.0, bx + 21.0, by + 9.0, c8(0xd3e3a2, 1.0)).align(Align::Center).clip(body));
                        hits.push(([bx, by, bx + 40.0, by + 40.0], Act::PlayAll(ids, true)));
                    }
                }
                y += 76.0;

                // ── admins: their desk
                if let Some(owner) = cx.admin {
                    cv.flat(Quad::grad(px - 8.0, y, cw + 16.0, 60.0, rgb(0xffe08a), rgb(0xf5b93b), true).rounded(14.0));
                    cv.flat(Quad::flat(px, y + 1.0, cw, 1.5, rgba(0xffffff, 0.6)).rounded(1.0));
                    cv.text(Txt::new("★", text::MR, 22.0, px + 8.0, y + 15.0, c8(0x3a2a05, 1.0)).clip(body));
                    cv.text(Txt::new(if owner { "BOSH ADMIN PANELI" } else { "ADMIN PANELI" }, text::MR, 16.0, px + 42.0, y + 9.0, c8(0x3a2a05, 1.0)).weight(800).clip(body));
                    cv.text(Txt::new("Chat sozlamalari: sekin rejim, yopish, tozalash", text::MR, 13.0, px + 42.0, y + 32.0, c8(0x3a2a05, 0.7)).weight(500).clip(body));
                    cv.text(Txt::new("›", text::VT, 30.0, x1 - 54.0, y + 12.0, c8(0x3a2a05, 1.0)).clip(body));
                    hits.push(([px - 8.0, y, x1 - 40.0, y + 60.0], Act::Admin));
                    y += 74.0;
                }

                // ── STATISTIKA (Kawaii Pass): all of the account's devices together
                label(cv, px, &mut y, "STATISTIKA", body);
                if !cx.pass {
                    cv.flat(Quad::flat(px - 8.0, y - 4.0, cw + 16.0, 70.0, rgba(0, 0.28)).rounded(14.0));
                    cv.text(Txt::new("🔒 KAWAII PASS", text::VT, 24.0, px + 6.0, y + 4.0, c8(0xf5c85b, 1.0)).clip(body));
                    cv.text(Txt::new("Tinglagan vaqtingiz, eng ko'p tinglaganlaringiz va oy xulosasi obunachilar uchun.", text::MR, 13.0, px + 6.0, y + 34.0, c8(0x8f9a6a, 1.0)).weight(500).wrap(cw - 20.0).clip(body));
                    y += 80.0;
                } else {
                    let secs: u64 = cx.stats.iter().map(|x| x.1).sum::<u64>().max(cx.minutes * 60);
                    let plays: u64 = cx.stats.iter().map(|x| x.2).sum::<u64>().max(cx.songs);
                    cv.flat(Quad::flat(px - 8.0, y - 2.0, cw + 16.0, 112.0, rgb(0x2a3320)).rounded(14.0));
                    cv.flat(Quad::grad(px - 5.0, y + 1.0, cw + 10.0, 106.0, rgb(0xd3e3a2), rgb(0xb4c886), true).rounded(12.0));
                    let col = (cw + 10.0) / 4.0;
                    for (k, (v, l)) in [(secs / 60, "DAQIQA"), (plays, "TINGLASH"), (cx.liked.len() as u64, "SEVIMLI"), (cx.saved.len() as u64, "YUKLANGAN")].into_iter().enumerate() {
                        let bx = px - 5.0 + col * (k as f32 + 0.5);
                        cv.text(Txt::new(format!("{v:02}"), text::VT, 44.0, bx, y + 4.0, c8(0x26321a, 1.0)).align(Align::Center).clip(body));
                        cv.text(Txt::new(l, text::MONO, 11.0, bx, y + 50.0, c8(0x26321a, 0.65)).weight(700).align(Align::Center).clip(body));
                    }
                    // who you listen to most (by time)
                    let mut by: HashMap<String, u64> = HashMap::new();
                    for (id, sec, _) in cx.stats.iter() {
                        if let Some(t) = cx.tracks.get(id) {
                            let main = crate::artists::main_artist(&t.artist);
                            *by.entry(main).or_default() += sec;
                        }
                    }
                    let top_artist = by.into_iter().max_by_key(|x| x.1).map(|x| x.0);
                    cv.flat(Quad::flat(px + 6.0, y + 74.0, cw - 12.0, 1.0, rgba(0x26321a, 0.2)));
                    cv.text(Txt::new(format!("KO'P TINGLAGANINGIZ: {}", top_artist.unwrap_or_else(|| "—".into()).to_uppercase()), text::VT, 20.0, px + 8.0, y + 80.0, c8(0x26321a, 1.0)).clip([body[0], body[1], x1 - 40.0, body[3]]));
                    y += 124.0;
                    let mut k = 0;
                    for (id, sec, _) in cx.stats.iter() {
                        let Some(t) = cx.tracks.get(id) else { continue };
                        if k == 5 {
                            break;
                        }
                        k += 1;
                        if y + 52.0 > body[1] - 10.0 && y < body[3] + 10.0 {
                            cv.text(Txt::new(k.to_string(), text::VT, 24.0, px + 8.0, y + 8.0, c8(0x8f9a6a, 1.0)).align(Align::Center).clip(body));
                            match img(&crate::net::url(&t.cover_art)) {
                                Some(c) => cv.quad(Some(c), Quad { rect: [px + 26.0, y + 2.0, 40.0, 40.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 6.0, ..Quad::full() }),
                                None => cv.flat(Quad::flat(px + 26.0, y + 2.0, 40.0, 40.0, rgb(0x1d2a20)).rounded(6.0)),
                            }
                            let cl = [body[0], body[1].max(y - 4.0), x1 - 150.0, body[3]];
                            cv.text(Txt::new(t.title.clone(), text::MR, 15.0, px + 80.0, y + 2.0, c8(0xe6ff9a, 1.0)).weight(700).clip(cl));
                            cv.text(Txt::new(t.artist.clone(), text::MR, 12.0, px + 80.0, y + 23.0, c8(0x8f9a6a, 1.0)).weight(500).clip(cl));
                            let tm = if *sec >= 3600 { format!("{} SOAT {} DAQ", sec / 3600, sec % 3600 / 60) } else { format!("{} DAQ", (sec / 60).max(1)) };
                            cv.text(Txt::new(tm, text::VT, 20.0, x1 - 52.0, y + 10.0, c8(0x9fb070, 1.0)).align(Align::Right).clip(body));
                            hits.push(([px - 6.0, y.max(body[1]), x1 - 40.0, (y + 46.0).min(body[3])], Act::Play(t.id.clone())));
                        }
                        y += 52.0;
                    }
                    y += 6.0;
                }

                // ── the account and the app
                label(cv, px, &mut y, "AKKAUNT VA ILOVA", body);
                cv.flat(Quad::flat(px - 8.0, y - 4.0, cw + 16.0, 128.0, rgba(0xc4e078, 0.05)).rounded(14.0));
                let row = |cv: &mut Canvas, y: f32, t: &str, sub: &str| {
                    cv.text(Txt::new(t, text::MR, 16.0, px + 6.0, y + 4.0, c8(0xe6ff9a, 1.0)).weight(700).clip([body[0], body[1], x1 - 200.0, body[3]]));
                    cv.text(Txt::new(sub, text::MR, 13.0, px + 6.0, y + 27.0, c8(0x8f9a6a, 1.0)).weight(500).clip([body[0], body[1], x1 - 200.0, body[3]]));
                };
                match cx.acc {
                    Some(_) => {
                        let lv = format!("LEVEL {}", n("level") as i64);
                        let rk = [s("displayLabel"), s("displayRank"), s("rank")].into_iter().find(|v| !v.is_empty());
                        let sub = [Some(lv), rk, cx.pass.then(|| "Kawaii Pass".to_string())].into_iter().flatten().collect::<Vec<_>>().join(" · ");
                        row(cv, y, &format!("@{}", s("username")), &sub);
                        let bw = tl.measure(&Txt::new("CHIQISH", text::VT, 22.0, 0.0, 0.0, [0; 4])) + 34.0;
                        pill(cv, &mut hits, tl, x1 - 48.0 - bw, y + 4.0, "CHIQISH", false, Act::Logout, body);
                    }
                    None => {
                        row(cv, y, "Kawaii ID", "Kawaii.uz akkauntingiz bilan kiring: ism va Kawaii Pass");
                        let bw = tl.measure(&Txt::new("KIRISH", text::VT, 22.0, 0.0, 0.0, [0; 4])) + 34.0;
                        pill(cv, &mut hits, tl, x1 - 48.0 - bw, y + 4.0, "KIRISH", true, Act::Login, body);
                    }
                }
                cv.flat(Quad::flat(px, y + 58.0, cw, 1.0, rgba(0xc4e078, 0.14)));
                let status = match (cx.update, cx.checking) {
                    (_, Some(t)) => t.to_string(),
                    (Some(v), None) => format!("Yangi versiya bor: v{v}"),
                    (None, None) => "Yangilanishlar o'zi tekshiriladi".to_string(),
                };
                row(cv, y + 64.0, &format!("Kawaii Radio v{}", cx.version), &status);
                let (bt, act) = if cx.update.is_some() { ("YANGILASH", Act::Update) } else { ("TEKSHIRISH", Act::Check) };
                let bw = tl.measure(&Txt::new(bt, text::VT, 22.0, 0.0, 0.0, [0; 4])) + 34.0;
                pill(cv, &mut hits, tl, x1 - 48.0 - bw, y + 68.0, bt, cx.update.is_some(), act, body);
                y += 140.0;
                cv.text(Txt::new("KAWAII RADIO · KR-01 DESK", text::MONO, 11.0, px + cw / 2.0, y, c8(0x6b7350, 1.0)).weight(700).align(Align::Center).clip(body));
                y += 30.0;
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
                row(cv, y, "Mavzu", "Kunduzgi oq yoki tungi grafit korpus");
                for (k, (l, a)) in [("KUN", Act::Theme(true)), ("TUN", Act::Theme(false))].into_iter().enumerate() {
                    let on = matches!(a, Act::Theme(x) if x == cx.light);
                    let bx = x1 - 168.0 + k as f32 * 66.0;
                    if on {
                        cv.flat(Quad::flat(bx - 4.0, y - 2.0, 66.0, 44.0, rgba(0xc4e078, 0.14)).rounded(8.0));
                    }
                    seg(cv, &mut hits, bx, y + 2.0, l, a, body);
                }
                y += 66.0;
                // the sleep timer: the music fades out by itself
                let left = cx.sleep.map(|m| format!("Musiqa {m} daqiqadan keyin sekin to'xtaydi")).unwrap_or_else(|| "Musiqa o'zi sekin to'xtaydi".into());
                row(cv, y, "Uyqu taymeri", &left);
                let mut bx = x1 - 48.0;
                for (l, m) in [("O'CHIQ", None), ("90", Some(90)), ("60", Some(60)), ("30", Some(30)), ("15", Some(15))] {
                    let cw = tl.measure(&Txt::new(l, text::VT, 20.0, 0.0, 0.0, [0; 4])) + 24.0;
                    bx -= cw + 6.0;
                    let on = m.is_none() && cx.sleep.is_none();
                    chip(cv, &mut hits, tl, bx, y + 6.0, l, on, Some(Act::Sleep(m)), body);
                }
                y += 66.0;
                match cx.update {
                    Some(v) => {
                        row(cv, y, &format!("O'rnatilgan: v{} → yangi: v{v}", cx.version), cx.checking.unwrap_or("Yuklab olinadi, ilova o'zi qayta ishga tushadi"));
                        button(cv, &mut hits, x1 - 230.0, y, "[ YANGILASH ]", Act::Update, body);
                    }
                    None => {
                        row(cv, y, &format!("O'rnatilgan versiya: v{}", cx.version), cx.checking.unwrap_or("Eng so'nggi versiya"));
                        button(cv, &mut hits, x1 - 230.0, y, "[TEKSHIRISH]", Act::Check, body);
                    }
                }
                y += 66.0;
                row(cv, y, "Grafika", &format!("{}{}", cx.gpu, if cx.soft { " · protsessorda (sekin): nur effektlari o'chirilgan" } else { "" }));
                y += 66.0;
            }
            Screen::Home => {}
        }
        self.hits = hits;
        let total = y + self.scroll - body[1];
        self.max = (total - (body[3] - body[1]) + 20.0).max(0.0);
        cv.clip = None;
        let _ = over;
    }
}

/// A row of artist photos (round) with their names; a click opens the artist.
fn artist_strip(cv: &mut Canvas, img: &mut dyn FnMut(&str) -> Option<usize>, hits: &mut Vec<([f32; 4], Act)>, px: f32, y: f32, list: Vec<&crate::artists::Artist>, body: [f32; 4]) {
    for (k, a) in list.into_iter().enumerate() {
        let x = px + k as f32 * 96.0;
        let pic = a.thumb.as_ref().or(a.picture.as_ref()).and_then(|u| img(u));
        cv.flat(Quad::flat(x + 6.0, y, 76.0, 76.0, rgba(0xc4e078, 0.25)).rounded(38.0));
        match pic {
            Some(t) => cv.quad(Some(t), Quad { rect: [x + 8.0, y + 2.0, 72.0, 72.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 36.0, ..Quad::full() }),
            None => cv.flat(Quad::flat(x + 8.0, y + 2.0, 72.0, 72.0, rgb(0x1d2a20)).rounded(36.0)),
        }
        cv.text(Txt::new(a.name.clone(), text::MR, 13.0, x + 44.0, y + 82.0, c8(0xe6ff9a, 1.0)).weight(700).align(Align::Center).clip([x, body[1], x + 90.0, body[3]]));
        hits.push(([x, y, x + 90.0, y + 102.0], Act::OpenArtist(a.id.clone())));
    }
}

impl Panel {
    /// An artist, like the phone's page: the photo, who they are and the monthly listeners; play, shuffle and
    /// favourite; the story; the songs played most on Kawaii Radio; the discography on shelves; every song.
    #[allow(clippy::too_many_arguments)]
    fn artist_page(&mut self, cv: &mut Canvas, tl: &mut text::TextLayer, cx: &Ctx, img: &mut dyn FnMut(&str) -> Option<usize>, hits: &mut Vec<([f32; 4], Act)>, y: &mut f32, id: &str, g: (f32, f32, f32, [f32; 4], f32)) {
        let (px, w, x1, body, _) = g;
        let Some(a) = cx.artists.iter().find(|a| a.id == id) else { return };
        let all = self.artist_songs(cx, id);
        let ids: Vec<String> = all.iter().map(|t| t.id.clone()).collect();
        // ── the head: a round photo in a lit ring, the name, who they are, an LCD plate with the listeners
        let top = *y;
        cv.flat(Quad::flat(px - 5.0, *y - 5.0, 138.0, 138.0, rgba(0xc4e078, 0.28)).rounded(69.0));
        cv.flat(Quad::flat(px - 2.0, *y - 2.0, 132.0, 132.0, rgb(0x0b1009)).rounded(66.0));
        match a.picture.as_ref().or(a.thumb.as_ref()).and_then(|u| img(u)) {
            Some(t) => cv.quad(Some(t), Quad { rect: [px, *y, 128.0, 128.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 64.0, ..Quad::full() }),
            None => cv.flat(Quad::flat(px, *y, 128.0, 128.0, rgb(0x1d2a20)).rounded(64.0)),
        }
        let tx = px + 156.0;
        let tw = x1 - 50.0 - tx;
        cv.text(Txt::new(a.name.clone(), text::MR, 30.0, tx, *y - 2.0, c8(0xe6ff9a, 1.0)).weight(800).clip(body).glow([230, 255, 154, 50], 2.0));
        let mut ty = *y + 42.0;
        if !a.tagline.is_empty() {
            let tag = Txt::new(a.tagline.clone(), text::MR, 15.0, tx, ty, c8(0xc9d79a, 1.0)).weight(500).wrap(tw).line(20.0).clip(body);
            let (_, th) = tl.measure2(&tag);
            cv.text(tag);
            ty += th + 10.0;
        }
        if !cx.listeners.contains_key(id) {
            self.want_listeners = Some(id.to_string());
        }
        let plate = match cx.listeners.get(id) {
            Some(Some(n)) if *n > 0 => format!("OYLIK TINGLOVCHILAR · {}", count(*n)),
            _ => format!("KAWAII RADIO'DA · {} TA QO'SHIQ", all.len()),
        };
        lcd_plate(cv, tl, tx, ty, &plate, body);
        *y = (top + 146.0).max(ty + 44.0);
        // play, shuffle, favourite
        let fav = cx.fav.is_some_and(|f| f.eq_ignore_ascii_case(&a.name));
        let mut bx = px;
        if !ids.is_empty() {
            bx += pill(cv, hits, tl, bx, *y, "► IJRO ETISH", true, Act::PlayAll(ids.clone(), false), body) + 10.0;
            bx += pill(cv, hits, tl, bx, *y, "⤮ ARALASH", false, Act::PlayAll(ids.clone(), true), body) + 10.0;
        }
        pill(cv, hits, tl, bx, *y, if fav { "★ SEVIMLI IJROCHI ✓" } else { "☆ SEVIMLI IJROCHI" }, fav, Act::Fav(a.name.clone()), body);
        *y += 58.0;

        // ── who they are (the first paragraph; BATAFSIL opens the whole story and the best-known songs)
        if !a.bio.is_empty() || !a.known.is_empty() {
            let y0 = *y;
            label(cv, px + 16.0, y, "IJROCHI HAQIDA", body);
            let meta: Vec<&str> = [a.genre.as_str(), a.origin.as_str(), a.since.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
            if !meta.is_empty() {
                let mut mx = px + 16.0;
                for m in meta {
                    mx += chip(cv, hits, tl, mx, *y, m, false, None, body) + 6.0;
                }
                *y += 40.0;
            }
            let paras: Vec<&String> = if self.bio { a.bio.iter().collect() } else { a.bio.iter().take(1).collect() };
            for para in paras {
                let t = Txt::new(para.clone(), text::MR, 15.0, px + 16.0, *y, c8(0xd6dcc0, 1.0)).weight(500).wrap(w - 110.0).line(22.0).clip(body);
                let (_, h) = tl.measure2(&t);
                cv.text(t);
                *y += h + 10.0;
            }
            if self.bio && !a.known.is_empty() {
                label(cv, px + 16.0, y, "ENG MASHHUR QO'SHIQLARI", body);
                let mut kx = px + 16.0;
                for k in &a.known {
                    let kw = tl.measure(&Txt::new(format!("♪ {k}"), text::MR, 14.0, 0.0, 0.0, [0; 4]).weight(600)) + 24.0;
                    if kx + kw > x1 - 50.0 {
                        kx = px + 16.0;
                        *y += 36.0;
                    }
                    cv.flat(Quad::flat(kx, *y, kw, 28.0, rgba(0xc4e078, 0.1)).rounded(14.0));
                    cv.text(Txt::new(format!("♪ {k}"), text::MR, 14.0, kx + 12.0, *y + 5.0, c8(0xe6ff9a, 1.0)).weight(600).clip(body));
                    kx += kw + 8.0;
                }
                *y += 40.0;
            }
            if self.bio {
                if let Some(src) = &a.picture_source {
                    cv.text(Txt::new(format!("Rasm: {src}"), text::MONO, 11.0, px + 16.0, *y, c8(0x6b7350, 1.0)).clip(body));
                    *y += 20.0;
                }
            }
            if a.bio.len() > 1 || !a.known.is_empty() || a.bio.first().is_some_and(|b| b.chars().count() > 180) {
                chip(cv, hits, tl, px + 16.0, *y, if self.bio { "YIG'ISH ▴" } else { "BATAFSIL ▾" }, false, Some(Act::Bio), body);
                *y += 40.0;
            }
            // the card: a lit edge down its left side
            cv.flat(Quad::flat(px, y0 + 2.0, 3.0, *y - y0 - 10.0, rgba(0xc4e078, 0.45)).rounded(1.5));
            *y += 10.0;
        }

        // ── MASHHUR: the songs played most on Kawaii Radio
        if !all.is_empty() {
            label(cv, px, y, "MASHHUR", body);
            let shown: Vec<&Track> = all.iter().take(if self.more { 10 } else { 5 }).copied().collect();
            numbered(cv, shown, y, hits, cx, img, g);
            if all.len() > 5 {
                chip(cv, hits, tl, px, *y, &if self.more { "KAMROQ ▴".to_string() } else { format!("YANA {} TA ▾", all.len().min(10) - 5) }, false, Some(Act::More), body);
                *y += 44.0;
            }
            *y += 6.0;
        }

        // ── DISKOGRAFIYA: shelves of covers (the ones with most of our songs, albums, singles and EPs)
        if let Some(d) = a.deezer {
            match cx.discos.get(&d) {
                Some(Some(disco)) if !disco.releases.is_empty() => {
                    label(cv, px, y, "DISKOGRAFIYA", body);
                    let all_t = format!("HAMMASI · {} ›", disco.releases.len());
                    let aw = tl.measure(&Txt::new(all_t.clone(), text::VT, 20.0, 0.0, 0.0, [0; 4]));
                    cv.text(Txt::new(all_t, text::VT, 20.0, x1 - 50.0, *y - 26.0, c8(0xc4e078, 1.0)).align(Align::Right).clip(body));
                    hits.push(([x1 - 60.0 - aw, *y - 30.0, x1 - 40.0, *y - 2.0], Act::Discography(a.id.clone())));
                    let mut cxp = px;
                    for (k, l) in ["MASHHUR", "ALBOMLAR", "SINGL VA EP"].iter().enumerate() {
                        let has = match k {
                            1 => disco.releases.iter().any(|r| r.kind == "album"),
                            2 => disco.releases.iter().any(|r| r.kind != "album"),
                            _ => true,
                        };
                        if has {
                            cxp += chip(cv, hits, tl, cxp, *y, l, self.shelf == k as u8, Some(Act::Shelf(k as u8)), body) + 6.0;
                        }
                    }
                    *y += 44.0;
                    let mut list: Vec<&crate::artists::Release> = disco
                        .releases
                        .iter()
                        .filter(|r| match self.shelf {
                            1 => r.kind == "album",
                            2 => r.kind != "album",
                            _ => true,
                        })
                        .collect();
                    if self.shelf == 0 {
                        list.sort_by_key(|r| (std::cmp::Reverse(r.ours().len()), r.kind != "album"));
                        list.truncate(10);
                    }
                    for (k, r) in list.iter().enumerate() {
                        let (col, row) = (k % 5, k / 5);
                        release_card(cv, img, hits, px + col as f32 * 134.0, *y + row as f32 * 186.0, r, &a.id, body);
                    }
                    *y += list.len().div_ceil(5) as f32 * 186.0 + 4.0;
                }
                Some(Some(_)) => {}
                Some(None) => {
                    label(cv, px, y, "DISKOGRAFIYA", body);
                    cv.text(Txt::new("yuklanmoqda…", text::MONO, 13.0, px, *y, c8(0x8f9a6a, 1.0)).clip(body));
                    *y += 30.0;
                }
                None => self.want_disco = Some(d),
            }
        }

        // ── every song
        if all.len() > 5 {
            let t = format!("☰ BARCHA QO'SHIQLARI · {}", all.len());
            pill(cv, hits, tl, px, *y, &t, false, Act::ArtistSongs(a.id.clone()), body);
            *y += 54.0;
        }
        if all.is_empty() {
            cv.text(Txt::new("Hozircha bu ijrochining qo'shig'i efirda yo'q.", text::MONO, 14.0, px, *y, c8(0x8f9a6a, 1.0)).clip(body));
            *y += 30.0;
        }
    }

    /// Every release of an artist, newest first: all, albums, or singles and EPs.
    #[allow(clippy::too_many_arguments)]
    fn disco_page(&mut self, cv: &mut Canvas, tl: &mut text::TextLayer, cx: &Ctx, img: &mut dyn FnMut(&str) -> Option<usize>, hits: &mut Vec<([f32; 4], Act)>, y: &mut f32, aid: &str, g: (f32, f32, f32, [f32; 4], f32)) {
        let (px, _w, x1, body, _) = g;
        let Some(a) = cx.artists.iter().find(|a| a.id == aid) else { return };
        let Some(Some(disco)) = a.deezer.and_then(|d| cx.discos.get(&d)) else {
            cv.text(Txt::new("Yuklanmoqda…", text::MONO, 14.0, px, *y, c8(0x8f9a6a, 1.0)).clip(body));
            *y += 30.0;
            return;
        };
        cv.text(Txt::new(a.name.clone(), text::MR, 22.0, px, *y, c8(0xe6ff9a, 1.0)).weight(800).clip(body));
        *y += 38.0;
        let mut bx = px;
        for (k, l) in [format!("HAMMASI · {}", disco.releases.len()), "ALBOMLAR".into(), "SINGL VA EP".into()].iter().enumerate() {
            bx += chip(cv, hits, tl, bx, *y, l, self.shelf == k as u8, Some(Act::Shelf(k as u8)), body) + 6.0;
        }
        *y += 46.0;
        for r in disco.releases.iter().filter(|r| match self.shelf {
            1 => r.kind == "album",
            2 => r.kind != "album",
            _ => true,
        }) {
            if *y + 72.0 > body[1] - 10.0 && *y < body[3] + 10.0 {
                cv.flat(Quad::flat(px - 6.0, *y - 4.0, x1 - 30.0 - px, 72.0, rgba(0xc4e078, 0.05)).rounded(10.0));
                match r.thumb.as_ref().or(r.cover.as_ref()).and_then(|u| img(u)) {
                    Some(t) => cv.quad(Some(t), Quad { rect: [px, *y, 64.0, 64.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 8.0, ..Quad::full() }),
                    None => cv.flat(Quad::flat(px, *y, 64.0, 64.0, rgb(0x1d2a20)).rounded(8.0)),
                }
                let cl = [body[0], body[1].max(*y - 4.0), x1 - 60.0, body[3]];
                cv.text(Txt::new(r.title.clone(), text::MR, 16.0, px + 80.0, *y + 4.0, c8(0xe6ff9a, 1.0)).weight(700).clip(cl));
                let mut sub = vec![r.year.map(|v| v.to_string()).unwrap_or_default(), r.kind_uz().to_string()];
                if r.kind != "single" {
                    sub.push(format!("{} ta qo'shiq", r.tracks.len()));
                }
                cv.text(Txt::new(sub.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "), text::MR, 13.0, px + 80.0, *y + 26.0, c8(0x8f9a6a, 1.0)).weight(500).clip(cl));
                let ours = r.ours().len();
                if ours > 0 {
                    led(cv, px + 85.0, *y + 54.0);
                    cv.text(Txt::new(format!("KAWAII RADIO'DA: {ours}"), text::MONO, 11.0, px + 96.0, *y + 47.0, c8(0xc4e078, 1.0)).weight(700).clip(cl));
                }
                cv.text(Txt::new("›", text::VT, 30.0, x1 - 50.0, *y + 14.0, c8(0x9fb070, 1.0)).clip(body));
                hits.push(([px - 6.0, (*y - 4.0).max(body[1]), x1 - 30.0, (*y + 68.0).min(body[3])], Act::OpenAlbum(a.id.clone(), r.id.clone())));
            }
            *y += 80.0;
        }
    }

    /// One release: its cover, what it is, the track list (our songs lit and playable).
    #[allow(clippy::too_many_arguments)]
    fn album_page(&mut self, cv: &mut Canvas, cx: &Ctx, img: &mut dyn FnMut(&str) -> Option<usize>, hits: &mut Vec<([f32; 4], Act)>, y: &mut f32, aid: &str, rid: &str, g: (f32, f32, f32, [f32; 4], f32)) {
        let (px, _w, x1, body, _) = g;
        let Some(a) = cx.artists.iter().find(|a| a.id == aid) else { return };
        let Some(r) = a.deezer.and_then(|d| cx.discos.get(&d)).and_then(|d| d.as_ref()).and_then(|d| d.releases.iter().find(|r| r.id == rid)) else { return };
        match r.cover.as_ref().or(r.thumb.as_ref()).and_then(|u| img(u)) {
            Some(t) => cv.quad(Some(t), Quad { rect: [px, *y, 140.0, 140.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 10.0, ..Quad::full() }),
            None => cv.flat(Quad::flat(px, *y, 140.0, 140.0, rgb(0x1d2a20)).rounded(10.0)),
        }
        let tx = px + 158.0;
        cv.text(Txt::new(r.title.clone(), text::MR, 24.0, tx, *y + 4.0, c8(0xe6ff9a, 1.0)).weight(800).wrap(x1 - 50.0 - tx).line(30.0).clip(body));
        cv.text(Txt::new(a.name.clone(), text::MR, 15.0, tx, *y + 72.0, c8(0xc9d79a, 1.0)).weight(600).clip(body));
        let total: i64 = r.tracks.iter().map(|t| t.duration).sum();
        let date = r.date.clone().or(r.year.map(|v| v.to_string())).unwrap_or_default();
        cv.text(Txt::new(format!("{} · {} · {} ta trek · {} daq", r.kind_uz(), date, r.tracks.len(), total / 60), text::MONO, 13.0, tx, *y + 98.0, c8(0x8f9a6a, 1.0)).clip(body));
        *y += 160.0;
        for t in &r.tracks {
            if *y + 36.0 > body[1] - 10.0 && *y < body[3] + 10.0 {
                let ours = t.track.as_ref().filter(|id| cx.tracks.contains_key(*id));
                let now = ours.is_some_and(|id| cx.now == Some(id.as_str()));
                if now {
                    cv.flat(Quad::flat(px - 8.0, *y - 4.0, x1 - 30.0 - px, 34.0, rgba(0xc4e078, 0.08)).rounded(6.0));
                }
                let ink = if ours.is_some() { 0xe6ff9a } else { 0x6f7856 };
                cv.text(Txt::new(format!("{:02}", t.n), text::VT, 22.0, px, *y, c8(0x7c8456, 1.0)).clip(body));
                cv.text(Txt::new(t.title.clone(), text::MR, 15.0, px + 40.0, *y + 3.0, c8(ink, 1.0)).weight(if ours.is_some() { 700 } else { 500 }).clip([body[0], body[1], x1 - 140.0, body[3]]));
                if ours.is_some() {
                    cv.text(Txt::new("► EFIRDA", text::VT, 18.0, x1 - 130.0, *y + 2.0, c8(0xc4e078, 0.9)).clip(body));
                }
                cv.text(Txt::new(format!("{}:{:02}", t.duration / 60, t.duration % 60), text::VT, 22.0, x1 - 52.0, *y, c8(0x9fb070, 1.0)).align(Align::Right).clip(body));
                if let Some(id) = ours {
                    hits.push(([px - 8.0, (*y - 4.0).max(body[1]), x1 - 30.0, (*y + 30.0).min(body[3])], Act::Play(id.clone())));
                }
            }
            *y += 36.0;
        }
        *y += 10.0;
        cv.text(Txt::new("Ma'lumot: Deezer", text::MONO, 11.0, px, *y, c8(0x6b7350, 1.0)).clip(body));
        *y += 24.0;
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
fn rows(cv: &mut Canvas, list: Vec<&Track>, y: &mut f32, hits: &mut Vec<([f32; 4], Act)>, empty: &str, cx: &Ctx, img: &mut dyn FnMut(&str) -> Option<usize>, g: (f32, f32, f32, [f32; 4], f32)) {
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

/// A small section label (y moves past it).
fn label(cv: &mut Canvas, x: f32, y: &mut f32, t: &str, body: [f32; 4]) {
    cv.text(Txt::new(t, text::MONO, 12.0, x, *y, c8(0xa9b97a, 1.0)).weight(700).clip(body));
    *y += 24.0;
}

/// 1234 → "1 234", 15300 → "15,3 ming"
fn count(n: u64) -> String {
    if n >= 10_000 {
        format!("{},{} ming", n / 1000, (n % 1000) / 100)
    } else if n >= 1000 {
        format!("{} {:03}", n / 1000, n % 1000)
    } else {
        n.to_string()
    }
}

/// A round key on the tube: lit (filled) or an outline. Returns its width.
#[allow(clippy::too_many_arguments)]
fn pill(cv: &mut Canvas, hits: &mut Vec<([f32; 4], Act)>, tl: &mut text::TextLayer, x: f32, y: f32, t: &str, lit: bool, a: Act, clip: [f32; 4]) -> f32 {
    let w = tl.measure(&Txt::new(t, text::VT, 22.0, 0.0, 0.0, [0; 4])) + 34.0;
    if lit {
        cv.flat(Quad::grad(x, y, w, 38.0, rgb(0xe2f7a2), rgb(0xa9c862), true).rounded(19.0));
        cv.flat(Quad::flat(x + 6.0, y + 2.0, w - 12.0, 1.5, rgba(0xffffff, 0.45)).rounded(1.0));
    } else {
        cv.flat(Quad::flat(x, y, w, 38.0, rgba(0xc4e078, 0.42)).rounded(19.0));
        cv.flat(Quad::grad(x + 1.5, y + 1.5, w - 3.0, 35.0, rgb(0x172014), rgb(0x0d120b), true).rounded(17.5));
    }
    cv.text(Txt::new(t, text::VT, 22.0, x + 17.0, y + 6.0, c8(if lit { 0x18210f } else { 0xe6ff9a }, 1.0)).clip(clip));
    hits.push(([x, y.max(clip[1]), x + w, (y + 38.0).min(clip[3])], a));
    w
}

/// A small chip (a tab when `on`, a tag when it has no action). Returns its width.
#[allow(clippy::too_many_arguments)]
fn chip(cv: &mut Canvas, hits: &mut Vec<([f32; 4], Act)>, tl: &mut text::TextLayer, x: f32, y: f32, t: &str, on: bool, a: Option<Act>, clip: [f32; 4]) -> f32 {
    let w = tl.measure(&Txt::new(t, text::VT, 20.0, 0.0, 0.0, [0; 4])) + 24.0;
    if on {
        cv.flat(Quad::flat(x, y, w, 30.0, rgba(0xc4e078, 0.88)).rounded(15.0));
    } else {
        cv.flat(Quad::flat(x, y, w, 30.0, rgba(0xc4e078, if a.is_some() { 0.3 } else { 0.16 })).rounded(15.0));
        cv.flat(Quad::flat(x + 1.0, y + 1.0, w - 2.0, 28.0, rgb(0x10160d)).rounded(14.0));
    }
    cv.text(Txt::new(t, text::VT, 20.0, x + 12.0, y + 4.0, c8(if on { 0x18210f } else { 0xd6e8a0 }, 1.0)).clip(clip));
    if let Some(a) = a {
        hits.push(([x, y.max(clip[1]), x + w, (y + 30.0).min(clip[3])], a));
    }
    w
}

/// The LCD strip of the phone's pages: dark ink on lit green glass.
fn lcd_plate(cv: &mut Canvas, tl: &mut text::TextLayer, x: f32, y: f32, t: &str, clip: [f32; 4]) {
    let w = tl.measure(&Txt::new(t, text::VT, 20.0, 0.0, 0.0, [0; 4])) + 28.0;
    cv.flat(Quad::flat(x - 2.0, y - 2.0, w + 4.0, 34.0, rgb(0x2a3320)).rounded(8.0));
    cv.flat(Quad::grad(x, y, w, 30.0, rgb(0xd3e3a2), rgb(0xb4c886), true).rounded(6.0));
    cv.text(Txt::new(t, text::VT, 20.0, x + 15.0, y + 5.0, c8(0x3a4a1c, 0.25)).clip(clip));
    cv.text(Txt::new(t, text::VT, 20.0, x + 14.0, y + 4.0, c8(0x26321a, 1.0)).clip(clip));
}

/// A lit LED dot.
fn led(cv: &mut Canvas, x: f32, y: f32) {
    cv.flat(Quad::flat(x - 6.0, y - 6.0, 12.0, 12.0, rgba(0x9dff6a, 0.18)).rounded(6.0));
    cv.flat(Quad::flat(x - 3.5, y - 3.5, 7.0, 7.0, rgb(0x9dff6a)).rounded(3.5));
    cv.flat(Quad::flat(x - 1.5, y - 2.5, 3.0, 2.0, rgba(0xffffff, 0.7)).rounded(1.0));
}

/// An album / single as a cover with its name and year; a lit LED tells it has songs on Kawaii Radio.
#[allow(clippy::too_many_arguments)]
fn release_card(cv: &mut Canvas, img: &mut dyn FnMut(&str) -> Option<usize>, hits: &mut Vec<([f32; 4], Act)>, x: f32, y: f32, r: &crate::artists::Release, aid: &str, body: [f32; 4]) {
    if y + 186.0 < body[1] - 10.0 || y > body[3] + 10.0 {
        return;
    }
    cv.flat(Quad::flat(x - 3.0, y - 3.0, 124.0, 124.0, rgba(0xc4e078, 0.16)).rounded(12.0));
    match r.thumb.as_ref().or(r.cover.as_ref()).and_then(|u| img(u)) {
        Some(t) => cv.quad(Some(t), Quad { rect: [x, y, 118.0, 118.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 10.0, ..Quad::full() }),
        None => cv.flat(Quad::flat(x, y, 118.0, 118.0, rgb(0x1d2a20)).rounded(10.0)),
    }
    let cl = [x, body[1].max(y), x + 122.0, body[3]];
    cv.text(Txt::new(r.title.clone(), text::MR, 13.0, x, y + 126.0, c8(0xe6ff9a, 1.0)).weight(700).wrap(120.0).line(16.0).clip([cl[0], cl[1], cl[2], (y + 160.0).min(body[3])]));
    let ours = !r.ours().is_empty();
    let sub = [r.year.map(|v| v.to_string()).unwrap_or_default(), r.kind_uz().to_string()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
    if ours {
        led(cv, x + 5.0, y + 170.0);
    }
    cv.text(Txt::new(sub, text::MR, 11.0, x + if ours { 14.0 } else { 0.0 }, y + 162.0, c8(0x8f9a6a, 1.0)).weight(500).clip(cl));
    hits.push(([x - 3.0, y.max(body[1]), x + 122.0, (y + 180.0).min(body[3])], Act::OpenAlbum(aid.to_string(), r.id.clone())));
}

/// Numbered song rows (an artist's top list): number, cover, title, how often everybody played it, length.
#[allow(clippy::too_many_arguments)]
fn numbered(cv: &mut Canvas, list: Vec<&Track>, y: &mut f32, hits: &mut Vec<([f32; 4], Act)>, cx: &Ctx, img: &mut dyn FnMut(&str) -> Option<usize>, g: (f32, f32, f32, [f32; 4], f32)) {
    let (px, w, x1, body, _) = g;
    let row_h = 56.0;
    for (i, t) in list.into_iter().enumerate() {
        if *y + row_h > body[1] - 10.0 && *y < body[3] + 10.0 {
            let now = cx.now == Some(t.id.as_str());
            cv.flat(Quad::flat(px - 6.0, *y - 3.0, w - 56.0, row_h - 6.0, rgba(0xc4e078, if now { 0.12 } else { 0.045 })).rounded(10.0));
            cv.text(Txt::new(format!("{}", i + 1), text::VT, 26.0, px + 14.0, *y + 8.0, c8(if now { 0xf2e49c } else { 0x8f9a6a }, 1.0)).align(Align::Center).clip(body));
            if let Some(c) = img(&crate::net::url(&t.cover_art)) {
                cv.quad(Some(c), Quad { rect: [px + 34.0, *y + 2.0, 42.0, 42.0], uv: [0.0, 0.0, 1.0, 1.0], radius: 6.0, ..Quad::full() });
            } else {
                cv.flat(Quad::flat(px + 34.0, *y + 2.0, 42.0, 42.0, rgb(0x1d2a20)).rounded(6.0));
            }
            let cl = [body[0], body[1].max(*y - 4.0), x1 - 110.0, body[3]];
            cv.text(Txt::new(t.title.clone(), text::MR, 16.0, px + 90.0, *y + 3.0, c8(if now { 0xf2e49c } else { 0xe6ff9a }, 1.0)).weight(700).clip(cl));
            let sub = match cx.plays.get(&t.id) {
                Some(n) if *n > 0 => format!("{} marta tinglangan", count(*n)),
                _ => t.artist.clone(),
            };
            cv.text(Txt::new(sub, text::MR, 13.0, px + 90.0, *y + 25.0, c8(0x8f9a6a, 1.0)).weight(500).clip(cl));
            if now {
                led(cv, x1 - 104.0, *y + 22.0);
            }
            let d = t.duration.max(0.0) as u64;
            cv.text(Txt::new(format!("{}:{:02}", d / 60, d % 60), text::VT, 22.0, x1 - 56.0, *y + 10.0, c8(0x9fb070, 1.0)).align(Align::Right).clip(body));
            hits.push(([px - 6.0, (*y - 3.0).max(body[1]), x1 - 34.0, (*y + row_h - 9.0).min(body[3])], Act::Play(t.id.clone())));
        }
        *y += row_h;
    }
}
