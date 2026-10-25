//! The Layout tab: the node's box as nested layers, margin around padding
//! around content. The padding and margin fields beneath commit like any
//! declared property.

use freya::prelude::*;
use freya::winit::window::WindowId;

use super::editor::{self, Declared, Input};
use super::snapshot::Node;
use super::styles::prop_row;
use crate::style::*;
use crate::theme::Tokens;
use crate::typography::{BODY, CAPTION, styled_label};

/// The Layout tab for one node.
pub fn view(
    tokens: &Tokens,
    window: WindowId,
    id: u64,
    node: &Node,
    picker: State<Option<&'static str>>,
) -> Element {
    let Some(props) = node.props.as_ref() else {
        return rect()
            .tw(tokens, &[w_full(), flex_col(), p(S2)])
            .child(styled_label(CAPTION, "This element has no box"))
            .into();
    };
    let size = match node.area {
        Some(area) => format!("{:.0} x {:.0}", area.width(), area.height()),
        None => "no layout".to_string(),
    };
    let padding = editor::gaps_text(&props.padding);
    let margin = editor::gaps_text(&props.margin);
    let content: Element = rect()
        .tw(tokens, &[p(S0_5)])
        .child(styled_label(BODY, size))
        .into();
    let padding_box = layer(tokens, "padding", &padding, content);
    let margin_box = layer(tokens, "margin", &margin, padding_box);
    rect()
        .tw(tokens, &[w_full(), h_full(), flex_col(), gap(S2)])
        .child(margin_box)
        .child(prop_row(
            tokens,
            window,
            id,
            Declared {
                name: "padding",
                value: padding,
                input: Input::Gaps,
            },
            picker,
        ))
        .child(prop_row(
            tokens,
            window,
            id,
            Declared {
                name: "margin",
                value: margin,
                input: Input::Gaps,
            },
            picker,
        ))
        .into()
}

/// One layer of the box: its name and values in caption type, the layer inside.
fn layer(tokens: &Tokens, name: &str, value: &str, inner: Element) -> Element {
    rect()
        .tw(
            tokens,
            &[w_full(), flex_col(), p(S0_5), border(Line::Outline)],
        )
        .child(styled_label(CAPTION, format!("{name}  {value}")))
        .child(inner)
        .into()
}
