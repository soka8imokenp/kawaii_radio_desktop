//! The spectrum for songs without lyrics: an FFT of the last 1024 samples played, with the Web Audio analyser's
//! smoothing and dB scale (so it moves like the Tauri version's dot-matrix display did), folded into 64 columns.

const N: usize = 1024;

/// In-place radix-2 FFT.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let a = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (a.cos(), a.sin());
        for s in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (ur, ui) = (re[s + k], im[s + k]);
                let (xr, xi) = (re[s + k + len / 2], im[s + k + len / 2]);
                let (vr, vi) = (xr * cr - xi * ci, xr * ci + xi * cr);
                re[s + k] = ur + vr;
                im[s + k] = ui + vi;
                re[s + k + len / 2] = ur - vr;
                im[s + k + len / 2] = ui - vi;
                let t = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = t;
            }
        }
        len <<= 1;
    }
}

pub struct Analyser {
    smooth: Vec<f32>,
    /// 64 columns (0..1) and their falling peaks (in rows)
    pub cols: [f32; 64],
    pub peaks: [f32; 64],
}

impl Analyser {
    pub fn new() -> Self {
        Analyser { smooth: vec![0.0; N / 2], cols: [0.0; 64], peaks: [0.0; 64] }
    }

    /// One frame: `samples` = the latest mono samples (any length; the last 1024 are used), `on` = playing.
    pub fn update(&mut self, samples: &[f32], on: bool, t: f32) {
        if on && samples.len() >= N {
            let s = &samples[samples.len() - N..];
            let mut re: Vec<f32> = s.iter().enumerate().map(|(i, v)| v * (0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N as f32).cos())).collect();
            let mut im = vec![0.0; N];
            fft(&mut re, &mut im);
            for k in 0..N / 2 {
                let m = (re[k] * re[k] + im[k] * im[k]).sqrt() / N as f32;
                self.smooth[k] = 0.8 * self.smooth[k] + 0.2 * m;
            }
        } else {
            for v in &mut self.smooth {
                *v *= 0.8;
            }
        }
        // like getByteFrequencyData: dB between -95 and -20 -> 0..1
        let byte = |m: f32| ((20.0 * m.max(1e-12).log10() + 95.0) / 75.0).clamp(0.0, 1.0);
        for c in 0..64 {
            let v = if on {
                let a = ((c as f32 / 64.0).powf(1.8) * 300.0) as usize + 1;
                let b = (((c + 1) as f32 / 64.0).powf(1.8) * 300.0) as usize + 2;
                (a..b.min(N / 2)).map(|i| byte(self.smooth[i])).fold(0.0, f32::max)
            } else {
                0.08 + 0.05 * (t / 0.6 + c as f32 * 0.4).sin()
            };
            self.cols[c] = v;
            let h = (v * 30.0).round();
            self.peaks[c] = (self.peaks[c] - 0.25).max(h);
        }
    }
}
