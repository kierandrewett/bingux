//! Number scrubbing. A numeric label scrubs its number: dragging it
//! horizontally steps the value, and the wheel steps it one notch at a time.
//! Shift multiplies a step by ten and Alt by a tenth. The steps are plain
//! deltas; the owner turns each one into text with [`step`].

use freya::prelude::*;

use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, TextStyle, styled_label};

/// Logical pixels of drag that make one step.
const PIXELS_PER_STEP: f32 = 4.0;

/// The text a numeric field moves to after a step of `delta`, or `None` when
/// the text is not a number. A trailing `px` is accepted and dropped. Every
/// value in a multi-value field such as `4 8 4 8` steps by the same delta.
pub fn step(text: &str, delta: f64) -> Option<String> {
    let values = text
        .split_whitespace()
        .map(|token| token.trim_end_matches("px").parse::<f32>().ok())
        .collect::<Option<Vec<f32>>>()?;
    if values.is_empty() {
        return None;
    }
    let stepped = values
        .iter()
        .map(|value| format!("{}", (value + delta as f32).max(0.0)))
        .collect::<Vec<_>>();
    Some(stepped.join(" "))
}

/// The multiplier a step gets from the modifier keys held: Shift is ten,
/// Alt a tenth, and anything else one.
fn scale(modifiers: Modifiers) -> f64 {
    if modifiers.contains(Modifiers::SHIFT) {
        10.0
    } else if modifiers.contains(Modifiers::ALT) {
        0.1
    } else {
        1.0
    }
}

/// A label that scrubs a number. `on_step` receives the signed,
/// modifier-scaled delta. When `enabled` is false the label is plain text.
/// The label is caption type in the surface's ink unless [`Scrub::style`]
/// and [`Scrub::paint`] say otherwise.
#[derive(PartialEq)]
pub struct Scrub {
    label: String,
    enabled: bool,
    fit: bool,
    style: TextStyle,
    paint: Option<Paint>,
    on_step: EventHandler<f64>,
}

/// A scrubbing label for a number's name.
pub fn scrub(label: impl Into<String>, on_step: impl Into<EventHandler<f64>>) -> Scrub {
    Scrub {
        label: label.into(),
        enabled: true,
        fit: false,
        style: CAPTION,
        paint: None,
        on_step: on_step.into(),
    }
}

impl Scrub {
    /// Sizes the label to its text rather than the full row. A code line uses
    /// this so the value after the name keeps its room.
    pub fn fit(mut self) -> Self {
        self.fit = true;
        self
    }

    /// Whether the label scrubs at all. A value that is not a plain number
    /// leaves it a plain label.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The text style of the label, such as [`crate::typography::CODE`].
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// The surface paint of the label, such as a syntax colour, in place of
    /// the style's ink.
    pub fn paint(mut self, paint: Paint) -> Self {
        self.paint = Some(paint);
        self
    }
}

impl Component for Scrub {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let label: Element = match self.paint {
            Some(paint) => self
                .style
                .label_in_paint(&tokens, self.label.clone(), paint)
                .into_element(),
            None => styled_label(self.style, self.label.clone()),
        };
        let width = if self.fit { Size::auto() } else { Size::fill() };
        if !self.enabled {
            return rect()
                .component("Scrub")
                .tw(&tokens, &[flex_row()])
                .width(width)
                .child(label)
                .into_element();
        }

        // The last pointer x of an active drag, `None` when no drag is on.
        let anchor = use_state(|| None::<f32>);
        // Shift or Alt as the last key event left them. Pointer and wheel
        // events carry no modifiers of their own.
        let modifiers = use_state(Modifiers::default);
        let on_step_move = self.on_step.clone();
        let on_step_wheel = self.on_step.clone();

        rect()
            .component("Scrub")
            .tw(&tokens, &[flex_row()])
            .width(width)
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let mut held = modifiers;
                held.set(event.modifiers);
            })
            .on_global_key_up(move |event: Event<KeyboardEventData>| {
                let mut held = modifiers;
                held.set(event.modifiers);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let mut origin = anchor;
                origin.set(Some(at.x as f32));
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                let Some(last) = *anchor.read() else {
                    return;
                };
                let x = event.global_location().x as f32;
                let steps = ((x - last) / PIXELS_PER_STEP).trunc();
                if steps == 0.0 {
                    return;
                }
                let mut origin = anchor;
                origin.set(Some(last + steps * PIXELS_PER_STEP));
                on_step_move.call(steps as f64 * scale(*modifiers.read()));
            })
            .on_global_pointer_up(move |_: Event<PointerEventData>| {
                let mut origin = anchor;
                origin.set(None);
            })
            .on_wheel(move |event: Event<WheelEventData>| {
                let notch = if event.delta_y < 0.0 {
                    1.0
                } else if event.delta_y > 0.0 {
                    -1.0
                } else {
                    return;
                };
                event.stop_propagation();
                on_step_wheel.call(notch * scale(*modifiers.read()));
            })
            .child(label)
            .into_element()
    }
}
