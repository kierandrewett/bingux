//! The Layout tab: the node's box as nested bands, margin around padding around
//! content, with each side labelled by the space token it is. Under it the
//! spacing rows edit the same values as the Properties pane.

use freya::prelude::*;
use freya::winit::window::WindowId;

use super::editor;
use super::props::def_row;
use super::snapshot::Node;
use crate::group::Group;
use crate::style::catalogue;
use crate::style::*;
use crate::theme::Tokens;
use crate::typography::{CAPTION, CODE, Ink};

/// The Layout tab for one node.
pub fn view(tokens: &Tokens, window: WindowId, id: u64, node: &Node, class: SizeClass) -> Element {
    let Some(props) = node.props.as_ref() else {
        return rect()
            .tw(tokens, &[w_full(), p(S4)])
            .child(CAPTION.label(tokens, "This element has no box"))
            .into();
    };
    let size = match node.area {
        Some(area) => format!("{:.0} × {:.0}", area.width(), area.height()),
        None => "no layout".to_string(),
    };
    let content: Element = rect()
        .tw(
            tokens,
            &[
                w_full(),
                py(S3),
                flex_col(),
                items(Align::Center),
                bg(Paint::InspectContent),
                rounded(Radius::Mark),
            ],
        )
        .child(CODE.label(tokens, size))
        .into();
    let padding = band(
        tokens,
        "padding",
        &props.padding,
        class,
        Paint::InspectPadding,
        content,
    );
    let margin = band(
        tokens,
        "margin",
        &props.margin,
        class,
        Paint::InspectMargin,
        padding,
    );
    let spacing: Vec<Element> = editor::defs(node, class, tokens)
        .into_iter()
        .filter(|def| matches!(def.key, "p" | "px" | "py" | "m" | "mx" | "my"))
        .map(|def| def_row(tokens, window, id, def, class))
        .collect();
    let column = rect()
        .tw(tokens, &[w_full(), flex_col(), gap(S4), p(S4)])
        .child(margin)
        .child(Group::titled("Spacing").rows(spacing));
    ScrollView::new()
        .drag_scrolling(false)
        .width(Size::fill())
        .height(Size::fill())
        .child(column)
        .into()
}

/// A side's value as the token it is, or its pixels.
fn side(px: f32, class: SizeClass) -> String {
    match catalogue::space_name(px, class) {
        Some(name) => name.to_string(),
        None => editor::number_text(px),
    }
}

/// One band of the box: its name, a value at each side, and what lies inside.
fn band(
    tokens: &Tokens,
    name: &str,
    sides: &[f32; 4],
    class: SizeClass,
    paint: Paint,
    inner: Element,
) -> Element {
    let value = |px: f32| {
        CODE.in_ink(Ink::Text)
            .label(tokens, side(px, class))
            .max_lines(1)
    };
    let [top, right, bottom, left] = *sides;
    let centred = |text: Label| {
        rect()
            .tw(tokens, &[w_full(), flex_row(), justify(Align::Center)])
            .child(text)
    };
    rect()
        .tw(
            tokens,
            &[
                w_full(),
                flex_col(),
                gap(S1),
                p(S2),
                bg(paint),
                rounded(Radius::Inner),
            ],
        )
        .child(CAPTION.in_ink(Ink::Muted).label(tokens, name.to_string()))
        .child(centred(value(top)))
        .child(
            rect()
                .tw(
                    tokens,
                    &[
                        w_full(),
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        gap(S2),
                    ],
                )
                .child(rect().tw(tokens, &[w(Dim::Box7)]).child(value(left)))
                .child(rect().tw(tokens, &[w_flex()]).child(inner))
                .child(rect().tw(tokens, &[w(Dim::Box7)]).child(value(right))),
        )
        .child(centred(value(bottom)))
        .into()
}
