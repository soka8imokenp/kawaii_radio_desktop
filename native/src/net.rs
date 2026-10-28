//! radio.kawaii.uz: the track list and the stations' schedule (everybody hears the same second).

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const API: &str = "https://radio.kawaii.uz";

pub fn agent() -> ureq::Agent {
    let tls = native_tls::TlsConnector::new().expect("tls");
    ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .user_agent(concat!("KawaiiRadioNative/", env!("CARGO_PKG_VERSION")))
        .build()
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub cover_art: String,
    pub audio_src: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub bpm: f64,
    #[serde(default)]
    pub energy: f64,
    #[serde(default)]
    pub lyrics: Option<Lyrics>,
    #[serde(default)]
    pub info: Option<serde_json::Value>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Lyrics {
    #[serde(default)]
    pub uzbek: Option<String>,
    #[serde(default)]
    pub romaji: Option<String>,
    #[serde(default)]
    pub kanji: Option<String>,
}

/// One timed line of an LRC text.
#[derive(Clone, Debug)]
pub struct Line {
    pub t: f64,
    pub s: String,
}

pub fn parse_lrc(text: Option<&str>) -> Vec<Line> {
    let mut out = vec![];
    for raw in text.unwrap_or("").lines() {
        let raw = raw.trim();
        let Some(rest) = raw.strip_prefix('[') else { continue };
        let Some((tm, txt)) = rest.split_once(']') else { continue };
        let Some((m, sec)) = tm.split_once(':') else { continue };
        let (Ok(m), Ok(sec)) = (m.trim().parse::<f64>(), sec.trim().parse::<f64>()) else { continue };
        let txt = txt.trim();
        if !txt.is_empty() {
            out.push(Line { t: m * 60.0 + sec, s: txt.to_string() });
        }
    }
    out
}

#[derive(Deserialize, Clone, Debug)]
pub struct Slot {
    pub id: String,
    pub start: i64,
    pub duration: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleJson {
    server_time: i64,
    slots: Vec<Slot>,
}

pub struct Schedule {
    pub slots: Vec<Slot>,
    /// server clock - our clock (ms)
    pub skew: i64,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn url(path: &str) -> String {
    if path.starts_with("http") { path.to_string() } else { format!("{API}{path}") }
}

pub fn tracks() -> Result<HashMap<String, Track>, String> {
    let v: serde_json::Value = agent().get(&url("/playlist.json")).call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
    let list = if v.is_array() { v } else { v.get("tracks").cloned().unwrap_or_default() };
    let list: Vec<Track> = serde_json::from_value(list).map_err(|e| e.to_string())?;
    Ok(list.into_iter().map(|t| (t.id.clone(), t)).collect())
}

pub fn schedule(station: &str) -> Result<Schedule, String> {
    let t0 = now_ms();
    let j: ScheduleJson = agent().get(&url(&format!("/radio?station={station}"))).call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
    let t1 = now_ms();
    Ok(Schedule { slots: j.slots, skew: j.server_time - (t0 + t1) / 2 })
}

impl Schedule {
    pub fn live_now(&self) -> i64 {
        now_ms() + self.skew
    }
    /// The slot on air now and how far into it we are (ms).
    pub fn on_air(&self) -> Option<(&Slot, i64)> {
        let now = self.live_now();
        self.slots.iter().find(|s| s.start <= now && now < s.start + s.duration).map(|s| (s, now - s.start))
    }
}

pub fn bytes(path: &str) -> Result<Vec<u8>, String> {
    let r = agent().get(&url(path)).call().map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut r.into_reader(), &mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}
