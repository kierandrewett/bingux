//! Readout: one large line of figures that always fits.
//!
//! A calculator's result, a counter, a stopwatch: the text is as large as
//! the box allows. When it is too wide it is scaled down, and when it would
//! have to be smaller than `min_scale` it keeps that size and scrolls
//! sideways instead, so a result of two hundred digits stays readable and
//! none of it is cut. The text is selectable, and a right click copies it.
//! It is a live region: a screen reader announces the new value, politely.

use accesskit::Live;
use freya::prelude::*;

use super::path_label::measure_text_scaled;
use crate::component::ComponentName;
use crate::controls::select_text::copy_text;
use crate::shell::menu::menu_item;
use crate::shell::overlays::Overlays;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{HERO, TextStyle};

/// What a scrolling readout adds to the measured width of its text, so the
/// last digit is never wrapped by a rounding error.
const SCROLL_SLACK: f32 = 4.0;
/// A [`Readout`] of `text`.
pub fn readout(text: impl Into<String>) -> Readout {
    Readout {
        text: text.into(),
        spoken: None,
        copy: None,
        style: HERO,
        scale: 1.0,
        min_scale: 0.45,
        align: TextAlign::End,
        live: true,
    }
}

#[derive(Clone, PartialEq)]
pub struct Readout {
    text: String,
    spoken: Option<String>,
    copy: Option<String>,
    style: TextStyle,
    scale: f32,
    min_scale: f32,
    align: TextAlign,
    live: bool,
}

impl Readout {
    /// The type style the text starts at (the hero style by default).
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// The size the text has when it fits, as a multiple of the style's size.
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale.max(0.1);
        self
    }

    /// The smallest multiple of the full size the text shrinks to before it
    /// scrolls instead (0.45 by default).
    pub fn min_scale(mut self, min_scale: f32) -> Self {
        self.min_scale = min_scale.clamp(0.1, 1.0);
        self
    }

    /// Where the text sits when it does not fill the line: `TextAlign::End`
    /// by default, as a calculator's does.
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// What a screen reader says for the text ("one thousand two hundred").
    pub fn spoken(mut self, spoken: impl Into<String>) -> Self {
        self.spoken = Some(spoken.into());
        self
    }

    /// What the context menu's Copy puts on the clipboard, when it should
    /// differ from what is shown (a number without its group marks).
    pub fn copy_text(mut self, text: impl Into<String>) -> Self {
        self.copy = Some(text.into());
        self
    }

    /// Do not announce changes: for a value that changes every second.
    pub fn silent(mut self) -> Self {
        self.live = false;
        self
    }

    /// The font scale the text is drawn at in `available` px, and whether it
    /// scrolls. Pure, so a test can hold it to its promise.
    pub(crate) fn fit(
        natural_at_full: f32,
        available: f32,
        scale: f32,
        min_scale: f32,
    ) -> (f32, bool) {
        if available <= 0.0 || natural_at_full * scale <= available {
            return (scale, false);
        }
        let fitted = available / natural_at_full;
        if fitted >= min_scale * scale {
            (fitted.min(scale), false)
        } else {
            (min_scale * scale, true)
        }
    }
}

impl Component for Readout {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = try_consume_context::<Overlays>();
        let mut available = use_state(|| 0.0_f32);
        let natural = measure_text_scaled(&self.text, self.style, 1.0);
        let (scale, scrolls) = Self::fit(natural, available(), self.scale, self.min_scale);
        // Quarter pixels, as `label_scaled` rounds, so the size does not
        // wander with every pixel of the window.
        let size = (self.style.size.px() * scale * 4.0).round() / 4.0;
        let line = self.style.leading * self.style.size.px() * self.scale;
        let shown = self.text.clone();
        let spoken = self.spoken.clone().unwrap_or_else(|| self.text.clone());
        let copy = self.copy.clone().unwrap_or_else(|| self.text.clone());
        let live = self.live;
        let at_end = self.align == TextAlign::End;

        let scroll_width = measure_text_scaled(&shown, self.style, scale) + SCROLL_SLACK;
        let text = SelectableText::new()
            // ui-lint: allow(the size is the style's, scaled to the measured width of the text)
            .span(self.style.span(&tokens, shown.clone()).font_size(size))
            .line_height(self.style.leading)
            .text_align(self.align)
            .highlight_color(Paint::Selection.color(&tokens))
            .a11y_focusable(false)
            // One line, however long: the width is the text's own when it scrolls.
            .maybe(scrolls, |text| text.width(Size::px(scroll_width)))
            .maybe(!scrolls, |text| text.width(Size::fill()));

        let body: Element = if scrolls {
            ScrollView::new()
                // ui-lint: allow(the scroll axis of a ScrollView, not a box style)
                .direction(Direction::Horizontal)
                .show_scrollbar(true)
                .child(text)
                .into()
        } else {
            text.into()
        };

        rect()
            .component("Readout")
            .width(Size::fill())
            .min_height(Size::px(line))
            .tw(&tokens, &[flex_col(), justify(Align::Center)])
            .on_sized(move |event: Event<SizedEventData>| {
                available.set_if_modified(event.area.width());
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let right = matches!(
                    event.data(),
                    PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
                );
                if let (true, Some(overlays)) = (right, overlays) {
                    event.stop_propagation();
                    let at = event.global_location();
                    let copy = copy.clone();
                    overlays.show_menu(
                        (at.x as f32, at.y as f32),
                        vec![menu_item("Copy", move |_| {
                            copy_text(overlays, copy.clone())
                        })],
                    );
                }
            })
            .a11y_role(if live {
                AccessibilityRole::Status
            } else {
                AccessibilityRole::Label
            })
            .a11y_alt(spoken.clone())
            .a11y_builder(move |node| {
                node.set_value(spoken.clone());
                if live {
                    node.set_live(Live::Polite);
                }
            })
            .maybe(at_end && !scrolls, |outer| {
                outer.tw(&tokens, &[items(Align::End)])
            })
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_that_fits_is_full_size() {
        assert_eq!(Readout::fit(100.0, 300.0, 1.0, 0.45), (1.0, false));
        assert_eq!(Readout::fit(300.0, 300.0, 1.0, 0.45), (1.0, false));
    }

    #[test]
    fn text_that_does_not_fit_shrinks_to_fit() {
        let (scale, scrolls) = Readout::fit(400.0, 300.0, 1.0, 0.45);
        assert!(!scrolls);
        assert!((scale - 0.75).abs() < 1e-6);
    }

    #[test]
    fn text_that_would_get_too_small_scrolls_at_the_smallest_size() {
        assert_eq!(Readout::fit(1000.0, 300.0, 1.0, 0.45), (0.45, true));
        let (scale, scrolls) = Readout::fit(1000.0, 300.0, 1.5, 0.45);
        assert!((scale - 0.675).abs() < 1e-6 && scrolls);
    }

    #[test]
    fn an_unmeasured_box_changes_nothing() {
        assert_eq!(Readout::fit(1000.0, 0.0, 1.0, 0.45), (1.0, false));
    }
}
