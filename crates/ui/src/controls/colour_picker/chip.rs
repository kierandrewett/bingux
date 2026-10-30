//! The colour chip, and the picker's old-and-new preview.

use freya::prelude::*;

use super::core::Link;
use super::geometry::Geometry;
use super::model::Hsva;
use crate::component::ComponentName;
use crate::controls::focus_stop::{StopRing, focus_stop_with};
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;

/// A colour as a round chip of `size` pixels, with a checkerboard behind it,
/// so a translucent colour shows its alpha against a visible pattern. Every
/// swatch of a colour that can be transparent uses this.
pub fn colour_chip(tokens: Tokens, colour: Color, size: f32) -> impl IntoElement {
    let cell = (size / 2.0).max(1.0);
    let chip = painted(
        rect()
            .component("ColourChip")
            .width(Size::px(size))
            .height(Size::px(size))
            .tw(&tokens, &[rounded(Radius::Pill), clip()]),
        checker(Paint::Knob.color(&tokens), Paint::Control.color(&tokens), cell),
    );
    chip.child(
        rect()
            .position(Position::new_absolute().left(0.0).top(0.0))
            .layer(Layer::Relative(1))
            .width(Size::px(size))
            .height(Size::px(size))
            .tw(&tokens, &[fill(colour)]),
    )
    .child(
        rect()
            .position(Position::new_absolute().left(0.0).top(0.0))
            .layer(Layer::Relative(2))
            .width(Size::px(size))
            .height(Size::px(size))
            .tw(
                &tokens,
                &[rounded(Radius::Pill), border(Line::Ring(Paint::Separator))],
            ),
    )
}

/// What the preview says about the two colours, for assistive technology.
pub(super) fn preview_label(old: Color, new: Color) -> String {
    format!(
        "Colour, was {}, now {}",
        colour_to_hex(old),
        colour_to_hex(new)
    )
}

/// The old colour beside the new one, in one rounded box over a checkerboard.
/// A press on the old half puts it back.
#[derive(PartialEq)]
pub(super) struct Preview {
    pub old: Color,
    pub new: Color,
    pub link: Link,
    pub geometry: Geometry,
}

impl Component for Preview {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let g = self.geometry;
        let (old, new) = (self.old, self.new);
        let link = self.link.clone();
        let changed = Hsva::from_colour(old).colour() != Hsva::from_colour(new).colour();
        let restore = EventHandler::new(move |_: ()| {
            link.set_colour(old);
            link.settle();
        });
        let half = |colour: Color| {
            rect()
                .tw(&tokens, &[w_flex(), h_full(), fill(colour)])
        };
        let old_half = focus_stop_with(
            use_a11y(),
            half(old).component("ColourOld"),
            format!("Old colour {}. Press to restore it.", colour_to_hex(old)),
            AccessibilityRole::Button,
            changed,
            Radius::Control,
            restore,
            Vec::new(),
            StopRing::Inside,
        );
        let card = painted(
            rect()
                .component("ColourPreview")
                .a11y_role(AccessibilityRole::Group)
                .a11y_alt(preview_label(old, new))
                .tw(
                    &tokens,
                    &[w(Dim::Box24), h(Dim::Control), flex_row(), rounded(Radius::Control), clip()],
                ),
            checker(
                Paint::Knob.color(&tokens),
                Paint::Control.color(&tokens),
                g.control / 4.0,
            ),
        )
        .child(old_half)
        .child(half(new).component("ColourNew"))
        .child(
            rect()
                .position(Position::new_absolute().left(0.0).top(0.0))
                .layer(Layer::Relative(2))
                .interactive(false)
                .tw(
                    &tokens,
                    &[w(Dim::Box24), h(Dim::Control), rounded(Radius::Control), border(Line::Hairline)],
                ),
        );
        card
    }
}
