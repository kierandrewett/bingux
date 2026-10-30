//! The mark on a dialog's default button: what Enter does when nothing else
//! has the keyboard focus.
//!
//! It is a small key cap with the Return glyph, after the button's label, in
//! the ink the button already uses: the on-colour of a coloured button
//! ([`Ink::OnDanger`], [`Ink::OnAccent`]), the text colour of a plain one, so
//! it reads on every fill with the contrast the contrast matrix already
//! measures for text on that fill.
//!
//! Why a key cap and not a ring in the button's colour. A dialog has two
//! things that point at a button: the keyboard focus (Tab) and the default
//! action (Enter). A ring for both, however it is coloured, is one shape in
//! two places: after Tab the focus ring is on one button and the default ring
//! on another, and which of them has the focus has to be read from a colour.
//! The focus ring is the only ring. The default is a label: it says "Enter"
//! on the button Enter will press, whether or not that button also has the
//! focus, and it never changes the button's outline, so it cannot fight the
//! focus ring or the destructive colour. The mark is drawn only at regular
//! density, where there is a keyboard; a phone shows the focus ring alone.

use freya::prelude::*;

use crate::style::*;
use crate::theme::Tokens;
use crate::typography::{CAPTION, Ink};

/// The fill of a button, which decides the ink the mark is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Face {
    Plain,
    Accent,
    Danger,
}

impl Face {
    fn paint(self) -> Paint {
        match self {
            Face::Plain => Paint::Text,
            Face::Accent => Paint::OnAccent,
            Face::Danger => Paint::OnDanger,
        }
    }

    fn ink(self) -> Ink {
        match self {
            Face::Plain => Ink::Text,
            Face::Accent => Ink::OnAccent,
            Face::Danger => Ink::OnDanger,
        }
    }
}

/// The key cap that follows the label of a default button on a `face`.
pub(crate) fn default_mark(tokens: &Tokens, face: Face) -> Rect {
    rect()
        .interactive(false)
        .tw(
            tokens,
            &[
                flex_row(),
                center(),
                h(Dim::Box5),
                px(S1),
                rounded(Radius::Mark),
                border(Line::Edge(face.paint())),
            ],
        )
        .child(CAPTION.in_ink(face.ink()).label(tokens, "↵"))
}
