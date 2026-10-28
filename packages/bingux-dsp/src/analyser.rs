//! The spectrum analyser: a pass-through plugin that publishes the spectrum of
//! what passes in 64 log-spaced bands (see `crate::meters`). It never changes
//! the sound. The FFT is 2048 samples, run every 2048 samples, about 23 times
//! a second at 48 kHz, so its cost is fixed and small.

use crate::dsp::{Dsp, PortSpec, Spec, stereo};
use crate::meters::{BANDS, publish_spectrum};

const SIZE: usize = 2048;
/// The lowest and highest band edge, in Hz.
const LOW: f32 = 20.0;
const HIGH: f32 = 20_000.0;

pub fn plugins() -> Vec<Spec> {
    vec![Spec {
        id: 0x0B1D_0040,
        label: "bingux_analyser",
        name: "Bingux Spectrum Analyser",
        ports: {
            let mut ports = stereo();
            ports.push(PortSpec::toggle("Publish", true));
            ports
        },
        make: |rate| Box::new(Analyser::new(rate)),
    }]
}

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
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let next = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = next;
            }
        }
        len <<= 1;
    }
}

/// The levels, in dBFS, of `BANDS` log-spaced bands from a block of `SIZE`
/// samples at `rate`: the loudest bin inside each band, and for a band
/// narrower than a bin, the bin it falls in.
pub fn spectrum(samples: &[f32], rate: f32) -> [f32; BANDS] {
    let mut re: Vec<f32> = samples
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let window = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / samples.len() as f32).cos();
            x * window
        })
        .collect();
    let mut im = vec![0.0; re.len()];
    fft(&mut re, &mut im);
    let n = samples.len();
    // A full-scale sine under a Hann window reads n / 4 in its bin.
    let scale = 4.0 / n as f32;
    let bin_hz = rate / n as f32;
    let mut bands = [-120.0f32; BANDS];
    for (band, level) in bands.iter_mut().enumerate() {
        let low = LOW * (HIGH / LOW).powf(band as f32 / BANDS as f32);
        let high = LOW * (HIGH / LOW).powf((band + 1) as f32 / BANDS as f32);
        let first = (low / bin_hz).floor() as usize;
        let last = ((high / bin_hz).ceil() as usize).max(first + 1).min(n / 2);
        let mut peak = 0.0f32;
        for bin in first.min(n / 2 - 1)..last {
            peak = peak.max((re[bin] * re[bin] + im[bin] * im[bin]).sqrt() * scale);
        }
        *level = 20.0 * peak.max(1e-6).log10();
    }
    bands
}

struct Analyser {
    rate: f32,
    block: Vec<f32>,
}

impl Analyser {
    fn new(rate: f32) -> Self {
        Self {
            rate,
            block: Vec::with_capacity(SIZE),
        }
    }
}

impl Dsp for Analyser {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], _: &mut [f32]) {
        let [out_l, out_r] = outs;
        out_l.copy_from_slice(ins[0]);
        out_r.copy_from_slice(ins[1]);
        if ctl[0] < 0.5 {
            self.block.clear();
            return;
        }
        for i in 0..ins[0].len() {
            self.block.push((ins[0][i] + ins[1][i]) * 0.5);
            if self.block.len() == SIZE {
                publish_spectrum(&spectrum(&self.block, self.rate));
                self.block.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_lights_the_band_it_falls_in() {
        let rate = 48_000.0;
        let block: Vec<f32> = (0..SIZE)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate).sin())
            .collect();
        let bands = spectrum(&block, rate);
        let loudest = bands.iter().enumerate().fold(0, |at, (i, v)| if *v > bands[at] { i } else { at });
        let centre = LOW * (HIGH / LOW).powf((loudest as f32 + 0.5) / BANDS as f32);
        assert!((centre / 1000.0).log2().abs() < 0.2, "centre {centre}");
        assert!((bands[loudest] + 6.0).abs() < 1.5, "{}", bands[loudest]);
        // Far from the tone it is quiet.
        assert!(bands[2] < -60.0);
    }
}
