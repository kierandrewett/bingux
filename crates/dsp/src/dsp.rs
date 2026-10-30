//! What a plugin is, apart from how a host talks to it.

/// Where a port sits and what it carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    AudioIn,
    AudioOut,
    /// A setting the host writes.
    ControlIn,
    /// A reading, such as gain reduction, the plugin writes.
    ControlOut,
}

/// How a control looks to the host. These match LADSPA's range hints.
pub const LOG: u32 = 0x10;
pub const INTEGER: u32 = 0x20;
pub const TOGGLED: u32 = 0x4;

/// One port of a plugin.
#[derive(Clone, Debug)]
pub struct PortSpec {
    pub name: &'static str,
    pub kind: Kind,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// `LOG`, `INTEGER` or `TOGGLED` bits.
    pub hint: u32,
}

impl PortSpec {
    pub const fn audio_in(name: &'static str) -> Self {
        Self::new(name, Kind::AudioIn, 0.0, 0.0, 0.0, 0)
    }
    pub const fn audio_out(name: &'static str) -> Self {
        Self::new(name, Kind::AudioOut, 0.0, 0.0, 0.0, 0)
    }
    pub const fn control(name: &'static str, min: f32, max: f32, default: f32) -> Self {
        Self::new(name, Kind::ControlIn, min, max, default, 0)
    }
    pub const fn log(mut self) -> Self {
        self.hint |= LOG;
        self
    }
    pub const fn integer(mut self) -> Self {
        self.hint |= INTEGER;
        self
    }
    pub const fn toggle(name: &'static str, default: bool) -> Self {
        Self::new(
            name,
            Kind::ControlIn,
            0.0,
            1.0,
            if default { 1.0 } else { 0.0 },
            TOGGLED,
        )
    }
    pub const fn meter(name: &'static str, min: f32, max: f32) -> Self {
        Self::new(name, Kind::ControlOut, min, max, 0.0, 0)
    }
    const fn new(name: &'static str, kind: Kind, min: f32, max: f32, default: f32, hint: u32) -> Self {
        Self {
            name,
            kind,
            min,
            max,
            default,
            hint,
        }
    }
}

/// The stereo audio ports every plugin starts with.
pub fn stereo() -> Vec<PortSpec> {
    vec![
        PortSpec::audio_in("Input L"),
        PortSpec::audio_in("Input R"),
        PortSpec::audio_out("Output L"),
        PortSpec::audio_out("Output R"),
    ]
}

/// One stereo effect. `process` gets the two input channels, fills the two
/// output channels, reads the controls in the order the ports list them and
/// writes the meters in their order. Inputs never alias outputs.
pub trait Dsp {
    fn process(&mut self, ins: [&[f32]; 2], outs: [&mut [f32]; 2], ctl: &[f32], meters: &mut [f32]);
}

/// A plugin: its names, its ports and how to make one.
pub struct Spec {
    pub id: u32,
    pub label: &'static str,
    pub name: &'static str,
    pub ports: Vec<PortSpec>,
    pub make: fn(sample_rate: f32) -> Box<dyn Dsp>,
}

/// Decibels to a linear gain.
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// A linear amplitude to decibels, floored at -180 dB.
pub fn lin_to_db(lin: f32) -> f32 {
    20.0 * lin.max(1e-9).log10()
}

/// The one-pole coefficient for a time constant of `ms` milliseconds: the
/// share of the old value kept each sample.
pub fn time_constant(ms: f32, rate: f32) -> f32 {
    (-1.0 / (ms.max(0.01) * 0.001 * rate)).exp()
}

/// A second-order filter (RBJ cookbook), with its state.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn from(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn setup(freq: f32, rate: f32, q: f32) -> (f64, f64, f64) {
        let w = 2.0 * std::f64::consts::PI * f64::from(freq.clamp(10.0, rate * 0.45)) / f64::from(rate);
        (w.cos(), w.sin(), w.sin() / (2.0 * f64::from(q.max(0.05))))
    }

    pub fn lowpass(freq: f32, q: f32, rate: f32) -> Self {
        let (c, _, a) = Self::setup(freq, rate, q);
        Self::from((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + a, -2.0 * c, 1.0 - a)
    }

    pub fn highpass(freq: f32, q: f32, rate: f32) -> Self {
        let (c, _, a) = Self::setup(freq, rate, q);
        Self::from((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + a, -2.0 * c, 1.0 - a)
    }

    pub fn low_shelf(freq: f32, gain_db: f32, rate: f32) -> Self {
        let (c, s, _) = Self::setup(freq, rate, 0.707);
        let big_a = 10f64.powf(f64::from(gain_db) / 40.0);
        let beta = 2.0 * big_a.sqrt() * (s / 2.0 * (2.0f64).sqrt());
        Self::from(
            big_a * ((big_a + 1.0) - (big_a - 1.0) * c + beta),
            2.0 * big_a * ((big_a - 1.0) - (big_a + 1.0) * c),
            big_a * ((big_a + 1.0) - (big_a - 1.0) * c - beta),
            (big_a + 1.0) + (big_a - 1.0) * c + beta,
            -2.0 * ((big_a - 1.0) + (big_a + 1.0) * c),
            (big_a + 1.0) + (big_a - 1.0) * c - beta,
        )
    }

    pub fn high_shelf(freq: f32, gain_db: f32, rate: f32) -> Self {
        let (c, s, _) = Self::setup(freq, rate, 0.707);
        let big_a = 10f64.powf(f64::from(gain_db) / 40.0);
        let beta = 2.0 * big_a.sqrt() * (s / 2.0 * (2.0f64).sqrt());
        Self::from(
            big_a * ((big_a + 1.0) + (big_a - 1.0) * c + beta),
            -2.0 * big_a * ((big_a - 1.0) + (big_a + 1.0) * c),
            big_a * ((big_a + 1.0) + (big_a - 1.0) * c - beta),
            (big_a + 1.0) - (big_a - 1.0) * c + beta,
            2.0 * ((big_a - 1.0) - (big_a + 1.0) * c),
            (big_a + 1.0) - (big_a - 1.0) * c - beta,
        )
    }

    /// Filter one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = f64::from(x);
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y as f32
    }
}
