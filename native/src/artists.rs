//! Artist pages, like the phone's (Qidiruv → Ijrochilar, Kutubxona → Sevimli ijrochilar): texts written in Uzbek and
//! kept in the cloud (files/data/artists.json), discographies from Deezer (files/data/disco-<deezer>.json).
//! Both are kept on disk for the next start and for offline use.

use crate::net::Track;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub picture: Option<String>,
    pub thumb: Option<String>,
    #[serde(rename = "pictureSource")]
    pub picture_source: Option<String>,
    pub genre: String,
    pub origin: String,
    pub since: String,
    pub tagline: String,
    pub bio: Vec<String>,
    pub known: Vec<String>,
    pub deezer: Option<i64>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Disco {
    pub releases: Vec<Release>,
    /// our songs of this artist on none of the releases
    pub loose: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Release {
    pub id: String,
    pub title: String,
    /// album | ep | single
    #[serde(rename = "type")]
    pub kind: String,
    pub year: Option<i64>,
    pub date: Option<String>,
    pub cover: Option<String>,
    pub thumb: Option<String>,
    pub tracks: Vec<RTrack>,
}

impl Release {
    pub fn kind_uz(&self) -> &'static str {
        match self.kind.as_str() {
            "single" => "Singl",
            "ep" => "EP",
            _ => "Albom",
        }
    }
    /// Our songs on it, in its order.
    pub fn ours(&self) -> Vec<String> {
        let mut v: Vec<String> = vec![];
        for t in &self.tracks {
            if let Some(id) = &t.track {
                if !v.contains(id) {
                    v.push(id.clone());
                }
            }
        }
        v
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct RTrack {
    pub n: i64,
    pub title: String,
    pub duration: i64,
    pub track: Option<String>,
}

/// A name, normalised: lower case, letters and digits only.
pub fn key(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The first artist of a song credit, as written ("A feat. B & C" → "A").
pub fn main_artist(credit: &str) -> String {
    let low = credit.to_lowercase();
    let cut = [" feat. ", " feat ", " ft. ", " ft ", " & ", " x ", "/", ","].iter().filter_map(|s| low.find(s)).min().unwrap_or(credit.len());
    credit.get(..cut).unwrap_or(credit).trim().to_string()
}

/// The artists named in a song credit ("A, B feat. C & D"), normalised.
pub fn credit_keys(credit: &str) -> Vec<String> {
    let mut s = format!(" {} ", credit.to_lowercase());
    for sep in [" feat. ", " feat ", " ft. ", " ft ", " & ", " x ", "/", ","] {
        s = s.replace(sep, "|");
    }
    s.split('|').map(key).filter(|k| !k.is_empty()).collect()
}

impl Artist {
    pub fn keys(&self) -> Vec<String> {
        self.aliases.iter().chain(std::iter::once(&self.name)).map(|a| key(a)).collect()
    }
    pub fn sings(&self, credit: &str) -> bool {
        let k = self.keys();
        credit_keys(credit).iter().any(|c| k.contains(c))
    }
    /// Our songs of this artist, by title.
    pub fn songs(&self, tracks: &HashMap<String, Track>) -> Vec<Track> {
        let mut v: Vec<Track> = tracks.values().filter(|t| self.sings(&t.artist)).cloned().collect();
        v.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        v
    }
}

fn dir() -> std::path::PathBuf {
    let d = crate::store::dir().join("data");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn parse_all(text: &str) -> Option<Vec<Artist>> {
    #[derive(Deserialize)]
    struct F {
        artists: Vec<Artist>,
    }
    serde_json::from_str::<F>(text).ok().map(|f| f.artists)
}

/// The artists saved last time (at once, offline too).
pub fn cached() -> Vec<Artist> {
    std::fs::read_to_string(dir().join("artists.json")).ok().and_then(|t| parse_all(&t)).unwrap_or_default()
}

/// The artists from the cloud (and saved for next time).
pub fn fetch() -> Option<Vec<Artist>> {
    let b = crate::net::bytes("/files/data/artists.json").ok()?;
    let t = String::from_utf8(b).ok()?;
    let a = parse_all(&t)?;
    let _ = std::fs::write(dir().join("artists.json"), &t);
    Some(a)
}

/// An artist's discography (fresh, or the last copy offline).
pub fn disco(deezer: i64) -> Option<Disco> {
    let path = dir().join(format!("disco-{deezer}.json"));
    let fresh = crate::net::bytes(&format!("/files/data/disco-{deezer}.json")).ok().and_then(|b| String::from_utf8(b).ok());
    let text = match &fresh {
        Some(t) => {
            let _ = std::fs::write(&path, t);
            t.clone()
        }
        None => std::fs::read_to_string(&path).ok()?,
    };
    serde_json::from_str(&text).ok()
}
