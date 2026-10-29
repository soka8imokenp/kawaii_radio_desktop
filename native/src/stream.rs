//! An mp3 played while it downloads: a Read + Seek over a file that arrives in 16 KB blocks. The downloader fills it
//! from wherever the player is (an HTTP Range request); a seek far from what has arrived restarts it there, so live
//! radio starts in the middle of a song at once and O'Z TANLOVIM can jump anywhere.

use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const BLK: u64 = 16 * 1024;
/// a seek this far past the download cursor restarts the download there
const NEAR: u64 = 384 * 1024;

struct Inner {
    data: Vec<u8>,
    have: Vec<bool>,
    /// where the downloader writes now, and where it should be (a new `epoch` restarts it there)
    cursor: u64,
    want: u64,
    epoch: u64,
    failed: Option<String>,
}

pub struct Shared {
    m: Mutex<Inner>,
    cv: Condvar,
    pub len: u64,
    url: String,
}

pub struct Stream {
    sh: Arc<Shared>,
    pos: u64,
}

fn total_of(r: &ureq::Response) -> Option<u64> {
    if let Some(cr) = r.header("Content-Range") {
        return cr.rsplit('/').next()?.trim().parse().ok();
    }
    r.header("Content-Length")?.parse().ok()
}

/// Open `url` for streaming: learns the length from the first response and keeps downloading in the background.
pub fn open(url: &str) -> Result<Stream, String> {
    let r = crate::net::agent().get(url).set("Range", "bytes=0-").call().map_err(|e| e.to_string())?;
    let len = total_of(&r).ok_or("uzunlik noma'lum")?;
    let nb = len.div_ceil(BLK) as usize;
    let sh = Arc::new(Shared {
        m: Mutex::new(Inner { data: vec![0; len as usize], have: vec![false; nb], cursor: 0, want: 0, epoch: 0, failed: None }),
        cv: Condvar::new(),
        len,
        url: url.to_string(),
    });
    let weak = Arc::downgrade(&sh);
    std::thread::spawn(move || download(weak, Some(r)));
    Ok(Stream { sh, pos: 0 })
}

/// The downloader: from `want` to the end, then whatever is still missing; it stops when nobody reads any more.
fn download(weak: std::sync::Weak<Shared>, mut first: Option<ureq::Response>) {
    let mut fails = 0;
    loop {
        let Some(sh) = weak.upgrade() else { return };
        // where to start: the first missing block from `want` on (or from the start)
        let (start, epoch) = {
            let mut g = sh.m.lock().unwrap();
            let from = (g.want / BLK) as usize;
            let miss = (from..g.have.len()).find(|&b| !g.have[b]).or_else(|| (0..from).find(|&b| !g.have[b]));
            match miss {
                Some(b) => {
                    g.cursor = b as u64 * BLK;
                    (g.cursor, g.epoch)
                }
                None => return,
            }
        };
        let resp = match first.take() {
            Some(r) if start == 0 => Ok(r),
            _ => crate::net::agent().get(&sh.url).set("Range", &format!("bytes={start}-")).call().map_err(|e| e.to_string()),
        };
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                fails += 1;
                if fails > 4 {
                    sh.m.lock().unwrap().failed = Some(e);
                    sh.cv.notify_all();
                    return;
                }
                drop(sh);
                std::thread::sleep(Duration::from_millis(600));
                continue;
            }
        };
        // a server that ignores Range sends the whole file from 0
        let mut at = if resp.status() == 206 { start } else { 0 };
        // blocks are written in order from here: one is complete when the write passes its end
        let mut next = at / BLK;
        let mut rd = resp.into_reader();
        let mut buf = vec![0u8; BLK as usize];
        drop(sh);
        loop {
            let n = match rd.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            let Some(sh) = weak.upgrade() else { return };
            let mut g = sh.m.lock().unwrap();
            let end = (at + n as u64).min(sh.len);
            g.data[at as usize..end as usize].copy_from_slice(&buf[..(end - at) as usize]);
            while next < g.have.len() as u64 && ((next + 1) * BLK).min(sh.len) <= end {
                g.have[next as usize] = true;
                next += 1;
            }
            at = end;
            g.cursor = at;
            let restart = g.epoch != epoch;
            drop(g);
            sh.cv.notify_all();
            if restart || at >= sh.len {
                break;
            }
        }
        fails = 0;
    }
}

impl Stream {
    /// Wait (on this thread) until `bytes` from the current position have arrived, or `max` passed.
    pub fn buffer(&self, bytes: u64, max: Duration) {
        let t0 = std::time::Instant::now();
        let mut g = self.sh.m.lock().unwrap();
        loop {
            let (b0, b1) = (self.pos / BLK, ((self.pos + bytes).min(self.sh.len.saturating_sub(1))) / BLK);
            if (b0..=b1).all(|b| g.have.get(b as usize).copied().unwrap_or(true)) || g.failed.is_some() || t0.elapsed() > max {
                return;
            }
            g = self.sh.cv.wait_timeout(g, Duration::from_millis(100)).unwrap().0;
        }
    }
    pub fn len(&self) -> u64 {
        self.sh.len
    }
}

impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.sh.len || out.is_empty() {
            return Ok(0);
        }
        let b = (self.pos / BLK) as usize;
        let mut g = self.sh.m.lock().unwrap();
        let mut waited = 0;
        while !g.have[b] {
            if let Some(e) = &g.failed {
                return Err(std::io::Error::other(e.clone()));
            }
            // the downloader is elsewhere: send it here. It is near when it is inside this block (the block is
            // being written) or a little before it; past the block's end, or far before it, it is not coming.
            let bs = b as u64 * BLK;
            let near = g.cursor <= bs + BLK && g.cursor + NEAR > bs;
            if !near && g.want != bs {
                g.want = b as u64 * BLK;
                g.epoch += 1;
            }
            g = self.sh.cv.wait_timeout(g, Duration::from_millis(250)).unwrap().0;
            waited += 1;
            if waited > 120 {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "stream stalled"));
            }
        }
        let end = ((b as u64 + 1) * BLK).min(self.sh.len);
        let n = ((end - self.pos) as usize).min(out.len());
        out[..n].copy_from_slice(&g.data[self.pos as usize..self.pos as usize + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Stream {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let p = match to {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::End(d) => self.sh.len as i64 + d,
            SeekFrom::Current(d) => self.pos as i64 + d,
        };
        self.pos = p.clamp(0, self.sh.len as i64) as u64;
        Ok(self.pos)
    }
}
