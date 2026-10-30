//! One icon turning into another in the same place: the Reload arrows that
//! become the Stop square while a folder loads, and back.
//!
//! [`icon_morph`] draws two glyphs stacked in one slot of a fixed size and
//! sets each from a single `progress` (0 shows `from`, 1 shows `to`):
//!
//! - `from` turns a quarter turn clockwise while it shrinks to
//!   [`MORPH_SMALLEST`] of its size and fades out;
//! - `to` starts a quarter turn the other way, as small, and turns to rest
//!   while it grows to full size and fades in. The two turns read as one
//!   continuous clockwise turn, and the opacities add up to 1, so the slot
//!   is never empty;
//! - all of it follows one smoothstep of the progress, so the motion eases in
//!   and out inside the eased progress the caller passes.
//!
//! The slot is `size` square whatever the progress, so the button around it
//! never changes size. Both glyphs stay mounted (the hidden one at opacity 0)
//! so neither is rasterised for the first time in the middle of the motion.
//!
//! The layers are plain functions of `progress` ([`morph_layers`]), so the
//! motion is continuous exactly when the progress is. [`IconButton::morph`]
//! drives it from [`use_tween`], which starts from the value on screen when
//! the target flips: turning around half way continues from where the icon
//! is and never snaps. Reduced motion makes the tween instant, which is the
//! swap with no motion.
//!
//! [`IconButton::morph`]: crate::controls::IconButton::morph

use freya::prelude::*;

use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::style::IconSize;
use crate::surface::use_tokens;

use super::{MOVE_MS, use_tween};

/// The quarter turn each glyph makes, in degrees.
pub const MORPH_TURN: f32 = 90.0;

/// How small a glyph is at the point it is fully faded: a fraction of its
/// size.
pub const MORPH_SMALLEST: f32 = 0.5;

/// How one glyph of a morph is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphLayer {
    pub opacity: f32,
    /// Relative to the full size.
    pub scale: f32,
    /// Clockwise, in degrees.
    pub degrees: f32,
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The `from` and the `to` layer at `progress`, 0 to 1.
pub fn morph_layers(progress: f32) -> (MorphLayer, MorphLayer) {
    let s = smoothstep(progress);
    (
        MorphLayer {
            opacity: 1.0 - s,
            scale: 1.0 - (1.0 - MORPH_SMALLEST) * s,
            degrees: MORPH_TURN * s,
        },
        MorphLayer {
            opacity: s,
            scale: MORPH_SMALLEST + (1.0 - MORPH_SMALLEST) * s,
            degrees: -MORPH_TURN * (1.0 - s),
        },
    )
}

fn layer(glyph: Icon, size: IconSize, color: Color, look: MorphLayer) -> Rect {
    let edge = size.px();
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::px(edge))
        .height(Size::px(edge))
        .opacity(look.opacity)
        .scale(look.scale)
        .rotate(look.degrees)
        .child(icons::icon(glyph, size, color))
}

/// `from` and `to` stacked in one `size` square, at `progress` (0 shows
/// `from`, 1 shows `to`). `progress` is already eased; this only maps it to
/// the two glyphs. See the [module](self).
pub fn icon_morph(from: Icon, to: Icon, size: IconSize, color: Color, progress: f32) -> Rect {
    let edge = size.px();
    let (leaving, entering) = morph_layers(progress);
    rect()
        .component("IconMorph")
        .width(Size::px(edge))
        .height(Size::px(edge))
        .child(layer(from, size, color, leaving))
        .child(layer(to, size, color, entering))
}

/// [`icon_morph`] on the shared tween: `on` false shows `from`, true shows
/// `to`, and a flip eases over the motion token's duration from wherever the
/// icon is.
#[derive(Clone, PartialEq)]
pub(crate) struct MorphGlyph {
    pub from: Icon,
    pub to: Icon,
    pub size: IconSize,
    pub color: Color,
    pub on: bool,
}

impl Component for MorphGlyph {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let progress = use_tween(if self.on { 1.0 } else { 0.0 }, tokens.motion(MOVE_MS));
        icon_morph(self.from, self.to, self.size, self.color, progress)
    }
}

#[cfg(all(test, feature = "testing"))]
#[path = "icon_morph_tests.rs"]
mod frames;

#[cfg(test)]
mod tests {
    use super::*;

    fn sweep(steps: usize) -> Vec<(MorphLayer, MorphLayer)> {
        (0..=steps).map(|n| morph_layers(n as f32 / steps as f32)).collect()
    }

    #[test]
    fn the_ends_show_one_glyph_at_rest() {
        let (from, to) = morph_layers(0.0);
        assert_eq!(from, MorphLayer { opacity: 1.0, scale: 1.0, degrees: 0.0 });
        assert_eq!(to.opacity, 0.0);
        let (from, to) = morph_layers(1.0);
        assert_eq!(from.opacity, 0.0);
        assert_eq!(to, MorphLayer { opacity: 1.0, scale: 1.0, degrees: 0.0 });
    }

    #[test]
    fn out_of_range_progress_is_the_end() {
        assert_eq!(morph_layers(-3.0), morph_layers(0.0));
        assert_eq!(morph_layers(7.0), morph_layers(1.0));
    }

    #[test]
    fn the_leaving_glyph_turns_a_quarter_while_it_shrinks_and_fades() {
        let layers = sweep(100);
        for pair in layers.windows(2) {
            let (a, b) = (pair[0].0, pair[1].0);
            assert!(b.opacity <= a.opacity && b.scale <= a.scale && b.degrees >= a.degrees, "{a:?} -> {b:?}");
        }
        let (gone, _) = morph_layers(1.0);
        assert_eq!(gone.opacity, 0.0);
        assert_eq!(gone.scale, MORPH_SMALLEST);
        assert_eq!(gone.degrees, MORPH_TURN);
    }

    #[test]
    fn the_entering_glyph_grows_and_turns_to_rest() {
        let layers = sweep(100);
        for pair in layers.windows(2) {
            let (a, b) = (pair[0].1, pair[1].1);
            assert!(b.opacity >= a.opacity && b.scale >= a.scale && b.degrees >= a.degrees, "{a:?} -> {b:?}");
        }
        let (_, start) = morph_layers(0.0);
        assert_eq!(start.opacity, 0.0);
        assert_eq!(start.scale, MORPH_SMALLEST);
        assert_eq!(start.degrees, -MORPH_TURN);
    }

    #[test]
    fn the_opacities_add_up_to_one_so_the_slot_is_never_empty() {
        for (from, to) in sweep(200) {
            assert!((from.opacity + to.opacity - 1.0).abs() < 1e-6, "{from:?} {to:?}");
        }
    }

    #[test]
    fn it_is_continuous_and_a_turn_around_continues_from_where_it_is() {
        // The progress a tween gives when it is sent towards 1 and, half way,
        // back to 0: a value that moves a little each frame and reverses
        // without a jump. The layers must move a little each frame too,
        // through the reversal.
        let mut progress = 0.0_f32;
        let mut towards = 1.0_f32;
        let mut seen = Vec::new();
        for frame in 0..40 {
            if frame == 6 {
                towards = 0.0;
            }
            progress += (towards - progress) * 0.25;
            seen.push(morph_layers(progress));
        }
        let mut worst = 0.0_f32;
        for pair in seen.windows(2) {
            worst = worst
                .max((pair[1].0.opacity - pair[0].0.opacity).abs())
                .max((pair[1].1.opacity - pair[0].1.opacity).abs())
                .max((pair[1].0.scale - pair[0].0.scale).abs())
                .max((pair[1].0.degrees - pair[0].0.degrees).abs() / MORPH_TURN)
                .max((pair[1].1.degrees - pair[0].1.degrees).abs() / MORPH_TURN);
        }
        assert!(worst < 0.35, "no frame jumps: {worst}");
        // Back at rest on the reload glyph.
        let (from, to) = *seen.last().unwrap();
        assert!(from.opacity > 0.99 && to.opacity < 0.01, "{from:?} {to:?}");
    }
}
