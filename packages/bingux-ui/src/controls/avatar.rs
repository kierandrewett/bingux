//! A person's avatar: their initials on an accent circle.

use freya::prelude::*;

use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{EMPHASIS, Ink, TITLE};

/// An avatar of `size` showing `initials`. The large size (`Dim::Box24`)
/// uses title type, every smaller size emphasis type.
pub fn avatar(initials: impl Into<String>, size: Dim) -> Avatar {
    Avatar {
        initials: initials.into(),
        size,
    }
}

#[derive(PartialEq)]
pub struct Avatar {
    initials: String,
    size: Dim,
}

impl Component for Avatar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let style = if self.size == Dim::Box24 {
            TITLE
        } else {
            EMPHASIS
        };
        rect()
            .tw(
                &tokens,
                &[
                    w(self.size),
                    h(self.size),
                    rounded(Radius::Pill),
                    bg(Paint::Accent),
                    center(),
                ],
            )
            .child(
                style
                    .in_ink(Ink::OnAccent)
                    .label(&tokens, self.initials.clone()),
            )
    }
}
