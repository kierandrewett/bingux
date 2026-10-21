//! Page hero: an icon tile, a title and a one-line description, like the
//! top of a GNOME Settings page or a macOS System Settings pane.

use freya::prelude::*;

use crate::icons::{Icon, icon_tile};
use crate::surface::{FONT_SIZE, use_tokens};

/// A page hero. Build one with [`hero`].
#[derive(PartialEq)]
pub struct Hero {
    title: String,
    description: Option<String>,
    icon: Option<(Icon, [f32; 3])>,
}

pub fn hero(title: impl Into<String>) -> Hero {
    Hero {
        title: title.into(),
        description: None,
        icon: None,
    }
}

impl Hero {
    pub fn description(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.description = (!text.is_empty()).then_some(text);
        self
    }

    /// A coloured icon tile beside the title.
    pub fn icon(mut self, icon: Icon, rgb: [f32; 3]) -> Self {
        self.icon = Some((icon, rgb));
        self
    }
}

impl Component for Hero {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .spacing(16.0)
            .margin((8.0, 4.0, 24.0, 4.0))
            .maybe_child(self.icon.map(|(icon, rgb)| icon_tile(icon, rgb, 56.0)))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .spacing(2.0)
                    .child(
                        label()
                            .text(self.title.clone())
                            .font_size(24.0)
                            .font_weight(FontWeight::BOLD)
                            .max_lines(1),
                    )
                    .maybe_child(self.description.as_ref().map(|text| {
                        label()
                            .text(text.clone())
                            .color(tokens.muted)
                            .font_size(FONT_SIZE)
                            .max_lines(2)
                    })),
            )
    }
}
