//! SAQLASH: songs kept on this computer for listening without the internet (Kawaii Pass). They are stored scrambled
//! with this install's own key, so the files are of no use outside the app (no player can open them, a copy to
//! another computer neither).

use std::path::PathBuf;

fn dir() -> PathBuf {
    let d = crate::store::dir().join("saved");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn path(id: &str) -> PathBuf {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    dir().join(format!("{safe}.krs"))
}

/// A keystream from the install key and the song (xoshiro256** seeded by a mix of both): XOR both ways.
fn crypt(key: &[u8], id: &str, data: &mut [u8]) {
    let mut st = [0x9e37_79b9_7f4a_7c15u64, 0xbf58_476d_1ce4_e5b9, 0x94d0_49bb_1331_11eb, 0x2545_f491_4f6c_dd1d];
    for (i, b) in key.iter().chain(id.as_bytes()).enumerate() {
        st[i % 4] = (st[i % 4] ^ *b as u64).wrapping_mul(0x100_0000_01b3).rotate_left(23);
    }
    let mut next = || {
        let r = st[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = st[1] << 17;
        st[2] ^= st[0];
        st[3] ^= st[1];
        st[1] ^= st[2];
        st[0] ^= st[3];
        st[2] ^= t;
        st[3] = st[3].rotate_left(45);
        r
    };
    for _ in 0..16 {
        next();
    }
    for chunk in data.chunks_mut(8) {
        let k = next().to_le_bytes();
        for (d, k) in chunk.iter_mut().zip(k) {
            *d ^= k;
        }
    }
}

pub fn has(id: &str) -> bool {
    path(id).exists()
}

pub fn save(key: &[u8], id: &str, mut mp3: Vec<u8>) -> std::io::Result<()> {
    crypt(key, id, &mut mp3);
    let tmp = path(id).with_extension("part");
    std::fs::write(&tmp, &mp3)?;
    std::fs::rename(tmp, path(id))
}

pub fn load(key: &[u8], id: &str) -> Option<Vec<u8>> {
    let mut d = std::fs::read(path(id)).ok()?;
    crypt(key, id, &mut d);
    Some(d)
}

pub fn remove(id: &str) {
    let _ = std::fs::remove_file(path(id));
}

/// The ids of the songs kept here.
pub fn list() -> Vec<String> {
    let mut v: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(dir())
        .map(|r| {
            r.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    let id = n.strip_suffix(".krs")?.to_string();
                    Some((e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), id))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.into_iter().rev().map(|x| x.1).collect()
}
