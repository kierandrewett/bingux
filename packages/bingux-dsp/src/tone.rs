//! Tone and stereo: stereo tools, loudness, bass enhancer, exciter and crossfeed.

use crate::dsp::{Biquad, Dsp, PortSpec, Spec, db_to_lin, stereo};

pub fn plugins() -> Vec<Spec> {
    vec![
        Spec {
            id: 0x0B1D_0010,
            label: "bingux_stereo_tools",
            name: "Bingux Stereo Tools",
            ports: with(&[
                PortSpec::control("Width", 0.0, 200.0, 100.0),
                PortSpec::control("Balance", -100.0, 100.0, 0.0),
                PortSpec::control("Mid gain", -12.0, 12.0, 0.0),
                PortSpec::control("Side gain", -12.0, 12.0, 0.0),
                PortSpec::toggle("Mono", false),
                PortSpec::toggle("Invert left", false),
                PortSpec::toggle("Invert right", false),
                PortSpec::toggle("Swap channels", false),
            ]),
            make: |_| Box::new(StereoTools),
        },
        Spec {
            id: 0x0B1D_0011,
            label: "bingux_loudness",
            name: "Bingux Loudness",
            ports: with(&[PortSpec::control("Amount", 0.0, 100.0, 50.0)]),
            make: |rate| Box::new(Loudness::new(rate)),
        },
        Spec {
            id: 0x0B1D_0012,
            label: "bingux_bass_enhancer",
            name: "Bingux Bass Enhancer",
            ports: with(&[
                PortSpec::control("Frequency", 40.0, 250.0, 100.0).log(),
                PortSpec::control("Amount", 0.0, 100.0, 40.0),
                PortSpec::control("Drive", 1.0, 10.0, 4.0),
            ]),
            make: |rate| Box::new(Harmonics::bass(rate)),
        },
        Spec {
            id: 0x0B1D_0013,
            label: "bingux_exciter",
            name: "Bingux Exciter",
            ports: with(&[
                PortSpec::control("Frequency", 1000.0, 10000.0, 3500.0).log(),
                PortSpec::control("Amount", 0.0, 100.0, 25.0),
                PortSpec::control("Drive", 1.0, 10.0, 3.0),
            ]),
            make: |rate| Box::new(Harmonics::exciter(rate)),
        },
        Spec {
            id: 0x0B1D_0014,
            label: "bingux_crossfeed",
            name: "Bingux Crossfeed",
            ports: with(&[
                PortSpec::control("Cutoff", 300.0, 1000.0, 700.0).log(),
                PortSpec::control("Feed", 1.0, 15.0, 4.5),
            ]),
            make: |rate| Box::new(Crossfeed::new(rate)),
        },
    ]
}

fn with(controls: &[PortSpec]) -> Vec<PortSpec> {
    let mut ports = stereo();
    ports.extend_from_slice(controls);
    ports
}

/// Width, balance, mid and side gain, mono, polarity and swap.
struct StereoTools;

impl Dsp for StereoTools {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], _: &mut [f32]) {
        let width = ctl[0] / 100.0;
        let balance = ctl[1] / 100.0;
        let mid_gain = db_to_lin(ctl[2]);
        let side_gain = db_to_lin(ctl[3]);
        let (mono, invert_l, invert_r, swap) = (ctl[4] >= 0.5, ctl[5] >= 0.5, ctl[6] >= 0.5, ctl[7] >= 0.5);
        // Balance lowers the channel away from the side it leans to.
        let left_gain = (1.0 - balance).min(1.0);
        let right_gain = (1.0 + balance).min(1.0);
        let [out_l, out_r] = outs;
        for i in 0..ins[0].len() {
            let (mut l, mut r) = (ins[0][i], ins[1][i]);
            if invert_l {
                l = -l;
            }
            if invert_r {
                r = -r;
            }
            if swap {
                std::mem::swap(&mut l, &mut r);
            }
            let mid = (l + r) * 0.5 * mid_gain;
            let side = (l - r) * 0.5 * side_gain * if mono { 0.0 } else { width };
            out_l[i] = (mid + side) * left_gain;
            out_r[i] = (mid - side) * right_gain;
        }
    }
}

/// Raises the bass and the treble, which the ear hears less at low volume.
struct Loudness {
    rate: f32,
    amount: f32,
    low: [Biquad; 2],
    high: [Biquad; 2],
}

impl Loudness {
    fn new(rate: f32) -> Self {
        Self {
            rate,
            amount: -1.0,
            low: [Biquad::default(); 2],
            high: [Biquad::default(); 2],
        }
    }
}

impl Dsp for Loudness {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], _: &mut [f32]) {
        let amount = (ctl[0] / 100.0).clamp(0.0, 1.0);
        if (amount - self.amount).abs() > 1e-4 {
            self.amount = amount;
            for channel in 0..2 {
                self.low[channel] = Biquad::low_shelf(120.0, 10.0 * amount, self.rate);
                self.high[channel] = Biquad::high_shelf(7000.0, 6.0 * amount, self.rate);
            }
        }
        let [out_l, out_r] = outs;
        for i in 0..ins[0].len() {
            out_l[i] = self.high[0].tick(self.low[0].tick(ins[0][i]));
            out_r[i] = self.high[1].tick(self.low[1].tick(ins[1][i]));
        }
    }
}

/// Adds harmonics: the band below (bass enhancer) or above (exciter) the
/// frequency is driven into soft clipping, and only the new harmonics are
/// mixed back in.
struct Harmonics {
    rate: f32,
    low_band: bool,
    freq: f32,
    band: [Biquad; 2],
    clean: [Biquad; 2],
}

impl Harmonics {
    fn bass(rate: f32) -> Self {
        Self::new(rate, true)
    }

    fn exciter(rate: f32) -> Self {
        Self::new(rate, false)
    }

    fn new(rate: f32, low_band: bool) -> Self {
        Self {
            rate,
            low_band,
            freq: 0.0,
            band: [Biquad::default(); 2],
            clean: [Biquad::default(); 2],
        }
    }
}

impl Dsp for Harmonics {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], _: &mut [f32]) {
        let (freq, amount, drive) = (ctl[0], ctl[1] / 100.0, ctl[2]);
        if (freq - self.freq).abs() > 0.5 {
            self.freq = freq;
            for channel in 0..2 {
                if self.low_band {
                    // Take the bass, drive it, then keep only what is above it.
                    self.band[channel] = Biquad::lowpass(freq, 0.707, self.rate);
                    self.clean[channel] = Biquad::highpass(freq * 1.5, 0.707, self.rate);
                } else {
                    // Take the treble, drive it, and keep what is above it.
                    self.band[channel] = Biquad::highpass(freq, 0.707, self.rate);
                    self.clean[channel] = Biquad::highpass(freq, 0.707, self.rate);
                }
            }
        }
        let [out_l, out_r] = outs;
        for i in 0..ins[0].len() {
            for (channel, (input, output)) in [(ins[0][i], &mut out_l[i]), (ins[1][i], &mut out_r[i])]
                .into_iter()
                .enumerate()
            {
                let band = self.band[channel].tick(input);
                let driven = (band * drive).tanh() / drive.max(1.0);
                let harmonics = self.clean[channel].tick(driven);
                *output = input + harmonics * amount * 2.0;
            }
        }
    }
}

/// Mixes a darkened, slightly delayed copy of each channel into the other, as
/// loudspeakers do, so headphone stereo does not sit inside each ear.
struct Crossfeed {
    rate: f32,
    cutoff: f32,
    low: [Biquad; 2],
    delay: [Vec<f32>; 2],
    at: usize,
}

impl Crossfeed {
    fn new(rate: f32) -> Self {
        let length = ((rate * 0.0003) as usize).max(1);
        Self {
            rate,
            cutoff: 0.0,
            low: [Biquad::default(); 2],
            delay: [vec![0.0; length], vec![0.0; length]],
            at: 0,
        }
    }
}

impl Dsp for Crossfeed {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], _: &mut [f32]) {
        let (cutoff, feed_db) = (ctl[0], ctl[1]);
        if (cutoff - self.cutoff).abs() > 0.5 {
            self.cutoff = cutoff;
            self.low = [
                Biquad::lowpass(cutoff, 0.5, self.rate),
                Biquad::lowpass(cutoff, 0.5, self.rate),
            ];
        }
        let cross = db_to_lin(-feed_db);
        // Keep the level of a centred signal unchanged.
        let direct = 1.0 / (1.0 + cross);
        let cross = cross * direct;
        let length = self.delay[0].len();
        let [out_l, out_r] = outs;
        for i in 0..ins[0].len() {
            let (l, r) = (ins[0][i], ins[1][i]);
            let (from_r, from_l) = (self.low[1].tick(r), self.low[0].tick(l));
            let slot = self.at % length;
            let (heard_from_r, heard_from_l) = (self.delay[1][slot], self.delay[0][slot]);
            self.delay[1][slot] = from_r;
            self.delay[0][slot] = from_l;
            out_l[i] = l * direct + heard_from_r * cross;
            out_r[i] = r * direct + heard_from_l * cross;
            self.at = self.at.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dsp: &mut dyn Dsp, left: &[f32], right: &[f32], ctl: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut out_l = vec![0.0; left.len()];
        let mut out_r = vec![0.0; left.len()];
        dsp.process([left, right], [&mut out_l, &mut out_r], ctl, &mut []);
        (out_l, out_r)
    }

    #[test]
    fn mono_folds_both_channels_to_the_same_signal() {
        let left = vec![1.0; 64];
        let right = vec![-1.0; 64];
        let ctl = [100.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        let (l, r) = run(&mut StereoTools, &left, &right, &ctl);
        assert!(l.iter().chain(&r).all(|x| x.abs() < 1e-6));
    }

    #[test]
    fn neutral_stereo_tools_pass_the_signal() {
        let left: Vec<f32> = (0..64).map(|i| (i as f32 * 0.1).sin()).collect();
        let right: Vec<f32> = (0..64).map(|i| (i as f32 * 0.07).cos()).collect();
        let ctl = [100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (l, r) = run(&mut StereoTools, &left, &right, &ctl);
        for i in 0..64 {
            assert!((l[i] - left[i]).abs() < 1e-5 && (r[i] - right[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn crossfeed_leaks_one_channel_into_the_other() {
        let left = vec![0.0; 4800];
        let mut right = vec![0.0; 4800];
        right[0] = 1.0;
        let (l, _) = run(&mut Crossfeed::new(48_000.0), &left, &right, &[700.0, 4.5]);
        assert!(l.iter().any(|x| x.abs() > 0.01));
    }
}
