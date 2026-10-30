//! A stat tile: one count with its label beneath and an icon in a tone. Use it
//! for a figure that needs a glance, such as the problems of each severity.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, NUMERIC};

/// `value` is the figure ("3"), `label` the text beneath it ("Errors").
#[derive(PartialEq)]
pub struct StatTile {
    value: String,
    label: String,
    icon: Option<Icon>,
    tone: Option<Tone>,
}

pub fn stat_tile(value: impl Into<String>, label: impl Into<String>) -> StatTile {
    StatTile {
        value: value.into(),
        label: label.into(),
        icon: None,
        tone: None,
    }
}

impl StatTile {
    /// An icon at the left, drawn in the tone's colour.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The colour of the icon: accent, warning or danger.
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = Some(tone);
        self
    }
}

impl Component for StatTile {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let tone = self.tone.map(|tone| tone.color(&tokens));
        let icon_colour = tone.unwrap_or_else(|| Paint::Muted.color(&tokens));
        rect()
            .component("StatTile")
            .width(Size::fill())
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S3),
                    p(S4),
                    rounded(Radius::Card),
                    bg(Paint::Card),
                    border(Line::Hairline),
                    clip(),
                ],
            )
            .maybe_child(
                self.icon
                    .map(|icon| icons::icon(icon, IconSize::Row, icon_colour)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .tw(&tokens, &[flex_col(), gap(S0_5)])
                    .child(NUMERIC.label(&tokens, self.value.clone()).max_lines(1))
                    .child(CAPTION.label(&tokens, self.label.clone()).max_lines(1)),
            )
    }
}
