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

/// This computer's own id (Windows: MachineGuid, Linux: /etc/machine-id): the files are scrambled with it too, so a
/// copy of the folder (and of the settings with the install key) does not play on another computer.
fn machine() -> Vec<u8> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let o = std::process::Command::new("reg")
            .args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
            .creation_flags(0x0800_0000)
            .output()
            .ok();
        o.map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().last().unwrap_or("").as_bytes().to_vec()).unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        std::fs::read("/etc/machine-id").or_else(|_| std::fs::read("/var/lib/dbus/machine-id")).unwrap_or_default()
    }
}

/// Files made from 0.8.3 on start with this mark (install key + machine); older ones used the install key alone.
const MARK: &[u8; 4] = b"KRS2";

fn key2(key: &[u8]) -> Vec<u8> {
    let mut k = key.to_vec();
    k.extend(machine());
    k
}

pub fn has(id: &str) -> bool {
    path(id).exists()
}

pub fn save(key: &[u8], id: &str, mut mp3: Vec<u8>) -> std::io::Result<()> {
    crypt(&key2(key), id, &mut mp3);
    let tmp = path(id).with_extension("part");
    let mut out = MARK.to_vec();
    out.extend(mp3);
    std::fs::write(&tmp, &out)?;
    std::fs::rename(tmp, path(id))
}

pub fn load(key: &[u8], id: &str) -> Option<Vec<u8>> {
    let d = std::fs::read(path(id)).ok()?;
    match d.strip_prefix(MARK) {
        Some(body) => {
            let mut b = body.to_vec();
            crypt(&key2(key), id, &mut b);
            Some(b)
        }
        None => {
            let mut b = d;
            crypt(key, id, &mut b);
            Some(b)
        }
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip() {
        let id = "zz-vault-test";
        let mp3 = b"ID3 not really an mp3, but bytes all the same".to_vec();
        super::save(b"install-key", id, mp3.clone()).unwrap();
        let raw = std::fs::read(super::path(id)).unwrap();
        assert!(raw.starts_with(super::MARK) && raw[4..] != mp3[..]);
        assert_eq!(super::load(b"install-key", id).unwrap(), mp3);
        assert_ne!(super::load(b"other-key", id).unwrap(), mp3);
        // a file from before the machine binding still opens
        let mut old = mp3.clone();
        super::crypt(b"install-key", id, &mut old);
        std::fs::write(super::path(id), &old).unwrap();
        assert_eq!(super::load(b"install-key", id).unwrap(), mp3);
        super::remove(id);
        assert!(!super::machine().is_empty());
    }
}
