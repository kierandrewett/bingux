//! A line of text that does not fit fades out at its end instead of ending in
//! an ellipsis.
//!
//! The end is the end of the *reading direction*: the right edge in a
//! left-to-right interface, the left edge in a right-to-left one
//! ([`Direction`]). The last [`Dim::TextFade`] (24 px) of the room are the
//! fade: the glyphs' own alpha goes from opaque to nothing across it, so what
//! is behind the text (a hovered tab's fill, the selected tab's surface, the
//! strip) shows through and the fade is right on any ground. It is an alpha
//! ramp in the text's paint, not a coloured overlay: nothing is drawn over
//! the text, so the neighbour at the end of the line (a close button) is
//! never touched. Text that fits is drawn plain.
//!
//! ```ignore
//! // In a row: takes what room the siblings leave, fades where it ends.
//! fade_text("A name far too long for its tab").style(CAPTION).grow()
//! ```
//!
//! Use it where a *title* is cut beside a control (a tab, a chip) and the
//! full text is shown another way (a tooltip, the tab list). Where the end of
//! a name carries meaning (a file's extension) use
//! [`path_label`](super::path_label), which cuts the middle.
//!
//! The box is as wide as its parent gives it (or takes the room the other
//! children of a row leave, with [`FadeText::grow`]) and one line tall. It
//! measures itself once it is on screen, so its first frame is the whole
//! text, clipped. The label inside is a plain label, so the text is in the
//! accessibility tree and found by a test as any label is.

use freya::prelude::*;

use super::direction::{Direction, use_direction};
use super::path_label::text_width;
use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, TextStyle};

/// A line of `text` that fades out where it ends instead of being cut with an
/// ellipsis. The default style is [`CAPTION`]; see [`FadeText`].
pub fn fade_text(text: impl Into<String>) -> FadeText {
    FadeText {
        text: text.into(),
        style: CAPTION,
        grow: false,
        fade: Dim::TextFade,
    }
}

#[derive(Clone, PartialEq)]
pub struct FadeText {
    text: String,
    style: TextStyle,
    grow: bool,
    fade: Dim,
}

impl FadeText {
    /// The text style, which carries the ink. The default is [`CAPTION`].
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Takes the room the other children of its row leave (`flex: 1`)
    /// instead of the width of its parent.
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// How long the fade is. The default is [`Dim::TextFade`].
    pub fn length(mut self, length: Dim) -> Self {
        self.fade = length;
        self
    }
}

/// The alpha, `0.0` to `1.0`, of text at `x` px from its start in the
/// reading direction (`x` = 0 is where the line begins), for a line `natural`
/// px wide shown in `room` px with a `fade` px ramp at the end of the room:
/// what the paint gives every glyph. A line that fits is opaque everywhere.
pub fn fade_alpha(x: f32, natural: f32, room: f32, fade: f32) -> f32 {
    if natural <= room + 0.5 {
        return 1.0;
    }
    let from = (room - fade).max(0.0);
    if x <= from {
        1.0
    } else if x >= room {
        0.0
    } else {
        1.0 - (x - from) / (room - from)
    }
}

/// The text's paint: its ink, opaque, with the alpha ramp over the end of the
/// room, positions in percent of the line's own width as a gradient has them.
/// Left to right the line begins at its left edge and is cut on the right;
/// right to left it ends at the room's right edge and is cut on the left.
fn fade_paint(ink: Color, natural: f32, room: f32, fade: f32, direction: Direction) -> LinearGradient {
    let percent = |at: f32| (at / natural * 100.0).clamp(0.0, 100.0);
    let clear = ink.with_a(0);
    let ramp = fade.min(room);
    // In this engine's gradient 90 degrees runs from the right edge to the left
    // and 270 from the left to the right, which is the way the stops below run.
    let gradient = LinearGradient::new().angle(270.0);
    if direction.is_rtl() {
        // The visible part is the last `room` px of the line, which ends at the
        // room's right edge; the line is cut on the left.
        let hidden = natural - room;
        gradient
            .stop(GradientStop::new(clear, 0.0))
            .stop(GradientStop::new(clear, percent(hidden)))
            .stop(GradientStop::new(ink, percent(hidden + ramp)))
            .stop(GradientStop::new(ink, 100.0))
    } else {
        gradient
            .stop(GradientStop::new(ink, 0.0))
            .stop(GradientStop::new(ink, percent(room - ramp)))
            .stop(GradientStop::new(clear, percent(room)))
            .stop(GradientStop::new(clear, 100.0))
    }
}

impl Component for FadeText {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let direction = use_direction();
        let mut room = use_state(|| None::<f32>);
        let natural = text_width(&self.text, self.style);
        let fade = self.fade.px();
        let shown = room();
        let cut = shown.filter(|room| natural > room + 0.5);
        let ink = self.style.ink.color(&tokens);
        let label = match cut {
            Some(room) => self.style.label_filled(
                &tokens,
                self.text.clone(),
                fade_paint(ink, natural, room, fade, direction),
            ),
            None => self.style.label(&tokens, self.text.clone()),
        }
        .max_lines(1);
        rect()
            .component("FadeText")
            .tw(&tokens, &[if self.grow { w_flex() } else { w_full() }, clip()])
            .on_sized(move |event: Event<SizedEventData>| {
                room.set_if_modified(Some(event.area.width()));
            })
            // Right to left the engine puts the line's end at the room's right
            // edge, so a line that is too long runs out on the left, where it is cut.
            .child(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_that_fits_is_opaque_and_one_that_does_not_ramps_down_over_its_last_stretch() {
        assert_eq!(fade_alpha(40.0, 80.0, 100.0, 24.0), 1.0, "it fits");
        assert_eq!(fade_alpha(99.0, 100.0, 100.0, 24.0), 1.0, "it fits exactly");
        // 200 px of text in 100 px: opaque to 76, gone at 100.
        assert_eq!(fade_alpha(0.0, 200.0, 100.0, 24.0), 1.0);
        assert_eq!(fade_alpha(76.0, 200.0, 100.0, 24.0), 1.0);
        assert_eq!(fade_alpha(88.0, 200.0, 100.0, 24.0), 0.5);
        assert_eq!(fade_alpha(100.0, 200.0, 100.0, 24.0), 0.0);
        assert_eq!(fade_alpha(150.0, 200.0, 100.0, 24.0), 0.0, "what is past the room is not seen");
        let mut last = 1.0;
        for x in 0..=100 {
            let alpha = fade_alpha(x as f32, 200.0, 100.0, 24.0);
            assert!(alpha <= last, "the ramp only goes down: {alpha} after {last} at {x}");
            last = alpha;
        }
    }

    #[test]
    fn a_room_narrower_than_the_fade_ramps_over_the_whole_room() {
        assert_eq!(fade_alpha(0.0, 100.0, 10.0, 24.0), 1.0);
        assert_eq!(fade_alpha(5.0, 100.0, 10.0, 24.0), 0.5);
        assert_eq!(fade_alpha(10.0, 100.0, 10.0, 24.0), 0.0);
    }
}
