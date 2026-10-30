//! Colour contrast: WCAG 2.2 relative luminance and ratio, an optional APCA
//! reading, and the functions that derive a readable ink from a surface.
//!
//! Everything here is pure: colours in, numbers or colours out. The token
//! matrix ([`crate::contrast_matrix`]) and the inspector's Contrast check both
//! measure with it, so the design system and the running app agree.
//!
//! WCAG levels used by Bingux UI:
//!
//! | Requirement | Ratio | Where |
//! |---|---|---|
//! | AA text | 4.5:1 | text under 24 px (or under 18.66 px bold) |
//! | AA large text | 3:1 | text of 24 px and up |
//! | AA non-text | 3:1 | icons, focus rings, the edge of a control that needs one |
//! | AAA text | 7:1 | the goal for ink on a filled surface; reached where the hue allows |
//!
//! APCA (the WCAG 3 candidate) is reported beside the ratio and labelled
//! advisory. It never fails a pair.

use freya::prelude::Color;

/// A WCAG conformance requirement and the ratio it needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Requirement {
    /// Normal text, WCAG 1.4.3 AA: 4.5:1.
    Text,
    /// Large text, WCAG 1.4.3 AA: 3:1.
    LargeText,
    /// Icons, focus rings and component boundaries, WCAG 1.4.11 AA: 3:1.
    Component,
    /// Enhanced text, WCAG 1.4.6 AAA: 7:1.
    Enhanced,
}

impl Requirement {
    /// The least ratio that meets the requirement.
    pub const fn min(self) -> f32 {
        match self {
            Requirement::Text => 4.5,
            Requirement::LargeText | Requirement::Component => 3.0,
            Requirement::Enhanced => 7.0,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Requirement::Text => "AA text 4.5",
            Requirement::LargeText => "AA large 3",
            Requirement::Component => "AA UI 3",
            Requirement::Enhanced => "AAA 7",
        }
    }
}

/// Ratio for AA normal text.
pub const AA_TEXT: f32 = 4.5;
/// Ratio for AA large text and non-text.
pub const AA_LARGE: f32 = 3.0;
/// Ratio for AAA normal text.
pub const AAA_TEXT: f32 = 7.0;

/// The highest WCAG text level a ratio reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Fail,
    /// 3:1 or more: passes for large text and UI components.
    AaLarge,
    /// 4.5:1 or more.
    Aa,
    /// 7:1 or more.
    Aaa,
}

impl Level {
    pub fn of(ratio: f32) -> Self {
        if ratio >= AAA_TEXT {
            Level::Aaa
        } else if ratio >= AA_TEXT {
            Level::Aa
        } else if ratio >= AA_LARGE {
            Level::AaLarge
        } else {
            Level::Fail
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Fail => "fail",
            Level::AaLarge => "AA large",
            Level::Aa => "AA",
            Level::Aaa => "AAA",
        }
    }
}

/// A colour's channels as fractions of full scale.
pub fn channels(colour: Color) -> [f32; 3] {
    [colour.r(), colour.g(), colour.b()].map(|channel| f32::from(channel) / 255.0)
}

/// `top` composited over `bottom`, which is taken as opaque: the colour a
/// viewer sees. The result is opaque.
pub fn composite(top: Color, bottom: Color) -> Color {
    let alpha = f32::from(top.a()) / 255.0;
    let (t, b) = (channels(top), channels(bottom));
    let mixed = [0, 1, 2].map(|i| t[i] * alpha + b[i] * (1.0 - alpha));
    from_channels(mixed)
}

/// An opaque colour from float channels, rounded to 8 bits.
pub fn from_channels(rgb: [f32; 3]) -> Color {
    let to8 = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::from((to8(rgb[0]), to8(rgb[1]), to8(rgb[2]), 255u8))
}

fn linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn delinear(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// WCAG 2.2 relative luminance of an opaque sRGB colour, 0 (black) to 1.
/// Alpha is ignored; composite first.
pub fn luminance(colour: Color) -> f32 {
    let [r, g, b] = channels(colour).map(linear);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// The WCAG contrast ratio, 1 to 21, between two opaque colours.
pub fn ratio_opaque(a: Color, b: Color) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The contrast a viewer gets from `fg` drawn over the opaque `bg`: a
/// translucent `fg` is composited over `bg` first, so the ratio is of what is
/// on screen. `bg` must be opaque; composite translucent fills yourself with
/// [`composite`], walking down from the window.
pub fn ratio(fg: Color, bg: Color) -> f32 {
    ratio_opaque(composite(fg, bg), bg)
}

/// The smallest ratio of `fg` against each of `backgrounds`.
pub fn worst_ratio(fg: Color, backgrounds: &[Color]) -> f32 {
    backgrounds
        .iter()
        .map(|bg| ratio(fg, *bg))
        .fold(f32::INFINITY, f32::min)
}

/// APCA lightness contrast (Lc) of text `fg` on `bg`, version 0.0.98G-4g,
/// as a signed number: positive for dark text on a light ground, negative
/// for light on dark. About 60 reads body text, 75 is preferred for small
/// text. Advisory: it is shown next to the WCAG ratio and never fails a pair.
pub fn apca(fg: Color, bg: Color) -> f32 {
    fn screen_luminance(colour: Color) -> f32 {
        let [r, g, b] = channels(colour).map(|c| c.powf(2.4));
        let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
        if y < 0.022 { y + (0.022 - y).powf(1.414) } else { y }
    }
    let text = screen_luminance(composite(fg, bg));
    let ground = screen_luminance(bg);
    if (ground - text).abs() < 0.0005 {
        return 0.0;
    }
    if ground > text {
        let sapc = (ground.powf(0.56) - text.powf(0.57)) * 1.14;
        if sapc < 0.1 { 0.0 } else { (sapc - 0.027) * 100.0 }
    } else {
        let sapc = (ground.powf(0.65) - text.powf(0.62)) * 1.14;
        if sapc > -0.1 { 0.0 } else { (sapc + 0.027) * 100.0 }
    }
}

// ---- OKLCH ---------------------------------------------------------------

/// A colour as OKLab lightness, chroma and hue (degrees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

impl Oklch {
    pub fn from_colour(colour: Color) -> Self {
        let [r, g, b] = channels(colour).map(linear);
        let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
        let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
        let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
        Self {
            l: lightness,
            c: a.hypot(bb),
            h: bb.atan2(a).to_degrees().rem_euclid(360.0),
        }
    }

    /// The linear sRGB channels, which may fall outside 0..1.
    fn linear_rgb(self) -> [f32; 3] {
        let (sin, cos) = self.h.to_radians().sin_cos();
        let (a, b) = (self.c * cos, self.c * sin);
        let l = (self.l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
        let m = (self.l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
        let s = (self.l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
        [
            4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
            -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
            -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
        ]
    }

    /// The colour, with chroma reduced until it fits the sRGB gamut so the
    /// hue and lightness stay as asked.
    pub fn to_colour(self) -> Color {
        let fits = |rgb: [f32; 3]| rgb.iter().all(|c| (-0.000_5..=1.000_5).contains(c));
        let mut colour = self;
        if !fits(colour.linear_rgb()) {
            let (mut low, mut high) = (0.0_f32, self.c);
            for _ in 0..20 {
                let mid = (low + high) / 2.0;
                if fits((Oklch { c: mid, ..self }).linear_rgb()) {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            colour.c = low;
        }
        from_channels(colour.linear_rgb().map(delinear))
    }
}

/// How much of the surface's chroma the derived ink keeps: enough to read as
/// a darker shade of the same hue, little enough to stay near neutral.
const INK_CHROMA_KEEP: f32 = 0.55;
/// Largest chroma of a derived ink.
const INK_CHROMA_MAX: f32 = 0.09;
/// The darkest and lightest shades the search falls back to, as OKLab
/// lightness: near-black and near-white, tinted with the surface hue.
const NEAR_BLACK_L: f32 = 0.16;
const NEAR_WHITE_L: f32 = 0.985;

/// The same hue as `of`, at lightness `l`, with the ink chroma rule.
fn shade(of: Oklch, l: f32) -> Color {
    Oklch {
        l,
        c: (of.c * INK_CHROMA_KEEP).min(INK_CHROMA_MAX),
        h: of.h,
    }
    .to_colour()
}

/// The ink for text and icons on a filled surface: `bg` darkened in OKLCH,
/// same hue and much lower lightness, so it follows whatever accent `bg`
/// came from.
///
/// It searches lightness (binary search, ratio falls monotonically as the ink
/// gets lighter) for the lightest shade that reaches AAA (7:1) against the
/// surface, or AA (4.5:1) when the hue cannot reach AAA. When even a
/// near-black shade misses AA, as it does on a dark fill, it falls back to
/// near-black or near-white, whichever has the higher ratio.
///
/// A surface that gets a hover or pressed overlay should use
/// [`on_colour_over`] so the ink holds for those states as well.
pub fn on_colour(bg: Color) -> Color {
    on_colour_over(&[bg])
}

/// [`on_colour`] for a surface that takes several colours (rest, hover,
/// pressed): the ink reaches its target against every one of them.
pub fn on_colour_over(states: &[Color]) -> Color {
    let base = Oklch::from_colour(states[0]);
    let reaches = |ink: Color, target: f32| worst_ratio(ink, states) >= target;
    for target in [AAA_TEXT, AA_TEXT] {
        let lightest_dark = Oklch::from_colour(states[0]).l;
        let darkest = shade(base, 0.0);
        if !reaches(darkest, target) {
            continue;
        }
        let (mut low, mut high) = (0.0_f32, lightest_dark);
        if reaches(shade(base, high), target) {
            // The surface itself is the ink: only a degenerate surface.
            low = high;
        }
        for _ in 0..24 {
            let mid = (low + high) / 2.0;
            if reaches(shade(base, mid), target) {
                low = mid;
            } else {
                high = mid;
            }
        }
        return shade(base, low);
    }
    let dark = shade(base, NEAR_BLACK_L);
    let light = shade(base, NEAR_WHITE_L);
    if worst_ratio(dark, states) >= worst_ratio(light, states) { dark } else { light }
}

/// `ink` moved along lightness, keeping hue and chroma, until it reaches
/// `target` against every surface in `surfaces`; unchanged when it already
/// does. It darkens on light surfaces and lightens on dark ones, and falls
/// back to the extreme of that direction when `target` is out of reach.
///
/// This is how a coloured text token (accent, danger, success, warning) is
/// kept readable on every layer of a scheme while keeping its hue.
pub fn ensure_contrast(ink: Color, surfaces: &[Color], target: f32) -> Color {
    if worst_ratio(ink, surfaces) >= target {
        return ink;
    }
    let from = Oklch::from_colour(ink);
    let mean = surfaces.iter().map(|s| luminance(*s)).sum::<f32>() / surfaces.len() as f32;
    let goal = if mean > 0.18 { 0.0 } else { 1.0 };
    let at = |l: f32| Oklch { l, ..from }.to_colour();
    if worst_ratio(at(goal), surfaces) < target {
        return at(goal);
    }
    let (mut near, mut far) = (from.l, goal);
    for _ in 0..24 {
        let mid = (near + far) / 2.0;
        if worst_ratio(at(mid), surfaces) >= target {
            far = mid;
        } else {
            near = mid;
        }
    }
    at(far)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(value: u32) -> Color {
        Color::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }

    #[test]
    fn black_on_white_is_twenty_one() {
        assert!((ratio(rgb(0), rgb(0xffffff)) - 21.0).abs() < 0.01);
        assert!((ratio(rgb(0xffffff), rgb(0)) - 21.0).abs() < 0.01);
    }

    #[test]
    fn identical_colours_are_one() {
        assert!((ratio(rgb(0x3584e4), rgb(0x3584e4)) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn wcag_reference_pairs() {
        // The classic AA boundary: #767676 on white is 4.54:1, #777777 4.48:1.
        assert!((ratio(rgb(0x767676), rgb(0xffffff)) - 4.54).abs() < 0.01);
        assert!((ratio(rgb(0x777777), rgb(0xffffff)) - 4.48).abs() < 0.01);
        // #595959 on white is 7.0:1, the AAA boundary.
        assert!((ratio(rgb(0x595959), rgb(0xffffff)) - 7.0).abs() < 0.02);
        // Pure blue on white is 8.59:1; pure red on white is 4.0:1.
        assert!((ratio(rgb(0x0000ff), rgb(0xffffff)) - 8.59).abs() < 0.01);
        assert!((ratio(rgb(0xff0000), rgb(0xffffff)) - 4.0).abs() < 0.01);
        // #949494 on white is 3.03:1, the 3:1 non-text boundary.
        assert!((ratio(rgb(0x949494), rgb(0xffffff)) - 3.03).abs() < 0.01);
    }

    #[test]
    fn luminance_linearises_srgb() {
        assert!((luminance(rgb(0xffffff)) - 1.0).abs() < 1e-4);
        assert!(luminance(rgb(0)).abs() < 1e-6);
        // Mid grey #808080 has luminance 0.2159.
        assert!((luminance(rgb(0x808080)) - 0.2159).abs() < 0.001);
    }

    #[test]
    fn a_translucent_foreground_is_composited_over_the_background() {
        // 50% black over white is #808080 (rounded): 3.95:1 against white.
        let half_black = Color::from_argb(128, 0, 0, 0);
        let expected = ratio_opaque(rgb(0x808080), rgb(0xffffff));
        assert!((ratio(half_black, rgb(0xffffff)) - expected).abs() < 0.1);
        // Fully transparent ink is invisible.
        assert!((ratio(Color::TRANSPARENT, rgb(0xffffff)) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn apca_matches_reference_extremes() {
        // Published reference values: black on white Lc 106, white on black -108.
        assert!((apca(rgb(0), rgb(0xffffff)) - 106.0).abs() < 1.0);
        assert!((apca(rgb(0xffffff), rgb(0)) + 107.9).abs() < 1.0);
        assert_eq!(apca(rgb(0x808080), rgb(0x808080)), 0.0);
    }

    #[test]
    fn levels_split_at_the_wcag_thresholds() {
        assert_eq!(Level::of(2.99), Level::Fail);
        assert_eq!(Level::of(3.0), Level::AaLarge);
        assert_eq!(Level::of(4.49), Level::AaLarge);
        assert_eq!(Level::of(4.5), Level::Aa);
        assert_eq!(Level::of(7.0), Level::Aaa);
        assert_eq!(Requirement::Text.min(), 4.5);
        assert_eq!(Requirement::Component.min(), 3.0);
    }

    #[test]
    fn oklch_round_trips() {
        for value in [0x3584e4, 0xe62d42, 0xc88800, 0x222226, 0xffffff, 0x000000] {
            let back = Oklch::from_colour(rgb(value)).to_colour();
            assert!((ratio_opaque(back, rgb(value)) - 1.0).abs() < 0.01, "{value:06x}");
        }
    }

    #[test]
    fn on_colour_keeps_the_hue_and_reaches_aa_on_a_light_fill() {
        let fill = rgb(0x3584e4);
        let ink = on_colour(fill);
        assert!(ratio(ink, fill) >= AA_TEXT, "{}", ratio(ink, fill));
        assert!(Oklch::from_colour(ink).l < Oklch::from_colour(fill).l);
        let (a, b) = (Oklch::from_colour(ink), Oklch::from_colour(fill));
        assert!((a.h - b.h).abs() < 12.0, "{} vs {}", a.h, b.h);
    }

    #[test]
    fn on_colour_reaches_aaa_on_a_pale_fill() {
        let fill = rgb(0xa4c8f4);
        assert!(ratio(on_colour(fill), fill) >= AAA_TEXT);
    }

    #[test]
    fn on_colour_falls_back_to_near_white_on_a_dark_fill() {
        let fill = rgb(0xc01c28);
        let ink = on_colour(fill);
        assert!(luminance(ink) > luminance(fill));
        assert!(ratio(ink, fill) >= AA_TEXT);
    }

    #[test]
    fn on_colour_over_holds_for_every_state() {
        let fill = rgb(0x3584e4);
        let states = [fill, composite(Color::from_argb(13, 0, 0, 0), fill)];
        let ink = on_colour_over(&states);
        assert!(worst_ratio(ink, &states) >= AA_TEXT);
    }

    #[test]
    fn ensure_contrast_moves_lightness_only() {
        let ink = rgb(0x26a269);
        let white = rgb(0xffffff);
        let fixed = ensure_contrast(ink, &[white], AA_TEXT);
        assert!(ratio(fixed, white) >= AA_TEXT);
        assert!(ratio(ensure_contrast(rgb(0x000000), &[white], AA_TEXT), white) >= 21.0 - 0.1);
        // Already readable: unchanged.
        assert_eq!(ensure_contrast(rgb(0x222222), &[white], AA_TEXT), rgb(0x222222));
    }
}
