//! Dynamics: compressor, limiter, gate, expander, de-esser and auto gain.

use crate::dsp::{Biquad, Dsp, Kind, PortSpec, Spec, db_to_lin, lin_to_db, stereo, time_constant};

pub fn plugins() -> Vec<Spec> {
    vec![
        Spec {
            id: 0x0B1D_0001,
            label: "bingux_compressor",
            name: "Bingux Compressor",
            ports: with(&[
                PortSpec::control("Threshold", -60.0, 0.0, -18.0),
                PortSpec::control("Ratio", 1.0, 20.0, 4.0),
                PortSpec::control("Attack", 0.1, 200.0, 10.0).log(),
                PortSpec::control("Release", 5.0, 2000.0, 150.0).log(),
                PortSpec::control("Knee", 0.0, 24.0, 6.0),
                PortSpec::control("Makeup", -12.0, 36.0, 0.0),
                PortSpec::control("Mix", 0.0, 100.0, 100.0),
                PortSpec::meter("Gain reduction", -40.0, 0.0),
            ]),
            make: |rate| Box::new(Compressor::new(rate)),
        },
        Spec {
            id: 0x0B1D_0002,
            label: "bingux_limiter",
            name: "Bingux Limiter",
            ports: with(&[
                PortSpec::control("Threshold", -30.0, 0.0, -1.0),
                PortSpec::control("Release", 1.0, 1000.0, 100.0).log(),
                PortSpec::meter("Gain reduction", -40.0, 0.0),
            ]),
            make: |rate| Box::new(Limiter::new(rate)),
        },
        Spec {
            id: 0x0B1D_0003,
            label: "bingux_gate",
            name: "Bingux Gate",
            ports: with(&[
                PortSpec::control("Threshold", -80.0, 0.0, -45.0),
                PortSpec::control("Attack", 0.1, 100.0, 5.0).log(),
                PortSpec::control("Hold", 0.0, 1000.0, 50.0),
                PortSpec::control("Release", 1.0, 2000.0, 200.0).log(),
                PortSpec::control("Range", -80.0, 0.0, -60.0),
                PortSpec::meter("Gain reduction", -80.0, 0.0),
            ]),
            make: |rate| Box::new(Gate::new(rate)),
        },
        Spec {
            id: 0x0B1D_0004,
            label: "bingux_expander",
            name: "Bingux Expander",
            ports: with(&[
                PortSpec::control("Threshold", -80.0, 0.0, -40.0),
                PortSpec::control("Ratio", 1.0, 10.0, 2.0),
                PortSpec::control("Attack", 0.1, 200.0, 10.0).log(),
                PortSpec::control("Release", 5.0, 2000.0, 200.0).log(),
                PortSpec::control("Range", -60.0, 0.0, -30.0),
                PortSpec::meter("Gain reduction", -60.0, 0.0),
            ]),
            make: |rate| Box::new(Expander::new(rate)),
        },
        Spec {
            id: 0x0B1D_0005,
            label: "bingux_deesser",
            name: "Bingux De-esser",
            ports: with(&[
                PortSpec::control("Threshold", -60.0, 0.0, -30.0),
                PortSpec::control("Frequency", 2000.0, 12000.0, 6000.0).log(),
                PortSpec::control("Ratio", 1.0, 20.0, 4.0),
                PortSpec::control("Release", 5.0, 500.0, 60.0).log(),
                PortSpec::meter("Gain reduction", -40.0, 0.0),
            ]),
            make: |rate| Box::new(Deesser::new(rate)),
        },
        Spec {
            id: 0x0B1D_0006,
            label: "bingux_autogain",
            name: "Bingux Auto Gain",
            ports: with(&[
                PortSpec::control("Target", -40.0, -6.0, -18.0),
                PortSpec::control("Maximum gain", 0.0, 30.0, 12.0),
                PortSpec::control("Speed", 0.1, 10.0, 1.0).log(),
                PortSpec::meter("Gain", -30.0, 30.0),
            ]),
            make: |rate| Box::new(AutoGain::new(rate)),
        },
    ]
}

/// The stereo ports, the controls, a "Meter slot" control and then the meters.
/// The slot says where the plugin publishes its meters (see `crate::meters`).
pub(crate) fn with(controls: &[PortSpec]) -> Vec<PortSpec> {
    let mut ports = stereo();
    let first_meter = controls
        .iter()
        .position(|port| port.kind == Kind::ControlOut)
        .unwrap_or(controls.len());
    ports.extend_from_slice(&controls[..first_meter]);
    ports.push(PortSpec::control("Meter slot", 0.0, 63.0, 0.0).integer());
    ports.extend_from_slice(&controls[first_meter..]);
    ports
}

/// The gain change, in dB and never positive, a compressor applies to a signal
/// at `level_db`: nothing under the knee, the ratio over it and a quadratic
/// blend across it.
pub fn compressor_gain(level_db: f32, threshold: f32, ratio: f32, knee: f32) -> f32 {
    let over = level_db - threshold;
    let slope = 1.0 / ratio.max(1.0) - 1.0;
    if 2.0 * over < -knee {
        0.0
    } else if 2.0 * over.abs() <= knee {
        slope * (over + knee / 2.0).powi(2) / (2.0 * knee.max(1e-6))
    } else {
        slope * over
    }
}

/// Smooth a gain in dB: toward more reduction with the attack, back with the release.
pub(crate) fn follow(state: &mut f32, target: f32, attack: f32, release: f32) {
    let coef = if target < *state { attack } else { release };
    *state = coef * *state + (1.0 - coef) * target;
}

struct Compressor {
    rate: f32,
    envelope: f32,
    reduction: f32,
}

impl Compressor {
    fn new(rate: f32) -> Self {
        Self {
            rate,
            envelope: 0.0,
            reduction: 0.0,
        }
    }
}

impl Dsp for Compressor {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]) {
        let (threshold, ratio, knee) = (ctl[0], ctl[1], ctl[4]);
        let attack = time_constant(ctl[2], self.rate);
        let release = time_constant(ctl[3], self.rate);
        let makeup = ctl[5];
        let mix = (ctl[6] / 100.0).clamp(0.0, 1.0);
        let [out_l, out_r] = outs;
        let mut deepest = 0.0f32;
        let fall = time_constant(10.0, self.rate);
        for i in 0..ins[0].len() {
            let (l, r) = (ins[0][i], ins[1][i]);
            // A peak envelope, so the zero crossings of a wave do not lift the gain.
            self.envelope = l.abs().max(r.abs()).max(self.envelope * fall);
            let level = lin_to_db(self.envelope);
            follow(&mut self.reduction, compressor_gain(level, threshold, ratio, knee), attack, release);
            deepest = deepest.min(self.reduction);
            let wet = db_to_lin(self.reduction + makeup);
            let gain = 1.0 - mix + mix * wet;
            out_l[i] = l * gain;
            out_r[i] = r * gain;
        }
        meters[0] = deepest;
    }
}

/// A look-ahead peak limiter. A peak is found `window` samples before it is
/// heard; the gain falls to hold it under the threshold, smoothed over the
/// window so there is no click, and recovers with the release time.
pub(crate) struct Limiter {
    rate: f32,
    window: usize,
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    wanted: Vec<f32>,
    lowest: Vec<f32>,
    sum: f64,
    at: usize,
    gain: f32,
}

impl Limiter {
    pub(crate) fn new(rate: f32) -> Self {
        let window = ((rate * 0.003) as usize).max(2);
        Self {
            rate,
            window,
            delay_l: vec![0.0; window - 1],
            delay_r: vec![0.0; window - 1],
            wanted: vec![1.0; window],
            lowest: vec![1.0; window],
            sum: window as f64,
            at: 0,
            gain: 1.0,
        }
    }
}

impl Dsp for Limiter {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]) {
        let threshold = db_to_lin(ctl[0]);
        let release = time_constant(ctl[1], self.rate);
        let [out_l, out_r] = outs;
        let delay = self.window - 1;
        let mut lowest_gain = 1.0f32;
        for i in 0..ins[0].len() {
            let (l, r) = (ins[0][i], ins[1][i]);
            let peak = l.abs().max(r.abs());
            let wanted = if peak > threshold { threshold / peak } else { 1.0 };
            let slot = self.at % self.window;
            self.wanted[slot] = wanted;
            let lowest = self.wanted.iter().copied().fold(1.0f32, f32::min);
            self.sum += f64::from(lowest) - f64::from(self.lowest[slot]);
            self.lowest[slot] = lowest;
            let smoothed = (self.sum / self.window as f64) as f32;
            self.gain = if smoothed < self.gain {
                smoothed
            } else {
                self.gain + (smoothed - self.gain) * (1.0 - release)
            };
            lowest_gain = lowest_gain.min(self.gain);
            let held = self.at % delay;
            let (heard_l, heard_r) = (self.delay_l[held], self.delay_r[held]);
            self.delay_l[held] = l;
            self.delay_r[held] = r;
            out_l[i] = heard_l * self.gain;
            out_r[i] = heard_r * self.gain;
            self.at = self.at.wrapping_add(1);
        }
        meters[0] = lin_to_db(lowest_gain);
    }
}

struct Gate {
    rate: f32,
    level: f32,
    gain: f32,
    open: bool,
    held: u64,
}

impl Gate {
    fn new(rate: f32) -> Self {
        Self {
            rate,
            level: 0.0,
            gain: 0.0,
            open: false,
            held: 0,
        }
    }
}

impl Dsp for Gate {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]) {
        let threshold = db_to_lin(ctl[0]);
        let close_below = threshold * 0.708;
        let attack = time_constant(ctl[1], self.rate);
        let hold = (ctl[2] * 0.001 * self.rate) as u64;
        let release = time_constant(ctl[3], self.rate);
        let floor = db_to_lin(ctl[4]);
        let fall = time_constant(5.0, self.rate);
        let [out_l, out_r] = outs;
        let mut deepest = 1.0f32;
        for i in 0..ins[0].len() {
            let (l, r) = (ins[0][i], ins[1][i]);
            self.level = l.abs().max(r.abs()).max(self.level * fall);
            if self.level >= threshold {
                self.open = true;
                self.held = hold;
            } else if self.open && self.level < close_below {
                if self.held > 0 {
                    self.held -= 1;
                } else {
                    self.open = false;
                }
            }
            let (target, coef) = if self.open {
                (1.0, attack)
            } else {
                (floor, release)
            };
            self.gain = target + (self.gain - target) * coef;
            deepest = deepest.min(self.gain);
            out_l[i] = l * self.gain;
            out_r[i] = r * self.gain;
        }
        meters[0] = lin_to_db(deepest);
    }
}

struct Expander {
    rate: f32,
    reduction: f32,
}

impl Expander {
    fn new(rate: f32) -> Self {
        Self { rate, reduction: 0.0 }
    }
}

impl Dsp for Expander {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]) {
        let (threshold, ratio) = (ctl[0], ctl[1]);
        let attack = time_constant(ctl[2], self.rate);
        let release = time_constant(ctl[3], self.rate);
        let range = ctl[4];
        let [out_l, out_r] = outs;
        let mut deepest = 0.0f32;
        for i in 0..ins[0].len() {
            let (l, r) = (ins[0][i], ins[1][i]);
            let level = lin_to_db(l.abs().max(r.abs()));
            let wanted = ((level - threshold) * (ratio - 1.0)).clamp(range, 0.0);
            // The gain falls with the attack time and comes back with the release.
            follow(&mut self.reduction, wanted, attack, release);
            deepest = deepest.min(self.reduction);
            let gain = db_to_lin(self.reduction);
            out_l[i] = l * gain;
            out_r[i] = r * gain;
        }
        meters[0] = deepest;
    }
}

/// Reduces only the band above `Frequency`, so a hissing "s" is tamed while
/// the voice below it is untouched.
struct Deesser {
    rate: f32,
    split_freq: f32,
    low_l: Biquad,
    low_r: Biquad,
    reduction: f32,
}

impl Deesser {
    fn new(rate: f32) -> Self {
        Self {
            rate,
            split_freq: 0.0,
            low_l: Biquad::default(),
            low_r: Biquad::default(),
            reduction: 0.0,
        }
    }
}

impl Dsp for Deesser {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]) {
        let (threshold, freq, ratio) = (ctl[0], ctl[1], ctl[2]);
        let attack = time_constant(0.5, self.rate);
        let release = time_constant(ctl[3], self.rate);
        if (freq - self.split_freq).abs() > 0.5 {
            self.split_freq = freq;
            self.low_l = Biquad::lowpass(freq, 0.707, self.rate);
            self.low_r = Biquad::lowpass(freq, 0.707, self.rate);
        }
        let [out_l, out_r] = outs;
        let mut deepest = 0.0f32;
        for i in 0..ins[0].len() {
            let (l, r) = (ins[0][i], ins[1][i]);
            let (low_l, low_r) = (self.low_l.tick(l), self.low_r.tick(r));
            let (high_l, high_r) = (l - low_l, r - low_r);
            let level = lin_to_db(high_l.abs().max(high_r.abs()));
            follow(&mut self.reduction, compressor_gain(level, threshold, ratio, 0.0), attack, release);
            deepest = deepest.min(self.reduction);
            let gain = db_to_lin(self.reduction);
            out_l[i] = low_l + high_l * gain;
            out_r[i] = low_r + high_r * gain;
        }
        meters[0] = deepest;
    }
}

/// Moves a slow loudness average toward the target by raising or lowering
/// the gain, never past the maximum, and never raising it for silence.
struct AutoGain {
    rate: f32,
    mean_square: f32,
    gain_db: f32,
}

impl AutoGain {
    fn new(rate: f32) -> Self {
        Self {
            rate,
            mean_square: 0.0,
            gain_db: 0.0,
        }
    }
}

impl Dsp for AutoGain {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]) {
        let (target, max_gain, speed) = (ctl[0], ctl[1], ctl[2]);
        let average = time_constant(400.0, self.rate);
        // The gain moves by `speed` dB per second at most.
        let step = speed / self.rate;
        let [out_l, out_r] = outs;
        for i in 0..ins[0].len() {
            let (l, r) = (ins[0][i], ins[1][i]);
            self.mean_square = average * self.mean_square + (1.0 - average) * (l * l + r * r) * 0.5;
            let loudness = 10.0 * self.mean_square.max(1e-12).log10();
            let wanted = if loudness < -60.0 {
                self.gain_db
            } else {
                (target - loudness).clamp(-max_gain, max_gain)
            };
            self.gain_db += (wanted - self.gain_db).clamp(-step, step);
            let gain = db_to_lin(self.gain_db);
            out_l[i] = l * gain;
            out_r[i] = r * gain;
        }
        meters[0] = self.gain_db;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn sine(db: f32, freq: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| db_to_lin(db) * (2.0 * std::f32::consts::PI * freq * i as f32 / RATE).sin())
            .collect()
    }

    fn run(dsp: &mut dyn Dsp, input: &[f32], ctl: &[f32], meters: &mut [f32]) -> Vec<f32> {
        let mut out_l = vec![0.0; input.len()];
        let mut out_r = vec![0.0; input.len()];
        dsp.process([input, input], [&mut out_l, &mut out_r], ctl, meters);
        out_l
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |peak: f32, x| peak.max(x.abs()))
    }

    #[test]
    fn compressor_curve_matches_the_ratio() {
        // 14 dB over the threshold at 4:1 leaves 3.5 dB: 10.5 dB of reduction.
        assert!((compressor_gain(-6.0, -20.0, 4.0, 0.0) + 10.5).abs() < 1e-4);
        assert_eq!(compressor_gain(-30.0, -20.0, 4.0, 0.0), 0.0);
        // At the threshold itself a 6 dB knee has hardly started to reduce.
        assert!(compressor_gain(-20.0, -20.0, 4.0, 6.0) > -0.6);
    }

    #[test]
    fn compressor_settles_on_the_curve() {
        let mut dsp = Compressor::new(RATE);
        let input = sine(-6.0, 1000.0, 48_000);
        let mut meters = [0.0];
        let ctl = [-20.0, 4.0, 5.0, 100.0, 0.0, 0.0, 100.0];
        let out = run(&mut dsp, &input, &ctl, &mut meters);
        let tail = peak(&out[40_000..]);
        assert!((20.0 * tail.log10() + 16.5).abs() < 0.6, "peak {tail}");
        assert!(meters[0] < -9.0);
    }

    #[test]
    fn limiter_never_passes_the_threshold() {
        let mut dsp = Limiter::new(RATE);
        let mut input = sine(-30.0, 440.0, 24_000);
        input.extend(sine(6.0, 440.0, 24_000));
        let mut meters = [0.0];
        let out = run(&mut dsp, &input, &[-3.0, 100.0], &mut meters);
        assert!(peak(&out) <= db_to_lin(-3.0) * 1.001, "peak {}", peak(&out));
        assert!(meters[0] < -5.0);
    }

    #[test]
    fn gate_closes_on_quiet_and_opens_on_loud() {
        let mut dsp = Gate::new(RATE);
        let quiet = sine(-70.0, 1000.0, 24_000);
        let mut meters = [0.0];
        let ctl = [-45.0, 1.0, 10.0, 50.0, -60.0];
        let out = run(&mut dsp, &quiet, &ctl, &mut meters);
        assert!(peak(&out[12_000..]) < db_to_lin(-70.0) * 0.01);
        let loud = sine(-20.0, 1000.0, 24_000);
        let out = run(&mut dsp, &loud, &ctl, &mut meters);
        assert!(peak(&out[12_000..]) > db_to_lin(-20.0) * 0.95);
    }

    #[test]
    fn auto_gain_lifts_a_quiet_signal_toward_the_target() {
        let mut dsp = AutoGain::new(RATE);
        let input = sine(-34.0, 300.0, 48_000 * 20);
        let mut meters = [0.0];
        run(&mut dsp, &input, &[-24.0, 12.0, 2.0], &mut meters);
        assert!(meters[0] > 5.0, "gain {}", meters[0]);
    }
}
