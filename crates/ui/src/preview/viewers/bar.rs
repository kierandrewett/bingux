//! The slim row a viewer adds above its content: a line of facts on the left
//! and the viewer's tools on the right.
//!
//! A quick look ([`Depth::Glance`](super::super::mode::Depth::Glance)) shows
//! the content alone, so the tools are only passed in a dedicated window. A
//! viewer that always has a line of facts (an archive's totals) passes no
//! tools in a quick look and gets the same row.

use freya::prelude::*;

use crate::style::*;
use crate::theme::Tokens;
use crate::typography::{CAPTION, Ink};

/// The row: `facts` on the left, cut with an ellipsis when it does not fit,
/// then `tools` at the trailing end, with a hairline under it.
pub fn tool_bar(tokens: &Tokens, facts: String, tools: Vec<Element>) -> Element {
    rect()
        .tw(tokens, &[w_full(), flex_col()])
        .child(
            rect()
                .tw(
                    tokens,
                    &[
                        w_full(),
                        h(Dim::Control),
                        px(S4),
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        gap(S2),
                        bg(Paint::Surface),
                    ],
                )
                .child(
                    rect().tw(tokens, &[w_flex()]).child(
                        CAPTION
                            .in_ink(Ink::Muted)
                            .label(tokens, facts)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
                )
                .children(tools),
        )
        .child(rect().tw(tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
        .into()
}
