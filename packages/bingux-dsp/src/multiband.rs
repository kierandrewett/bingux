//! Three-band dynamics (compressor and gate) and the maximizer.
//!
//! The bands come from two Linkwitz-Riley crossovers (two cascaded
//! Butterworth filters each), so the bands add back to a flat response. The
//! low band also passes the upper crossover's all-pass pair, which keeps its
//! phase in step with the others.

use crate::dsp::{Biquad, Dsp, PortSpec, Spec, db_to_lin, lin_to_db, time_constant};
use crate::dynamics::{Limiter, compressor_gain, follow, with};

/// Port names by band: threshold, ratio, attack, release, makeup, range, reduction.
const NAMES: [[&str; 7]; 3] = [
    ["Low threshold", "Low ratio", "Low attack", "Low release", "Low makeup", "Low range", "Low reduction"],
    ["Mid threshold", "Mid ratio", "Mid attack", "Mid release", "Mid makeup", "Mid range", "Mid reduction"],
    ["High threshold", "High ratio", "High attack", "High release", "High makeup", "High range", "High reduction"],
];

pub fn plugins() -> Vec<Spec> {
    let mut compressor = vec![
        PortSpec::control("Crossover 1", 40.0, 1000.0, 250.0).log(),
        PortSpec::control("Crossover 2", 1000.0, 12000.0, 3000.0).log(),
    ];
    let mut gate = compressor.clone();
    for names in NAMES {
        compressor.push(PortSpec::control(names[0], -60.0, 0.0, -20.0));
        compressor.push(PortSpec::control(names[1], 1.0, 20.0, 3.0));
        compressor.push(PortSpec::control(names[2], 0.1, 200.0, 10.0).log());
        compressor.push(PortSpec::control(names[3], 5.0, 2000.0, 150.0).log());
        compressor.push(PortSpec::control(names[4], -12.0, 24.0, 0.0));
        gate.push(PortSpec::control(names[0], -80.0, 0.0, -50.0));
        gate.push(PortSpec::control(names[5], -80.0, 0.0, -40.0));
        gate.push(PortSpec::control(names[3], 5.0, 2000.0, 200.0).log());
    }
    for names in NAMES {
        compressor.push(PortSpec::meter(names[6], -40.0, 0.0));
        gate.push(PortSpec::meter(names[6], -80.0, 0.0));
    }
    vec![
        Spec {
            id: 0x0B1D_0030,
            label: "bingux_multiband_compressor",
            name: "Bingux Multiband Compressor",
            ports: with(&compressor),
            make: |rate| Box::new(Multiband::new(rate, Kind::Compressor)),
        },
        Spec {
            id: 0x0B1D_0031,
            label: "bingux_multiband_gate",
            name: "Bingux Multiband Gate",
            ports: with(&gate),
            make: |rate| Box::new(Multiband::new(rate, Kind::Gate)),
        },
        Spec {
            id: 0x0B1D_0032,
            label: "bingux_maximizer",
            name: "Bingux Maximizer",
            ports: with(&[
                PortSpec::control("Threshold", -30.0, 0.0, -10.0),
                PortSpec::control("Ceiling", -12.0, 0.0, -0.5),
                PortSpec::control("Release", 1.0, 1000.0, 80.0).log(),
                PortSpec::meter("Gain reduction", -40.0, 0.0),
            ]),
            make: |rate| Box::new(Maximizer::new(rate)),
        },
    ]
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Compressor,
    Gate,
}

/// One Linkwitz-Riley 4th order filter: two Butterworth sections.
#[derive(Clone, Copy, Default)]
struct Lr4([Biquad; 2]);

impl Lr4 {
    fn low(freq: f32, rate: f32) -> Self {
        Self([Biquad::lowpass(freq, 0.7071, rate); 2])
    }
    fn high(freq: f32, rate: f32) -> Self {
        Self([Biquad::highpass(freq, 0.7071, rate); 2])
    }
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.0[0].tick(x);
        self.0[1].tick(y)
    }
}

/// The filters that split one channel into three bands.
#[derive(Clone, Copy, Default)]
struct Split {
    low1: Lr4,
    high1: Lr4,
    low2: Lr4,
    high2: Lr4,
    comp_low: Lr4,
    comp_high: Lr4,
}

impl Split {
    fn new(f1: f32, f2: f32, rate: f32) -> Self {
        Self {
            low1: Lr4::low(f1, rate),
            high1: Lr4::high(f1, rate),
            low2: Lr4::low(f2, rate),
            high2: Lr4::high(f2, rate),
            comp_low: Lr4::low(f2, rate),
            comp_high: Lr4::high(f2, rate),
        }
    }

    fn tick(&mut self, x: f32) -> [f32; 3] {
        let low = self.low1.tick(x);
        let rest = self.high1.tick(x);
        let mid = self.low2.tick(rest);
        let high = self.high2.tick(rest);
        let low = self.comp_low.tick(low) + self.comp_high.tick(low);
        [low, mid, high]
    }
}

#[derive(Clone, Copy, Default)]
struct BandState {
    envelope: f32,
    reduction: f32,
    gate_open: bool,
}

struct Multiband {
    rate: f32,
    kind: Kind,
    crossovers: (f32, f32),
    split: [Split; 2],
    bands: [BandState; 3],
}

impl Multiband {
    fn new(rate: f32, kind: Kind) -> Self {
        Self {
            rate,
            kind,
            crossovers: (0.0, 0.0),
            split: [Split::default(); 2],
            bands: [BandState::default(); 3],
        }
    }
}

impl Dsp for Multiband {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]) {
        let (f1, f2) = (ctl[0], ctl[1].max(ctl[0] * 1.5));
        if (f1, f2) != self.crossovers {
            self.crossovers = (f1, f2);
            self.split = [Split::new(f1, f2, self.rate); 2];
        }
        let fall = time_constant(10.0, self.rate);
        let per = if self.kind == Kind::Compressor { 5 } else { 3 };
        let [out_l, out_r] = outs;
        let mut deepest = [0.0f32; 3];
        for i in 0..ins[0].len() {
            let bands = [self.split[0].tick(ins[0][i]), self.split[1].tick(ins[1][i])];
            let (mut sum_l, mut sum_r) = (0.0, 0.0);
            for band in 0..3 {
                let at = 2 + band * per;
                let state = &mut self.bands[band];
                let (l, r) = (bands[0][band], bands[1][band]);
                state.envelope = l.abs().max(r.abs()).max(state.envelope * fall);
                let level = lin_to_db(state.envelope);
                let gain_db = match self.kind {
                    Kind::Compressor => {
                        let target = compressor_gain(level, ctl[at], ctl[at + 1], 0.0);
                        follow(
                            &mut state.reduction,
                            target,
                            time_constant(ctl[at + 2], self.rate),
                            time_constant(ctl[at + 3], self.rate),
                        );
                        state.reduction + ctl[at + 4]
                    }
                    Kind::Gate => {
                        if level >= ctl[at] {
                            state.gate_open = true;
                        } else if level < ctl[at] - 3.0 {
                            state.gate_open = false;
                        }
                        let target = if state.gate_open { 0.0 } else { ctl[at + 1] };
                        follow(
                            &mut state.reduction,
                            target,
                            time_constant(2.0, self.rate),
                            time_constant(ctl[at + 2], self.rate),
                        );
                        state.reduction
                    }
                };
                deepest[band] = deepest[band].min(state.reduction);
                let gain = db_to_lin(gain_db);
                sum_l += l * gain;
                sum_r += r * gain;
            }
            out_l[i] = sum_l;
            out_r[i] = sum_r;
        }
        meters[..3].copy_from_slice(&deepest);
    }
}

/// Raises the level until the peaks touch the ceiling: the threshold is the
/// gain applied before a look-ahead limiter set to the ceiling.
struct Maximizer {
    limiter: Limiter,
    scratch: [Vec<f32>; 2],
}

impl Maximizer {
    fn new(rate: f32) -> Self {
        Self {
            limiter: Limiter::new(rate),
            scratch: [Vec::new(), Vec::new()],
        }
    }
}

impl Dsp for Maximizer {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]) {
        let boost = db_to_lin(-ctl[0] + ctl[1]);
        for (scratch, input) in self.scratch.iter_mut().zip(ins) {
            scratch.clear();
            scratch.extend(input.iter().map(|x| x * boost));
        }
        let [left, right] = &self.scratch;
        self.limiter.process([left, right], outs, &[ctl[1], ctl[2]], meters);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn tone(freq: f32, db: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| db_to_lin(db) * (2.0 * std::f32::consts::PI * freq * i as f32 / RATE).sin())
            .collect()
    }

    fn rms_tail(x: &[f32]) -> f32 {
        let tail = &x[x.len() / 2..];
        (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt()
    }

    fn run(dsp: &mut dyn Dsp, input: &[f32], ctl: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut l = vec![0.0; input.len()];
        let mut r = vec![0.0; input.len()];
        let mut meters = [0.0; 4];
        dsp.process([input, input], [&mut l, &mut r], ctl, &mut meters);
        (l, meters.to_vec())
    }

    #[test]
    fn crossovers_add_back_to_a_flat_response() {
        let mut dsp = Multiband::new(RATE, Kind::Compressor);
        // Thresholds at 0 dB and no makeup: nothing is compressed.
        let mut ctl = vec![250.0, 3000.0];
        for _ in 0..3 {
            ctl.extend([0.0, 1.0, 10.0, 100.0, 0.0]);
        }
        for freq in [80.0, 700.0, 6000.0] {
            let input = tone(freq, -12.0, 48_000);
            let (out, _) = run(&mut dsp, &input, &ctl);
            let ratio = rms_tail(&out) / rms_tail(&input);
            assert!((ratio - 1.0).abs() < 0.06, "{freq} Hz: {ratio}");
        }
    }

    #[test]
    fn only_the_loud_band_is_compressed() {
        let mut dsp = Multiband::new(RATE, Kind::Compressor);
        let mut ctl = vec![250.0, 3000.0];
        ctl.extend([-30.0, 8.0, 2.0, 50.0, 0.0]);
        ctl.extend([0.0, 1.0, 10.0, 100.0, 0.0]);
        ctl.extend([0.0, 1.0, 10.0, 100.0, 0.0]);
        let (_, meters) = run(&mut dsp, &tone(80.0, -6.0, 48_000), &ctl);
        assert!(meters[0] < -8.0, "{meters:?}");
        assert!(meters[2] > -0.5);
    }

    #[test]
    fn the_maximizer_raises_the_level_to_the_ceiling() {
        let mut dsp = Maximizer::new(RATE);
        let (out, _) = run(&mut dsp, &tone(440.0, -20.0, 48_000), &[-20.0, -1.0, 80.0]);
        let peak = out[24_000..].iter().fold(0.0f32, |p, x| p.max(x.abs()));
        assert!(peak > db_to_lin(-1.5) && peak <= db_to_lin(-1.0) * 1.01, "{peak}");
    }

    #[test]
    fn the_multiband_gate_silences_a_quiet_band() {
        let mut dsp = Multiband::new(RATE, Kind::Gate);
        let mut ctl = vec![250.0, 3000.0];
        for _ in 0..3 {
            ctl.extend([-40.0, -60.0, 50.0]);
        }
        let input = tone(700.0, -60.0, 48_000);
        let (out, _) = run(&mut dsp, &input, &ctl);
        assert!(rms_tail(&out) < rms_tail(&input) * 0.2);
    }
}
