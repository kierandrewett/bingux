//! Space: echo and reverb.

use crate::dsp::{Biquad, Dsp, PortSpec, Spec, stereo};

pub fn plugins() -> Vec<Spec> {
    vec![
        Spec {
            id: 0x0B1D_0020,
            label: "bingux_echo",
            name: "Bingux Echo",
            ports: with(&[
                PortSpec::control("Delay", 10.0, 2000.0, 300.0).log(),
                PortSpec::control("Feedback", 0.0, 90.0, 35.0),
                PortSpec::control("Tone", 500.0, 16000.0, 8000.0).log(),
                PortSpec::control("Mix", 0.0, 100.0, 25.0),
            ]),
            make: |rate| Box::new(Echo::new(rate)),
        },
        Spec {
            id: 0x0B1D_0021,
            label: "bingux_reverb",
            name: "Bingux Reverb",
            ports: with(&[
                PortSpec::control("Room size", 0.0, 100.0, 50.0),
                PortSpec::control("Damping", 0.0, 100.0, 50.0),
                PortSpec::control("Width", 0.0, 100.0, 100.0),
                PortSpec::control("Mix", 0.0, 100.0, 20.0),
            ]),
            make: |rate| Box::new(Reverb::new(rate)),
        },
    ]
}

fn with(controls: &[PortSpec]) -> Vec<PortSpec> {
    let mut ports = stereo();
    ports.extend_from_slice(controls);
    ports
}

/// A delay with feedback, the repeats darkened by the tone filter.
struct Echo {
    rate: f32,
    line: [Vec<f32>; 2],
    at: usize,
    tone: f32,
    filter: [Biquad; 2],
}

impl Echo {
    fn new(rate: f32) -> Self {
        // Two seconds is the longest delay.
        let length = (rate * 2.0) as usize + 1;
        Self {
            rate,
            line: [vec![0.0; length], vec![0.0; length]],
            at: 0,
            tone: 0.0,
            filter: [Biquad::default(); 2],
        }
    }
}

impl Dsp for Echo {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], _: &mut [f32]) {
        let length = self.line[0].len();
        let delay = ((ctl[0] * 0.001 * self.rate) as usize).clamp(1, length - 1);
        let feedback = (ctl[1] / 100.0).clamp(0.0, 0.9);
        let mix = (ctl[3] / 100.0).clamp(0.0, 1.0);
        if (ctl[2] - self.tone).abs() > 0.5 {
            self.tone = ctl[2];
            self.filter = [
                Biquad::lowpass(ctl[2], 0.707, self.rate),
                Biquad::lowpass(ctl[2], 0.707, self.rate),
            ];
        }
        let [out_l, out_r] = outs;
        for i in 0..ins[0].len() {
            let read = (self.at + length - delay) % length;
            for (channel, (input, output)) in [(ins[0][i], &mut out_l[i]), (ins[1][i], &mut out_r[i])]
                .into_iter()
                .enumerate()
            {
                let echoed = self.line[channel][read];
                self.line[channel][self.at] = input + self.filter[channel].tick(echoed) * feedback;
                *output = input + echoed * mix;
            }
            self.at = (self.at + 1) % length;
        }
    }
}

/// Eight parallel comb filters into four series all-pass filters per channel:
/// the classic Freeverb design.
struct Reverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
}

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
/// The right channel's lines are this much longer, which decorrelates it.
const STEREO_SPREAD: usize = 23;

struct Comb {
    buffer: Vec<f32>,
    at: usize,
    store: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length],
            at: 0,
            store: 0.0,
        }
    }

    fn tick(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let out = self.buffer[self.at];
        self.store = out * (1.0 - damping) + self.store * damping;
        self.buffer[self.at] = input + self.store * feedback;
        self.at = (self.at + 1) % self.buffer.len();
        out
    }
}

struct Allpass {
    buffer: Vec<f32>,
    at: usize,
}

impl Allpass {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length],
            at: 0,
        }
    }

    fn tick(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.at];
        let out = buffered - input;
        self.buffer[self.at] = input + buffered * 0.5;
        self.at = (self.at + 1) % self.buffer.len();
        out
    }
}

impl Reverb {
    fn new(rate: f32) -> Self {
        let scale = rate / 44_100.0;
        let lines = |spread: usize| -> (Vec<Comb>, Vec<Allpass>) {
            (
                COMB_TUNING
                    .iter()
                    .map(|&length| Comb::new(((length + spread) as f32 * scale) as usize))
                    .collect(),
                ALLPASS_TUNING
                    .iter()
                    .map(|&length| Allpass::new(((length + spread) as f32 * scale) as usize))
                    .collect(),
            )
        };
        let (comb_l, allpass_l) = lines(0);
        let (comb_r, allpass_r) = lines(STEREO_SPREAD);
        Self {
            combs: [comb_l, comb_r],
            allpasses: [allpass_l, allpass_r],
        }
    }
}

impl Dsp for Reverb {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], _: &mut [f32]) {
        let feedback = 0.7 + 0.28 * (ctl[0] / 100.0).clamp(0.0, 1.0);
        let damping = 0.4 * (ctl[1] / 100.0).clamp(0.0, 1.0);
        let width = (ctl[2] / 100.0).clamp(0.0, 1.0);
        let mix = (ctl[3] / 100.0).clamp(0.0, 1.0);
        let [out_l, out_r] = outs;
        for i in 0..ins[0].len() {
            let (l, r) = (ins[0][i], ins[1][i]);
            let fed = (l + r) * 0.015;
            let mut wet = [0.0f32; 2];
            for channel in 0..2 {
                for comb in &mut self.combs[channel] {
                    wet[channel] += comb.tick(fed, feedback, damping);
                }
                for allpass in &mut self.allpasses[channel] {
                    wet[channel] = allpass.tick(wet[channel]);
                }
            }
            let (wet_l, wet_r) = (
                wet[0] * (0.5 + 0.5 * width) + wet[1] * (0.5 - 0.5 * width),
                wet[1] * (0.5 + 0.5 * width) + wet[0] * (0.5 - 0.5 * width),
            );
            out_l[i] = l * (1.0 - mix) + wet_l * 3.0 * mix;
            out_r[i] = r * (1.0 - mix) + wet_r * 3.0 * mix;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_repeats_after_the_delay() {
        let mut dsp = Echo::new(48_000.0);
        let mut input = vec![0.0; 24_000];
        input[0] = 1.0;
        let silence = vec![0.0; 24_000];
        let mut out_l = vec![0.0; 24_000];
        let mut out_r = vec![0.0; 24_000];
        dsp.process([&input, &silence], [&mut out_l, &mut out_r], &[100.0, 0.0, 16000.0, 100.0], &mut []);
        assert!((out_l[4800] - 1.0).abs() < 1e-6);
        assert_eq!(out_l[4000], 0.0);
    }

    #[test]
    fn reverb_has_a_tail() {
        let mut dsp = Reverb::new(48_000.0);
        let mut input = vec![0.0; 48_000];
        input[0] = 1.0;
        let silence = vec![0.0; 48_000];
        let mut out_l = vec![0.0; 48_000];
        let mut out_r = vec![0.0; 48_000];
        dsp.process([&input, &silence], [&mut out_l, &mut out_r], &[70.0, 30.0, 100.0, 50.0], &mut []);
        assert!(out_l[20_000..].iter().any(|x| x.abs() > 1e-4));
    }
}
