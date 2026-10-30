//! Helpers the demos share.

use bingux_ui::prelude::*;
use freya::prelude::*;

/// An untitled group of rows: how a row is shown in a stage.
pub fn rows(rows: Vec<Element>) -> Element {
    Group::new(None::<String>).rows(rows).into_element()
}

/// A short title for a row, with a control at its right.
pub fn with(title: &str, control: impl Into<Element> + 'static) -> Element {
    row(title).trailing(control).into_element()
}

/// Text that does not fit a row: the long-text state of a component.
pub const LONG: &str =
    "A name far too long for the room it is given, which keeps going and going and going";

/// A phrase with a script that needs no latin letters, for the overflow
/// states: bingux-ui must not assume ASCII widths.
pub const WIDE: &str = "ネットワークの設定とセキュリティ";
