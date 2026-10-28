//! Playback with our own decoder (symphonia via rodio) and our own output (cpal): no system codecs needed.
//! A tap on the decoded samples measures the level of each channel for the VU meters and LEVEL.

use rodio::{Decoder, Player as Sink, Source};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Peak-ish RMS of the last few ms of each channel (f32 bits), written by the audio thread.
#[derive(Default)]
pub struct Levels {
    pub l: AtomicU32,
    pub r: AtomicU32,
}
impl Levels {
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
}

pub struct Player {
    _stream: rodio::MixerDeviceSink,
    sink: Sink,
    pub levels: Arc<Levels>,
}

impl Player {
    pub fn new() -> Result<Self, String> {
        let stream = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        let sink = Sink::connect_new(stream.mixer());
        Ok(Player { _stream: stream, sink, levels: Arc::new(Levels::default()) })
    }

    /// Replace what is playing with this mp3, starting `offset` into it.
    pub fn play(&self, mp3: Vec<u8>, offset: Duration) -> Result<(), String> {
        let dec = Decoder::new(Cursor::new(mp3)).map_err(|e| e.to_string())?;
        let ch: u16 = dec.channels().into();
        let src = dec.skip_duration(offset);
        let tap = Tap { inner: src, levels: self.levels.clone(), ch, idx: 0, acc: [0.0; 2], n: 0 };
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
    pub fn idle(&self) -> bool {
        self.sink.empty()
    }
}
