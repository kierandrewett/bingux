//! Touch text editing: the edit bar over a text field's caret or selection,
//! and the two lollipop handles that drag a selection's ends.
//!
//! Touch only. A mouse keeps the floating right-click menu. The bar holds the
//! general edit actions, and its chevron opens the full menu anchored to the
//! bar. Everything is placed in window coordinates from the probe the
//! vendored `Input` reports. The caller draws it as a child of the field, offset
//! by the field's area origin, as the tooltip does.

use std::time::Duration;

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::shell::menu::MenuItem;
use crate::shell::panel::{PanelRect, Side, place};
use crate::style::*;
use crate::theme::Tokens;
use crate::typography::BODY;

/// Two taps this close together count as a double tap, which selects a word.
pub(crate) const DOUBLE_TAP: Duration = Duration::from_millis(350);
/// Space between the bar and the caret or selection it sits above.
const BAR_GAP: f32 = 4.0;
/// Diameter of a handle's knob.
const KNOB: f32 = 12.0;
/// Width of a handle's stem, the line through the caret.
const STEM: f32 = 2.0;

/// The window rectangle the bar is placed against: the caret, or the bounding
/// box of the selection. An empty field has no caret to show, so the bar sits
/// on the text area's leading edge. `None` only when the field is not focused.
pub(crate) fn anchor(probe: &InputProbe) -> Option<PanelRect> {
    if !probe.focused {
        return None;
    }
    let (a, b) = match probe.selection {
        Some(range) => range,
        None => {
            let caret = probe.caret.unwrap_or(InputCaret {
                x: probe.area.min_x(),
                top: probe.area.min_y(),
                bottom: probe.area.max_y(),
            });
            (caret, caret)
        }
    };
    let left = a.x.min(b.x);
    let right = a.x.max(b.x);
    let top = a.top.min(b.top);
    let bottom = a.bottom.max(b.bottom);
    Some(PanelRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// The bar's window rectangle once its size is measured. The bar opens above
/// its anchor, or below when there is no room, and stays inside the window.
pub(crate) fn bar_rect(
    anchor: PanelRect,
    size: Option<(f32, f32)>,
    window: (f32, f32),
) -> Option<PanelRect> {
    let (width, height) = size?;
    let (x, y) = place(anchor, (width, height), window, Side::Above, BAR_GAP);
    Some(PanelRect {
        x,
        y,
        width,
        height,
    })
}

/// A lollipop handle at one selection end: a stem through the caret with a
/// knob below it. Dragging it moves that end of the selection live.
/// `origin` is the field's area origin, so the handle is placed in its frame.
pub(crate) fn handle(
    tokens: &Tokens,
    caret: InputCaret,
    origin: (f32, f32),
    start: bool,
    mut request: State<Option<InputRequest>>,
) -> impl IntoElement {
    let height = (caret.bottom - caret.top).max(0.0);
    let left = caret.x - origin.0 - KNOB / 2.0;
    let top = caret.top - origin.1;
    rect()
        .layer(Layer::Overlay)
        .position(Position::new_absolute().left(left).top(top))
        .width(Size::px(KNOB))
        .height(Size::px(height + KNOB))
        .on_pointer_down(move |event: Event<PointerEventData>| {
            event.stop_propagation();
            request.set(Some(InputRequest::DragSelectionEnd { start }));
        })
        .child(
            rect()
                .position(Position::new_absolute().left((KNOB - STEM) / 2.0).top(0.0))
                .width(Size::px(STEM))
                .height(Size::px(height))
                .tw(tokens, &[bg(Paint::Accent)]),
        )
        .child(
            rect()
                .position(Position::new_absolute().left(0.0).top(height))
                .width(Size::px(KNOB))
                .height(Size::px(KNOB))
                .tw(tokens, &[rounded(Radius::Pill), bg(Paint::Accent)]),
        )
}

/// The edit bar: a compact pill of `entries`, each a label, then a chevron
/// that calls `on_more`. Entries are separated by hairlines. The bar reports
/// its size through `measure`, so the caller can place it once it is known.
/// `offset` is the bar's position in the field's frame, `None` until placed.
pub(crate) fn bar(
    tokens: &Tokens,
    entries: Vec<MenuItem>,
    on_more: EventHandler<()>,
    mut measure: State<Option<(f32, f32)>>,
    offset: Option<(f32, f32)>,
) -> impl IntoElement {
    let (left, top) = offset.unwrap_or((0.0, 0.0));
    let mut row = rect()
        .on_sized(move |event: Event<SizedEventData>| {
            measure.set_if_modified(Some((event.area.width(), event.area.height())));
        })
        .layer(Layer::Overlay)
        .position(Position::new_absolute().left(left).top(top))
        .opacity(if offset.is_some() { 1.0 } else { 0.0 })
        .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
        .tw(
            tokens,
            &[
                flex_row(),
                items(Align::Center),
                h(Dim::Control),
                rounded(Radius::Pill),
                bg(Paint::Floating),
                border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline)),
                shadow(Elevation::Raised),
            ],
        );
    for entry in entries {
        row = row.child(separator(tokens)).child(bar_entry(tokens, entry));
    }
    row.child(separator(tokens)).child(
        rect()
            .tw(tokens, &[flex_row(), center(), px(S2), h(Dim::Control)])
            .on_press(move |_| on_more.call(()))
            .child(icons::icon(
                Icon::GoNext,
                IconSize::Control,
                Paint::Muted.color(tokens),
            )),
    )
}

/// One bar entry: its label, dimmed and inert when disabled.
fn bar_entry(tokens: &Tokens, entry: MenuItem) -> impl IntoElement {
    let label = if entry.enabled {
        BODY.label(tokens, entry.label)
    } else {
        BODY.label_in_paint(tokens, entry.label, Paint::Muted)
    };
    let on_press = entry.on_press;
    let enabled = entry.enabled;
    rect()
        .tw(tokens, &[flex_row(), center(), px(S3), h(Dim::Control)])
        .on_press(move |_| {
            if enabled {
                on_press.call(());
            }
        })
        .child(label)
}

/// A hairline between two bar entries.
fn separator(tokens: &Tokens) -> impl IntoElement {
    rect().tw(
        tokens,
        &[w(Dim::Hairline), h(Dim::Box5), bg(Paint::Separator)],
    )
}
