//! Updates: the same signed manifest the first (Tauri) version reads (dist/latest.json on GitHub, minisign
//! signatures by the key kept outside the repo), so both versions update from one place — the old one installs this
//! app as its update. A new version is downloaded, its signature checked, then installed:
//! Windows — the NSIS installer runs silently and starts the app again; Linux — the AppImage replaces itself.

use serde::Deserialize;
use std::collections::HashMap;

const MANIFEST: &str = "https://raw.githubusercontent.com/soka8imokenp/kawaii_radio_desktop/main/dist/latest.json";
/// The updater's public key (minisign, base64 of the key file), the same as in app/tauri.conf.json.
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEFGQUIzMjQ0NzkzOUYwRDMKUldUVDhEbDVSREtycjZUNDNxdC9IUENtOHFxTTc1aFpuYWVkRTZ4NHRvNDdxSm9lMHp0eS9mUzkK";

#[derive(Deserialize, Clone, Debug)]
struct Platform {
    signature: String,
    url: String,
}

#[derive(Deserialize, Clone, Debug)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    platforms: HashMap<String, Platform>,
}

#[derive(Clone, Debug)]
pub struct Update {
    pub version: String,
    pub notes: String,
    url: String,
    sig: String,
}

fn parse_ver(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-']).map(|x| x.parse().unwrap_or(0)).collect()
}

/// Is `a` newer than `b`?
pub fn newer(a: &str, b: &str) -> bool {
    let (x, y) = (parse_ver(a), parse_ver(b));
    for i in 0..x.len().max(y.len()) {
        let (p, q) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if p != q {
            return p > q;
        }
    }
    false
}

/// Which build this copy is: the Windows installer, the Linux AppImage, or the Linux .deb (each signed).
fn platform_key() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else if std::env::var_os("APPIMAGE").is_some() {
        "linux-x86_64"
    } else {
        "linux-x86_64-deb"
    }
}

/// A newer version for this system, if there is one.
pub fn check() -> Option<Update> {
    let m: Manifest = crate::net::agent().get(MANIFEST).call().ok()?.into_json().ok()?;
    if !newer(&m.version, env!("CARGO_PKG_VERSION")) {
        return None;
    }
    let p = m.platforms.get(platform_key())?;
    Some(Update { version: m.version, notes: m.notes, url: p.url.clone(), sig: p.signature.clone() })
}

fn b64(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = vec![];
    let (mut acc, mut n) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = T.iter().position(|x| *x == c)? as u32;
        acc = (acc << 6) | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((acc >> n) as u8);
            acc &= (1 << n) - 1;
        }
    }
    Some(out)
}

/// Is `data` signed by our key?
fn verify(data: &[u8], sig_b64: &str) -> Result<(), String> {
    let pk_text = String::from_utf8(b64(PUBKEY).ok_or("kalit")?).map_err(|e| e.to_string())?;
    let pk = minisign_verify::PublicKey::decode(&pk_text).map_err(|e| format!("kalit: {e}"))?;
    let sig_text = String::from_utf8(b64(sig_b64).ok_or("imzo")?).map_err(|e| e.to_string())?;
    let sig = minisign_verify::Signature::decode(&sig_text).map_err(|e| format!("imzo: {e}"))?;
    pk.verify(data, &sig, false).map_err(|e| format!("imzo noto'g'ri: {e}"))
}

/// Download, check, install; on success the app quits (the new version starts by itself).
pub fn install(u: &Update) -> Result<(), String> {
    let data = crate::net::bytes(&u.url)?;
    verify(&data, &u.sig)?;
    #[cfg(windows)]
    {
        let path = std::env::temp_dir().join(format!("KawaiiRadio-{}-setup.exe", u.version));
        std::fs::write(&path, &data).map_err(|e| e.to_string())?;
        std::process::Command::new(&path).args(["/S", "/UPDATE"]).spawn().map_err(|e| e.to_string())?;
        std::process::exit(0);
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        // an AppImage replaces its own file; a .deb install gets the package through the system's installer
        if let Some(app) = std::env::var_os("APPIMAGE") {
            let app = std::path::PathBuf::from(app);
            let tmp = app.with_extension("new");
            std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
            std::fs::rename(&tmp, &app).map_err(|e| e.to_string())?;
            let _ = std::process::Command::new(&app).spawn();
            std::process::exit(0);
        }
        let path = std::env::temp_dir().join(format!("kawaii-radio_{}_amd64.deb", u.version));
        std::fs::write(&path, &data).map_err(|e| e.to_string())?;
        let ok = std::process::Command::new("pkexec").args(["apt-get", "install", "-y"]).arg(&path).status().is_ok_and(|s| s.success());
        if !ok {
            return Err("o'rnatib bo'lmadi".into());
        }
        if let Ok(me) = std::env::current_exe() {
            let _ = std::process::Command::new(me).spawn();
        }
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    /// The installer in dist/ verifies against the manifest's signature (run after release.py).
    #[test]
    fn dist_signature() {
        let m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("../dist/latest.json").unwrap()).unwrap();
        let p = &m["platforms"]["windows-x86_64"];
        let name = p["url"].as_str().unwrap().rsplit('/').next().unwrap();
        let data = std::fs::read(format!("../dist/{name}")).unwrap();
        super::verify(&data, p["signature"].as_str().unwrap()).unwrap();
        assert!(super::verify(&data[1..], p["signature"].as_str().unwrap()).is_err());
        assert!(super::newer("0.8.0", "0.7.0") && !super::newer("0.7.0", "0.8.0") && super::newer("0.10.0", "0.9.9"));
    }
}
