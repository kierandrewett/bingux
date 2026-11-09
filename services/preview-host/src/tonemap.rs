//! Showing high dynamic range pictures on an ordinary display.
//!
//! A radiance file, an OpenEXR image and a 10-bit HDR10 or HLG photo hold
//! brightness an SDR screen cannot show. The picture is brought to display
//! referred linear light where 1.0 is paper white, its highlights are rolled off
//! with the extended Reinhard curve on luminance (so colours keep their hue),
//! its primaries are brought to sRGB's, and the result is encoded with the sRGB
//! curve. An SDR picture never comes through here: it is shown as it is.
//!
//! Everything is pure arithmetic on `f32` triples, tested without a file.

/// How bright a diffuse white is in an HDR10 picture, in nits (ITU-R BT.2408).
pub const REFERENCE_WHITE_NITS: f32 = 203.0;
/// The brightest a PQ signal goes.
pub const PQ_PEAK_NITS: f32 = 10_000.0;
/// The signal level of diffuse white in an HLG picture (BT.2408).
const HLG_REFERENCE_WHITE: f32 = 0.265;

/// How the picture's code values relate to light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    /// The sRGB curve: nothing to undo and nothing to tone map.
    Srgb,
    /// SMPTE ST 2084, as HDR10 and many AVIF files use.
    Pq,
    /// ARIB STD-B67, hybrid log-gamma.
    Hlg,
    /// Already linear light (an EXR, a Radiance file).
    Linear,
}

/// The colour primaries of the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primaries {
    Bt709,
    DisplayP3,
    Bt2020,
}

/// The ST 2084 curve: a code value in 0 to 1 to nits.
pub fn pq_to_nits(signal: f32) -> f32 {
    const M1: f32 = 2610.0 / 16384.0;
    const M2: f32 = 2523.0 / 4096.0 * 128.0;
    const C1: f32 = 3424.0 / 4096.0;
    const C2: f32 = 2413.0 / 4096.0 * 32.0;
    const C3: f32 = 2392.0 / 4096.0 * 32.0;
    let power = signal.clamp(0.0, 1.0).powf(1.0 / M2);
    let top = (power - C1).max(0.0);
    let bottom = C2 - C3 * power;
    PQ_PEAK_NITS * (top / bottom).powf(1.0 / M1)
}

/// The HLG inverse OETF: a code value in 0 to 1 to scene light in 0 to 1.
pub fn hlg_to_scene(signal: f32) -> f32 {
    const A: f32 = 0.178_832_77;
    const B: f32 = 0.284_668_92;
    const C: f32 = 0.559_910_7;
    let signal = signal.clamp(0.0, 1.0);
    if signal <= 0.5 {
        signal * signal / 3.0
    } else {
        (((signal - C) / A).exp() + B) / 12.0
    }
}

/// The sRGB encoding of linear light, clamped to 0 to 1.
pub fn srgb_encode(linear: f32) -> f32 {
    let linear = linear.clamp(0.0, 1.0);
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// The inverse of [`srgb_encode`].
pub fn srgb_decode(signal: f32) -> f32 {
    let signal = signal.clamp(0.0, 1.0);
    if signal <= 0.04045 {
        signal / 12.92
    } else {
        ((signal + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light in `primaries` to linear sRGB (Rec. 709) primaries.
pub fn to_srgb_primaries(rgb: [f32; 3], primaries: Primaries) -> [f32; 3] {
    let m: [[f32; 3]; 3] = match primaries {
        Primaries::Bt709 => return rgb,
        Primaries::DisplayP3 => [
            [1.224_940_2, -0.224_940_2, 0.0],
            [-0.042_056_96, 1.042_057, 0.0],
            [-0.019_637_6, -0.078_636_1, 1.098_273_7],
        ],
        Primaries::Bt2020 => [
            [1.660_491, -0.587_641_1, -0.072_849_9],
            [-0.124_550_5, 1.132_899_9, -0.008_349_4],
            [-0.018_150_8, -0.100_578_9, 1.118_729_7],
        ],
    };
    [
        m[0][0] * rgb[0] + m[0][1] * rgb[1] + m[0][2] * rgb[2],
        m[1][0] * rgb[0] + m[1][1] * rgb[1] + m[1][2] * rgb[2],
        m[2][0] * rgb[0] + m[2][1] * rgb[1] + m[2][2] * rgb[2],
    ]
}

fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// How the whole picture is exposed and where its highlights end, from the
/// picture itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exposure {
    /// What light is multiplied by before the curve.
    pub scale: f32,
    /// The scaled luminance that maps to white.
    pub white: f32,
}

/// Choose the exposure for linear `pixels` (RGB triples, 1.0 is paper white
/// for display-referred input). `fixed_scale` keeps a display-referred
/// picture's own brightness; a scene-referred one (`None`) is exposed so its
/// log-average luminance lands on middle grey.
pub fn exposure_of(pixels: &[[f32; 3]], fixed_scale: Option<f32>) -> Exposure {
    if pixels.is_empty() {
        return Exposure {
            scale: 1.0,
            white: 1.0,
        };
    }
    // Sample at most 64k pixels evenly: the statistic does not need more.
    let stride = (pixels.len() / 65_536).max(1);
    let mut logs = 0.0f64;
    let mut seen = 0u32;
    let mut lumas: Vec<f32> = Vec::with_capacity(65_536);
    for pixel in pixels.iter().step_by(stride) {
        let luma = luminance(*pixel).max(0.0);
        lumas.push(luma);
        logs += f64::from((luma + 1e-4).ln());
        seen += 1;
    }
    let log_average = (logs / f64::from(seen.max(1))).exp() as f32;
    let scale = fixed_scale.unwrap_or_else(|| (0.18 / log_average.max(1e-4)).clamp(0.25, 16.0));
    lumas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let high = lumas[((lumas.len() as f32 * 0.995) as usize).min(lumas.len() - 1)] * scale;
    // The white point is never below what already fits, so an SDR-range
    // picture is left alone.
    Exposure {
        scale,
        white: high.max(1.0),
    }
}

/// The extended Reinhard curve on luminance: 0 stays 0, `white` maps to 1.
pub fn reinhard(luma: f32, white: f32) -> f32 {
    luma * (1.0 + luma / (white * white)) / (1.0 + luma)
}

/// Bring one linear pixel to an sRGB-encoded one in 0 to 1.
pub fn map_pixel(rgb: [f32; 3], exposure: Exposure) -> [f32; 3] {
    let scaled = [
        rgb[0] * exposure.scale,
        rgb[1] * exposure.scale,
        rgb[2] * exposure.scale,
    ];
    let luma = luminance(scaled);
    if luma <= 1e-6 {
        return [
            srgb_encode(scaled[0].max(0.0)),
            srgb_encode(scaled[1].max(0.0)),
            srgb_encode(scaled[2].max(0.0)),
        ];
    }
    let target = reinhard(luma, exposure.white);
    let gain = target / luma;
    [
        srgb_encode(scaled[0] * gain),
        srgb_encode(scaled[1] * gain),
        srgb_encode(scaled[2] * gain),
    ]
}

/// Code values (0 to 1 per channel) with `transfer` to display-referred linear
/// light in `primaries`, ready for [`exposure_of`] and [`map_pixel`]. The
/// scale that keeps the picture's own brightness for PQ and HLG is returned
/// with it; a linear picture is scene-referred and has none.
pub fn linearise(
    signal: [f32; 3],
    transfer: Transfer,
    primaries: Primaries,
) -> ([f32; 3], Option<f32>) {
    let (linear, fixed) = match transfer {
        Transfer::Srgb => (
            [
                srgb_decode(signal[0]),
                srgb_decode(signal[1]),
                srgb_decode(signal[2]),
            ],
            Some(1.0),
        ),
        Transfer::Pq => {
            let nits = |channel: f32| pq_to_nits(channel) / REFERENCE_WHITE_NITS;
            (
                [nits(signal[0]), nits(signal[1]), nits(signal[2])],
                Some(1.0),
            )
        }
        Transfer::Hlg => {
            let scene = |channel: f32| hlg_to_scene(channel) / (hlg_to_scene(HLG_REFERENCE_WHITE));
            (
                [scene(signal[0]), scene(signal[1]), scene(signal[2])],
                Some(1.0),
            )
        }
        Transfer::Linear => (signal, None),
    };
    (to_srgb_primaries(linear, primaries), fixed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn pq_of_zero_is_black_and_of_one_is_the_peak() {
        assert!(pq_to_nits(0.0) < 1e-3);
        assert!(
            close(pq_to_nits(1.0), PQ_PEAK_NITS, 5.0),
            "{}",
            pq_to_nits(1.0)
        );
        // About 58 % of the signal is diffuse white at 203 nits.
        assert!(close(pq_to_nits(0.58), 203.0, 12.0), "{}", pq_to_nits(0.58));
    }

    #[test]
    fn hlg_runs_from_zero_to_one_and_is_monotonic() {
        assert!(close(hlg_to_scene(0.0), 0.0, 1e-6));
        assert!(close(hlg_to_scene(1.0), 1.0, 1e-3), "{}", hlg_to_scene(1.0));
        let mut last = -1.0;
        for step in 0..=100 {
            let value = hlg_to_scene(step as f32 / 100.0);
            assert!(value >= last);
            last = value;
        }
    }

    #[test]
    fn the_srgb_curve_round_trips() {
        for step in 0..=20 {
            let signal = step as f32 / 20.0;
            assert!(
                close(srgb_encode(srgb_decode(signal)), signal, 1e-4),
                "{signal}"
            );
        }
    }

    #[test]
    fn white_in_any_primaries_stays_white() {
        for primaries in [Primaries::Bt709, Primaries::DisplayP3, Primaries::Bt2020] {
            let white = to_srgb_primaries([1.0, 1.0, 1.0], primaries);
            assert!(
                white.iter().all(|channel| close(*channel, 1.0, 5e-3)),
                "{primaries:?} {white:?}"
            );
        }
    }

    #[test]
    fn the_curve_keeps_black_rolls_off_highlights_and_maps_the_white_point_to_one() {
        assert_eq!(reinhard(0.0, 4.0), 0.0);
        assert!(close(reinhard(4.0, 4.0), 1.0, 1e-5));
        // Below white it is monotonic.
        assert!(reinhard(1.0, 4.0) < reinhard(2.0, 4.0));
        // A picture already inside the display's range keeps its mid-tones.
        let exposure = Exposure {
            scale: 1.0,
            white: 1.0,
        };
        let mid = map_pixel([0.18, 0.18, 0.18], exposure);
        assert!(mid[0] > 0.4 && mid[0] < 0.6, "{mid:?}");
    }

    #[test]
    fn a_dim_scene_referred_picture_is_exposed_up_and_a_bright_one_down() {
        let dim = vec![[0.01, 0.01, 0.01]; 100];
        let bright = vec![[4.0, 4.0, 4.0]; 100];
        assert!(exposure_of(&dim, None).scale > 10.0);
        assert!(exposure_of(&bright, None).scale < 0.25 + 1e-6);
        // A display-referred picture keeps its brightness.
        assert_eq!(exposure_of(&dim, Some(1.0)).scale, 1.0);
        assert_eq!(
            exposure_of(&[], None),
            Exposure {
                scale: 1.0,
                white: 1.0
            }
        );
    }

    #[test]
    fn hue_survives_the_curve() {
        let exposure = Exposure {
            scale: 1.0,
            white: 8.0,
        };
        let mapped = map_pixel([6.0, 3.0, 0.0], exposure);
        // Red stays the strongest, blue the weakest.
        assert!(mapped[0] > mapped[1] && mapped[1] > mapped[2], "{mapped:?}");
        assert!(mapped.iter().all(|channel| (0.0..=1.0).contains(channel)));
    }

    #[test]
    fn hdr_reference_white_lands_at_paper_white() {
        let (linear, fixed) = linearise([0.58, 0.58, 0.58], Transfer::Pq, Primaries::Bt709);
        assert!(close(linear[0], 1.0, 0.07), "{linear:?}");
        assert_eq!(fixed, Some(1.0));
        let (_, scene) = linearise([0.5, 0.5, 0.5], Transfer::Linear, Primaries::Bt709);
        assert_eq!(scene, None);
    }
}
