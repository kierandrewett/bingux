//! The person's accessibility preferences: reduce motion, increase contrast,
//! reduce transparency and the text size. Every component follows them
//! through the tokens and the type ramp; an app never reads them itself.
//!
//! * **Reduce motion** makes every animation instant ([`crate::theme::Tokens::motion`]).
//! * **Increase contrast** derives a high-contrast palette: text and secondary
//!   text at 7:1 (WCAG AAA), separators and control edges that show, a
//!   stronger focus ring, and an outline on every control fill.
//! * **Reduce transparency** makes the surfaces that let content show through
//!   (the frosted header over a scrolled page) opaque.
//! * **Text size** multiplies the whole type ramp, 100 % to 200 %; rows,
//!   controls and icons in a line of text grow with it, and layouts reflow
//!   (WCAG 1.4.4). Sizes that are not text (window chrome, spacing) stay.
//!
//! The preferences are process-wide, like the text they size. The platform
//! source (`theme.rs` reads the portal Settings, `bingux.toml` and
//! `gsettings`) calls [`set`]; until it has, they come from the environment
//! ([`Access::from_env`]): `BINGUX_REDUCED_MOTION=1`, `BINGUX_HIGH_CONTRAST=1`,
//! `BINGUX_REDUCE_TRANSPARENCY=1` and `BINGUX_TEXT_SCALE=1.5`.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The text sizes the stress tools and the taxonomy's switcher try, as
/// factors of the type ramp; 2 is the largest WCAG 1.4.4 asks a layout to
/// survive. Settings offers 80 % to 200 %.
pub const TEXT_SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

/// The smallest and largest text scale accepted from any source.
pub const TEXT_SCALE_RANGE: (f32, f32) = (0.75, 3.0);

/// The accessibility preferences in force.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Access {
    pub reduced_motion: bool,
    pub high_contrast: bool,
    pub reduce_transparency: bool,
    /// The factor the type ramp is multiplied by: 1 is the designed size.
    pub text_scale: f32,
}

impl Default for Access {
    fn default() -> Self {
        Self {
            reduced_motion: false,
            high_contrast: false,
            reduce_transparency: false,
            text_scale: 1.0,
        }
    }
}

impl Access {
    /// The preferences the environment sets, each off (and the text at
    /// 100 %) when its variable is missing or not understood.
    pub fn from_env() -> Self {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let flag = |name: &str| var(name).is_some_and(|value| value.trim() == "1");
        Self {
            reduced_motion: flag("BINGUX_REDUCED_MOTION"),
            high_contrast: flag("BINGUX_HIGH_CONTRAST"),
            reduce_transparency: flag("BINGUX_REDUCE_TRANSPARENCY"),
            text_scale: var("BINGUX_TEXT_SCALE")
                .and_then(|value| value.trim().parse::<f32>().ok())
                .map(clamp_scale)
                .unwrap_or(1.0),
        }
    }

    /// The preferences in force now.
    pub fn current() -> Self {
        Self {
            reduced_motion: STATE.reduced_motion.load(Ordering::Relaxed),
            high_contrast: STATE.high_contrast.load(Ordering::Relaxed),
            reduce_transparency: STATE.reduce_transparency.load(Ordering::Relaxed),
            text_scale: text_scale(),
        }
    }

    /// The text scale as a whole percentage, for a label ("150 %").
    pub fn text_percent(&self) -> u32 {
        (self.text_scale * 100.0).round() as u32
    }
}

/// A text scale from any source, held to [`TEXT_SCALE_RANGE`]; not a number
/// is 1.
pub fn clamp_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(TEXT_SCALE_RANGE.0, TEXT_SCALE_RANGE.1)
    } else {
        1.0
    }
}

struct State {
    reduced_motion: AtomicBool,
    high_contrast: AtomicBool,
    reduce_transparency: AtomicBool,
    text_scale: AtomicU32,
}

static STATE: LazyLock<State> = LazyLock::new(|| {
    let access = Access::from_env();
    State {
        reduced_motion: AtomicBool::new(access.reduced_motion),
        high_contrast: AtomicBool::new(access.high_contrast),
        reduce_transparency: AtomicBool::new(access.reduce_transparency),
        text_scale: AtomicU32::new(access.text_scale.to_bits()),
    }
});

/// Puts `access` in force. New tokens ([`crate::theme::Tokens::new`]) and
/// every text size measured after this follow it; the window must be drawn
/// again, which a change of its shared tokens does.
pub fn set(access: Access) {
    STATE
        .reduced_motion
        .store(access.reduced_motion, Ordering::Relaxed);
    STATE
        .high_contrast
        .store(access.high_contrast, Ordering::Relaxed);
    STATE
        .reduce_transparency
        .store(access.reduce_transparency, Ordering::Relaxed);
    set_text_scale(access.text_scale);
}

/// The factor the type ramp is multiplied by now.
pub fn text_scale() -> f32 {
    f32::from_bits(STATE.text_scale.load(Ordering::Relaxed))
}

/// Sets the text scale alone (the taxonomy's switcher, a test).
pub fn set_text_scale(scale: f32) {
    STATE
        .text_scale
        .store(clamp_scale(scale).to_bits(), Ordering::Relaxed);
}

/// How much a size that sits in a line of text (a row's height, a control's,
/// an icon beside a label) grows with the text: by the text's growth, but
/// never shrinking below its designed size, so 200 % text never makes a
/// control smaller than 100 % does.
pub fn grow(designed: f32) -> f32 {
    designed * text_scale().max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn the_environment_sets_each_preference() {
        let access = Access::from_vars(vars(&[
            ("BINGUX_HIGH_CONTRAST", "1"),
            ("BINGUX_REDUCE_TRANSPARENCY", "1"),
            ("BINGUX_TEXT_SCALE", "1.5"),
        ]));
        assert!(access.high_contrast);
        assert!(access.reduce_transparency);
        assert!(!access.reduced_motion);
        assert_eq!(access.text_percent(), 150);
    }

    #[test]
    fn nothing_set_is_the_designed_interface() {
        assert_eq!(Access::from_vars(|_| None), Access::default());
    }

    #[test]
    fn a_scale_from_outside_is_held_to_the_range() {
        let huge = Access::from_vars(vars(&[("BINGUX_TEXT_SCALE", "40")]));
        assert_eq!(huge.text_scale, TEXT_SCALE_RANGE.1);
        let junk = Access::from_vars(vars(&[("BINGUX_TEXT_SCALE", "big")]));
        assert_eq!(junk.text_scale, 1.0);
        assert_eq!(clamp_scale(f32::NAN), 1.0);
    }
}
