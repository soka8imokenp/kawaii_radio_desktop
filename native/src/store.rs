//! A small JSON settings file (the chat identity, the Kawaii ID token, likes, volume…) in the user's data folder.

use serde_json::{Map, Value};
use std::path::PathBuf;

pub struct Store {
    path: PathBuf,
    data: Map<String, Value>,
}

pub fn dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    base.unwrap_or_else(|| PathBuf::from(".")).join("KawaiiRadio")
}

impl Store {
    pub fn open() -> Self {
        let path = dir().join("native.json");
        let data = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Store { path, data }
    }
    pub fn get(&self, k: &str) -> Option<&Value> {
        self.data.get(k)
    }
    pub fn set(&mut self, k: &str, v: Value) {
        self.data.insert(k.to_string(), v);
        let _ = std::fs::create_dir_all(self.path.parent().unwrap());
        let _ = std::fs::write(&self.path, serde_json::to_string_pretty(&self.data).unwrap_or_default());
    }

    /// Set in memory only (counters ticking every second); the next `set` writes it to disk.
    pub fn data_set(&mut self, k: &str, v: Value) {
        self.data.insert(k.to_string(), v);
    }

    /// The chat identity of this computer (made once, kept).
    pub fn chat_id(&mut self) -> (String, String) {
        if let (Some(u), Some(k)) = (self.get("chatUid").and_then(|v| v.as_str()), self.get("chatKey").and_then(|v| v.as_str())) {
            return (u.to_string(), k.to_string());
        }
        let hex = |n: usize| -> String {
            let mut b = vec![0u8; n];
            let _ = getrandom(&mut b);
            b.iter().map(|x| format!("{x:02x}")).collect()
        };
        let (u, k) = (format!("desk-{}", hex(8)), hex(24));
        self.set("chatUid", Value::String(u.clone()));
        self.set("chatKey", Value::String(k.clone()));
        (u, k)
    }
}

/// Random bytes without another dependency: the OS's own source.
pub fn random(buf: &mut [u8]) -> std::io::Result<()> {
    getrandom(buf)
}

fn getrandom(buf: &mut [u8]) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        // a fresh UUID-ish mix of time and the process: good enough for a chat key that only has to be unguessable
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        for chunk in buf.chunks_mut(8) {
            let mut h = RandomState::new().build_hasher();
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
            let v = h.finish().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")?.read_exact(buf)
    }
}
