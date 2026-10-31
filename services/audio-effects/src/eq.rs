//! Equalizer bands, in the plugin's own terms.
//!
//! The equalizer is the Linux Studio Plugins parametric equalizer. Each of its
//! 32 filters has a type, frequency, gain and quality factor for each side
//! (left and right, or mid and side), stored in the preset under the
//! plugin's port symbols (`ftl_3`, `fl_3`, `gl_3`, `ql_3`, and `ftr_3` and so
//! on for the right). This module is the one place that knows those names:
//! the page, the importers and the migration all go through it.

use std::collections::BTreeMap;

/// How many filters the plugin has.
pub const BANDS: usize = 32;

/// The ten bands a new equalizer starts with, as Easy Effects has them:
/// frequency in Hz.
pub const DEFAULT_FREQUENCIES: [f64; 10] = [
    30.0, 60.0, 119.0, 238.0, 475.0, 947.0, 1900.0, 3800.0, 7500.0, 15000.0,
];
/// The quality factor a new band has.
pub const DEFAULT_Q: f64 = 1.5;

/// What a filter does. The numbers are the plugin's `Filter type` choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandKind {
    Off = 0,
    Bell = 1,
    HighPass = 2,
    HighShelf = 3,
    LowPass = 4,
    LowShelf = 5,
    Notch = 6,
    Resonance = 7,
    AllPass = 8,
    BandPass = 9,
}

impl BandKind {
    pub fn code(self) -> f64 {
        f64::from(self as u8)
    }

    /// Whether the filter uses its gain.
    pub fn has_gain(self) -> bool {
        matches!(
            self,
            BandKind::Bell | BandKind::HighShelf | BandKind::LowShelf | BandKind::Resonance
        )
    }
}

/// Which side of the stereo image a set of bands is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Left, or mid in the mid/side equalizer.
    First,
    /// Right, or side in the mid/side equalizer.
    Second,
}

/// The port symbols of band `index` for `side`. `mid_side` selects the
/// mid/side equalizer's names (`ftm_0`) instead of left/right (`ftl_0`).
pub struct Symbols {
    pub kind: String,
    pub mode: String,
    pub slope: String,
    pub solo: String,
    pub mute: String,
    pub frequency: String,
    pub width: String,
    pub gain: String,
    pub quality: String,
}

impl Symbols {
    pub fn of(index: usize, side: Side, mid_side: bool) -> Self {
        let letter = match (side, mid_side) {
            (Side::First, false) => 'l',
            (Side::Second, false) => 'r',
            (Side::First, true) => 'm',
            (Side::Second, true) => 's',
        };
        Self {
            kind: format!("ft{letter}_{index}"),
            mode: format!("fm{letter}_{index}"),
            slope: format!("s{letter}_{index}"),
            solo: format!("xs{letter}_{index}"),
            mute: format!("xm{letter}_{index}"),
            frequency: format!("f{letter}_{index}"),
            width: format!("w{letter}_{index}"),
            gain: format!("g{letter}_{index}"),
            quality: format!("q{letter}_{index}"),
        }
    }
}

/// One filter, in the units a person reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub kind: BandKind,
    /// Hz.
    pub frequency: f64,
    /// dB.
    pub gain: f64,
    pub q: f64,
    /// 0 to 3: the filter is repeated 1 to 4 times for a steeper slope.
    pub slope: u8,
    pub mute: bool,
}

impl Band {
    pub fn new(kind: BandKind, frequency: f64, gain: f64, q: f64) -> Self {
        Self {
            kind,
            frequency,
            gain,
            q,
            slope: 0,
            mute: false,
        }
    }

    pub fn bell(frequency: f64, gain: f64, q: f64) -> Self {
        Self::new(BandKind::Bell, frequency, gain, q)
    }
}

fn linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// The controls that set `first` and `second` as the two sides' bands of an
/// equalizer: left and right, or mid and side when `mid_side`. Bands past the
/// last are switched off, so replacing a preset's bands leaves none of the old
/// ones behind.
pub fn controls_for(first: &[Band], second: &[Band], mid_side: bool) -> BTreeMap<String, f64> {
    let mut controls = BTreeMap::new();
    for index in 0..BANDS {
        for (side, bands) in [(Side::First, first), (Side::Second, second)] {
            let symbols = Symbols::of(index, side, mid_side);
            match bands.get(index) {
                Some(band) => {
                    controls.insert(symbols.kind, band.kind.code());
                    controls.insert(symbols.frequency, band.frequency.clamp(10.0, 24_000.0));
                    controls.insert(symbols.gain, linear(band.gain).clamp(0.015_85, 63.095_7));
                    controls.insert(symbols.quality, band.q.clamp(0.0, 100.0));
                    controls.insert(symbols.slope, f64::from(band.slope.min(3)));
                    controls.insert(symbols.mute, if band.mute { 1.0 } else { 0.0 });
                }
                None => {
                    controls.insert(symbols.kind, BandKind::Off.code());
                }
            }
        }
    }
    controls
}

/// The controls that set `bands` on both sides of a left/right equalizer.
pub fn controls(bands: &[Band]) -> BTreeMap<String, f64> {
    controls_for(bands, bands, false)
}

/// The bands a new equalizer starts with.
pub fn default_controls() -> BTreeMap<String, f64> {
    let bands: Vec<Band> = DEFAULT_FREQUENCIES
        .iter()
        .map(|&frequency| Band::bell(frequency, 0.0, DEFAULT_Q))
        .collect();
    controls(&bands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_become_the_plugins_port_symbols_for_both_sides() {
        let controls = controls(&[Band::bell(1000.0, 6.0, 2.0)]);
        assert_eq!(controls["ftl_0"], 1.0);
        assert_eq!(controls["ftr_0"], 1.0);
        assert_eq!(controls["fl_0"], 1000.0);
        assert!((controls["gl_0"] - 1.9953).abs() < 1e-3);
        assert_eq!(controls["ql_0"], 2.0);
        assert_eq!(controls["ftl_1"], 0.0, "bands after the last are off");
        assert!(!controls.contains_key("fl_1"));
    }

    #[test]
    fn mid_side_symbols_use_m_and_s() {
        assert_eq!(Symbols::of(3, Side::First, true).frequency, "fm_3");
        assert_eq!(Symbols::of(3, Side::Second, true).kind, "fts_3");
        assert_eq!(Symbols::of(12, Side::Second, false).gain, "gr_12");
    }

    #[test]
    fn a_new_equalizer_has_ten_flat_bands() {
        let controls = default_controls();
        let bells = (0..BANDS)
            .filter(|i| controls[&format!("ftl_{i}")] == 1.0)
            .count();
        assert_eq!(bells, 10);
        assert_eq!(controls["fl_4"], 475.0);
        assert!((controls["gl_4"] - 1.0).abs() < 1e-9);
    }
}
