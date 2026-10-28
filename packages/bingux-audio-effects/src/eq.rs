//! The frequency response of an equalizer, for drawing its curve.
//!
//! The coefficients are the RBJ "Audio EQ Cookbook" ones, which are also what
//! PipeWire's built-in biquads use, so the drawn curve is the curve that is
//! heard.

use crate::preset::{Band, BandKind};

/// Filter coefficients, normalised so `a0` is 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coefficients {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coefficients {
    /// The filter `band` is at sample rate `rate`.
    pub fn of(band: &Band, rate: f64) -> Self {
        let freq = band.frequency.clamp(10.0, rate * 0.499);
        let q = band.q.max(0.01);
        let w = 2.0 * std::f64::consts::PI * freq / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a = 10f64.powf(band.gain / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match band.kind {
            BandKind::Peaking => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            BandKind::LowShelf => {
                let beta = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + beta),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - beta),
                    (a + 1.0) + (a - 1.0) * cos + beta,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - beta,
                )
            }
            BandKind::HighShelf => {
                let beta = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + beta),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - beta),
                    (a + 1.0) - (a - 1.0) * cos + beta,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - beta,
                )
            }
            BandKind::LowPass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            BandKind::HighPass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            BandKind::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            BandKind::Notch => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            BandKind::AllPass => (
                1.0 - alpha,
                -2.0 * cos,
                1.0 + alpha,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// The gain of the filter at `freq` Hz, in dB.
    pub fn magnitude_db(&self, freq: f64, rate: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * freq / rate;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        let num = num_re * num_re + num_im * num_im;
        let den = den_re * den_re + den_im * den_im;
        10.0 * (num / den.max(1e-300)).max(1e-300).log10()
    }
}

/// The sample rate the curve is drawn for.
pub const RATE: f64 = 48_000.0;

/// The gain of a whole equalizer at `freq` Hz, in dB: the preamp plus every
/// enabled band.
pub fn response_db(preamp: f64, bands: &[Band], freq: f64) -> f64 {
    preamp
        + bands
            .iter()
            .filter(|band| band.enabled)
            .map(|band| Coefficients::of(band, RATE).magnitude_db(freq, RATE))
            .sum::<f64>()
}

/// `count` frequencies spaced evenly on a log scale from `low` to `high` Hz.
pub fn log_frequencies(low: f64, high: f64, count: usize) -> Vec<f64> {
    (0..count)
        .map(|i| low * (high / low).powf(i as f64 / (count - 1).max(1) as f64))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bell_has_its_gain_at_its_centre() {
        let band = Band::peaking(1000.0, 6.0, 1.0);
        let db = Coefficients::of(&band, RATE).magnitude_db(1000.0, RATE);
        assert!((db - 6.0).abs() < 0.01, "{db}");
        let far = Coefficients::of(&band, RATE).magnitude_db(60.0, RATE);
        assert!(far.abs() < 0.3, "{far}");
    }

    #[test]
    fn shelves_reach_their_gain_on_their_side() {
        let low = Band {
            kind: BandKind::LowShelf,
            frequency: 200.0,
            gain: 5.0,
            q: 0.707,
            enabled: true,
        };
        assert!((response_db(0.0, &[low.clone()], 20.0) - 5.0).abs() < 0.2);
        assert!(response_db(0.0, &[low], 10_000.0).abs() < 0.2);
    }

    #[test]
    fn a_high_pass_cuts_the_bass_and_a_disabled_band_does_nothing() {
        let mut band = Band {
            kind: BandKind::HighPass,
            frequency: 1000.0,
            gain: 0.0,
            q: 0.707,
            enabled: true,
        };
        assert!(response_db(0.0, &[band.clone()], 100.0) < -30.0);
        band.enabled = false;
        assert_eq!(response_db(-3.0, &[band], 100.0), -3.0);
    }
}
