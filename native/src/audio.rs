//! Playback with our own decoder (symphonia via rodio) and our own output (cpal): no system codecs needed.
//! A tap on the decoded samples measures the level of each channel for the VU meters and LEVEL.

use rodio::{Decoder, Player as Sink, Source};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Peak-ish RMS of the last few ms of each channel (f32 bits), written by the audio thread.
#[derive(Default)]
pub struct Levels {
    pub l: AtomicU32,
    pub r: AtomicU32,
    /// the last 2048 samples played (left channel), for the spectrum
    ring: std::sync::Mutex<VecDeque<f32>>,
}
impl Levels {
    pub fn samples(&self) -> Vec<f32> {
        self.ring.lock().map(|r| r.iter().copied().collect()).unwrap_or_default()
    }
    pub fn get(&self) -> (f32, f32) {
        (f32::from_bits(self.l.load(Ordering::Relaxed)), f32::from_bits(self.r.load(Ordering::Relaxed)))
    }
}

/// Wraps a source: passes samples through and keeps a running RMS per channel.
struct Tap<S> {
    inner: S,
    levels: Arc<Levels>,
    ch: u16,
    idx: u16,
    acc: [f32; 2],
    n: u32,
    mono: Vec<f32>,
}

impl<S: Source<Item = f32>> Iterator for Tap<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = self.inner.next()?;
        let c = (self.idx % self.ch.max(1)).min(1) as usize;
        self.acc[c] += s * s;
        self.idx = self.idx.wrapping_add(1);
        if c == 0 || self.ch == 1 {
            self.n += 1;
            self.mono.push(s);
            if self.mono.len() >= 256 {
                if let Ok(mut r) = self.levels.ring.lock() {
                    r.extend(self.mono.drain(..));
                    while r.len() > 2048 {
                        r.pop_front();
                    }
                }
                self.mono.clear();
            }
        }
        if self.n >= 1024 {
            let l = (self.acc[0] / self.n as f32).sqrt();
            let r = if self.ch > 1 { (self.acc[1] / self.n as f32).sqrt() } else { l };
            self.levels.l.store(l.to_bits(), Ordering::Relaxed);
            self.levels.r.store(r.to_bits(), Ordering::Relaxed);
            self.acc = [0.0; 2];
            self.n = 0;
        }
        Some(s)
    }
}

impl<S: Source<Item = f32>> Source for Tap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    // seeking goes through to the decoder (without this the wrapper said "not supported" and the click did nothing)
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

/// A song ready to play: the streaming decoder, already at its place, with about a second decoded ahead (so the
/// sound starts at once and a slow network does not stutter at the very start).
pub struct Prepared {
    pre: VecDeque<f32>,
    inner: Box<dyn Source<Item = f32> + Send>,
}

impl Iterator for Prepared {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.pre.pop_front().or_else(|| self.inner.next())
    }
}

impl Source for Prepared {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.pre.clear();
        self.inner.try_seek(pos)
    }
}

/// Open an mp3 URL for streaming, `offset()` into it (asked once the stream is open, so a slow start is counted),
/// and decode the first second. Runs on a worker thread.
pub fn prepare(url: &str, offset: impl FnOnce() -> Duration) -> Result<Prepared, String> {
    let t0 = std::time::Instant::now();
    let st = crate::stream::open(url)?;
    crate::dlog(&format!("stream open {:?}", t0.elapsed()));
    let len = st.len();
    let mut dec = Decoder::builder().with_data(st).with_byte_len(len).with_seekable(true).with_coarse_seek(true).with_hint("mp3").build().map_err(|e| e.to_string())?;
    let off = offset();
    if off > Duration::from_millis(500) {
        let _ = dec.try_seek(off);
    }
    crate::dlog(&format!("decoder + seek {off:?} {:?}", t0.elapsed()));
    let ch: u16 = dec.channels().into();
    let sr: u32 = dec.sample_rate().into();
    let mut pre = VecDeque::with_capacity((sr * ch as u32) as usize);
    for _ in 0..(sr as usize * ch as usize) {
        match dec.next() {
            Some(s) => pre.push_back(s),
            None => break,
        }
    }
    crate::dlog(&format!("prebuffered {:?}", t0.elapsed()));
    Ok(Prepared { pre, inner: Box::new(dec) })
}

#[derive(Clone, Copy)]
pub enum Sfx {
    Down,
    Up,
    Latch,
}

/// A key's sound: `thump` = a sine falling an octave, `burst` = band-passed noise, both decaying exponentially.
fn synth(kind: Sfx) -> Vec<f32> {
    const SR: f32 = 44100.0;
    let mut out = vec![0.0f32; (SR * 0.1) as usize];
    let mut seed: u32 = 0x9e3779b9;
    let mut noise = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    let env = |t: f32, dur: f32, gain: f32| if t > dur { 0.0 } else { gain * (0.0001f32 / gain).powf(t / dur) };
    let mut thump = |out: &mut Vec<f32>, f: f32, dur: f32, gain: f32| {
        let mut ph = 0.0f32;
        for (i, o) in out.iter_mut().enumerate() {
            let t = i as f32 / SR;
            let fr = f * 0.5f32.powf((t / dur).min(1.0));
            ph += fr / SR * std::f32::consts::TAU;
            *o += ph.sin() * env(t, dur, gain);
        }
    };
    let mut burst = |out: &mut Vec<f32>, f: f32, q: f32, dur: f32, gain: f32| {
        // RBJ band-pass (constant 0 dB peak)
        let w = std::f32::consts::TAU * f / SR;
        let al = w.sin() / (2.0 * q);
        let (b0, b2, a0, a1, a2) = (al, -al, 1.0 + al, -2.0 * w.cos(), 1.0 - al);
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for (i, o) in out.iter_mut().enumerate() {
            let t = i as f32 / SR;
            let x = noise();
            let y = (b0 * x + b2 * x2 - a1 * y1 - a2 * y2) / a0;
            x2 = x1;
            x1 = x;
            y2 = y1;
            y1 = y;
            *o += y * env(t, dur, gain);
        }
    };
    match kind {
        Sfx::Down => {
            thump(&mut out, 150.0, 0.05, 0.22);
            burst(&mut out, 2600.0, 2.5, 0.025, 0.35);
        }
        Sfx::Up => burst(&mut out, 4200.0, 3.0, 0.018, 0.22),
        Sfx::Latch => {
            thump(&mut out, 95.0, 0.07, 0.28);
            burst(&mut out, 3000.0, 2.0, 0.03, 0.3);
        }
    }
    out.iter().map(|v| v * 0.55).collect()
}

/// A song kept on this computer (SAQLASH), from memory.
pub fn prepare_bytes(mp3: Vec<u8>) -> Result<Prepared, String> {
    let dec = Decoder::try_from(std::io::Cursor::new(mp3)).map_err(|e| e.to_string())?;
    Ok(Prepared { pre: VecDeque::new(), inner: Box::new(dec) })
}

pub struct Player {
    _stream: rodio::MixerDeviceSink,
    sink: Arc<Sink>,
    pub levels: Arc<Levels>,
}

impl Player {
    pub fn new() -> Result<Self, String> {
        let stream = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        let sink = Arc::new(Sink::connect_new(stream.mixer()));
        Ok(Player { _stream: stream, sink, levels: Arc::new(Levels::default()) })
    }

    /// Replace what is playing with this song.
    pub fn play(&self, src: Prepared) -> Result<(), String> {
        let ch: u16 = src.channels().into();
        let tap = Tap { inner: src, levels: self.levels.clone(), ch, idx: 0, acc: [0.0; 2], n: 0, mono: Vec::with_capacity(256) };
        self.sink.stop();
        self.sink.append(tap);
        self.sink.play();
        Ok(())
    }

    pub fn set_paused(&self, p: bool) {
        if p { self.sink.pause() } else { self.sink.play() }
    }
    pub fn set_volume(&self, v: f32) {
        self.sink.set_volume(v);
    }
    pub fn pos(&self) -> Duration {
        self.sink.get_pos()
    }
    /// Seek on a worker thread: the audio thread may have to wait for that part of the file to arrive.
    pub fn seek(&self, to: Duration) {
        let s = self.sink.clone();
        std::thread::spawn(move || {
            let _ = s.try_seek(to);
        });
    }
    /// A key's click, on its own (not under the music's volume).
    pub fn sfx(&self, kind: Sfx) {
        let buf = rodio::buffer::SamplesBuffer::new(std::num::NonZero::new(1u16).unwrap(), std::num::NonZero::new(44100u32).unwrap(), synth(kind));
        self._stream.mixer().add(buf);
    }
    pub fn idle(&self) -> bool {
        self.sink.empty()
    }
}
