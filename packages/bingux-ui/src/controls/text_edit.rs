//! Touch text editing: the edit bar over a text field's caret or selection,
//! and the two lollipop handles that drag a selection's ends.
//!
//! Touch only. A mouse keeps the floating right-click menu. The bar holds the
//! general edit actions, and its chevron opens the full menu as a floating
//! panel against the same caret. The bar is a `Panel` of kind `Bar`
//! (`shell::panel`), which places it; this module supplies its entries and the
//! handles. Everything is in window coordinates from the probe the vendored
//! `Input` reports. Handles are drawn in the field's frame.

use std::time::Duration;

use freya::prelude::*;
use freya::torin::prelude::{Point2D, Size2D};

use crate::icons::{self, Icon};
use crate::shell::menu::MenuItem;
use crate::shell::panel::PanelRect;
use crate::style::*;
use crate::theme::Tokens;
use crate::typography::BODY;

/// Two taps this close together count as a double tap, which selects a word.
pub(crate) const DOUBLE_TAP: Duration = Duration::from_millis(350);
/// Diameter of a handle's knob.
const KNOB: f32 = 12.0;
/// Width of a handle's stem, the line through the caret.
const STEM: f32 = 2.0;
/// Width of a handle's touch area, centred on its caret.
const HANDLE_HIT: f32 = 32.0;

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

/// The window rectangle a handle at `caret` covers, its touch area.
pub(crate) fn handle_rect(caret: InputCaret) -> PanelRect {
    let height = (caret.bottom - caret.top).max(0.0);
    PanelRect {
        x: caret.x - HANDLE_HIT / 2.0,
        y: caret.top,
        width: HANDLE_HIT,
        height: height + KNOB + HANDLE_HIT / 2.0,
    }
}

/// A window rectangle as the layout's area, for the taps an input must not
/// count as outside itself.
pub(crate) fn area(rect: PanelRect) -> Area {
    Area::new(
        Point2D::new(rect.x, rect.y),
        Size2D::new(rect.width, rect.height),
    )
}

/// A lollipop handle at one selection end: a stem through the caret with a
/// knob below it. Dragging it moves that end of the selection live. The caret
/// is in window coordinates, and the handle is placed there. Its touch area is
/// wider than the knob so a finger can find it.
pub(crate) fn handle(
    tokens: &Tokens,
    caret: InputCaret,
    start: bool,
    mut request: State<Option<InputRequest>>,
) -> impl IntoElement {
    let height = (caret.bottom - caret.top).max(0.0);
    let hit = handle_rect(caret);
    rect()
        .layer(Layer::Overlay)
        .position(Position::new_global().left(hit.x).top(hit.y))
        .width(Size::px(hit.width))
        .height(Size::px(hit.height))
        .on_pointer_down(move |event: Event<PointerEventData>| {
            event.stop_propagation();
            request.set(Some(InputRequest::DragSelectionEnd { start }));
        })
        .child(
            rect()
                .position(
                    Position::new_absolute()
                        .left((HANDLE_HIT - STEM) / 2.0)
                        .top(0.0),
                )
                .width(Size::px(STEM))
                .height(Size::px(height))
                .tw(tokens, &[bg(Paint::Accent)]),
        )
        .child(
            rect()
                .position(
                    Position::new_absolute()
                        .left((HANDLE_HIT - KNOB) / 2.0)
                        .top(height),
                )
                .width(Size::px(KNOB))
                .height(Size::px(KNOB))
                .tw(tokens, &[rounded(Radius::Pill), bg(Paint::Accent)]),
        )
}

/// The edit bar's entries, in order: each label, a hairline between two
/// entries, then a hairline and the chevron that calls `on_more`. The bar's
/// [`Panel`](crate::shell::panel::Panel) draws the pill around them.
pub(crate) fn bar(
    tokens: &Tokens,
    entries: Vec<MenuItem>,
    on_more: EventHandler<()>,
) -> Vec<Element> {
    let mut children = Vec::new();
    for entry in entries {
        if !children.is_empty() {
            children.push(separator(tokens).into_element());
        }
        children.push(bar_entry(tokens, entry).into_element());
    }
    children.push(separator(tokens).into_element());
    children.push(
        rect()
            .tw(tokens, &[flex_row(), center(), px(S2), h(Dim::Control)])
            .on_press(move |_| on_more.call(()))
            .child(icons::icon(
                Icon::GoNext,
                IconSize::Control,
                Paint::Muted.color(tokens),
            ))
            .into_element(),
    );
    children
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
