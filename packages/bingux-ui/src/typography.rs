//! The type ramp: every text style Bingux UI uses, named by its job.
//!
//! A style is a size, a weight, an ink (the token that colours it), a line
//! height and, for a few, tracking or tabular figures. Sizes step by about
//! 1.25x or more (11, 12, 14, 18, 24, 36), and each level differs from the
//! next in at least two of size, weight and ink, so the hierarchy reads at
//! a glance: page title, group title, row title, subtitle.
//!
//! Pages use these names and never pick a size and weight of their own.
//! `scripts/ui-lint.py` checks the sizes; the weights are chosen here.

use freya::prelude::*;

use crate::scale::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;

/// Line heights, as a multiple of the font size. Large text sits tighter;
/// small text opens up so it reads easily.
const LEADING_TIGHT: f32 = 1.1;
const LEADING: f32 = 1.3;
const LEADING_LOOSE: f32 = 1.45;

/// Letter spacing for small capitals, in px.
const TRACKING_OVERLINE: f32 = 0.6;

/// The colour of a text style, as a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// Primary text: titles, row names, body.
    Text,
    /// Secondary text: subtitles, captions, read-only values.
    Muted,
    /// The accent: badges and links.
    Accent,
    /// Errors and destructive actions.
    Danger,
}

impl Ink {
    pub fn color(self, tokens: &Tokens) -> Color {
        match self {
            Ink::Text => tokens.text,
            Ink::Muted => tokens.muted,
            Ink::Accent => tokens.accent,
            Ink::Danger => tokens.danger,
        }
    }
}

/// One step of the ramp. To tint a step, use [`TextStyle::in_ink`] rather
/// than inventing a new size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: FontWeight,
    pub ink: Ink,
    /// Line height as a multiple of `size`.
    pub leading: f32,
    /// Extra space between letters, in px.
    pub tracking: f32,
    /// Digits share one width, so a changing number does not shift the text.
    pub tabular: bool,
}

impl TextStyle {
    /// The same style in another colour.
    pub const fn in_ink(self, ink: Ink) -> Self {
        Self { ink, ..self }
    }

    /// A single line in this style. Long text is cut by the caller with
    /// `max_lines` and `text_overflow`.
    pub fn label(self, tokens: &Tokens, text: impl Into<String>) -> Label {
        let label = label()
            .text(text.into())
            .color(self.ink.color(tokens))
            .font_size(self.size)
            .font_weight(self.weight)
            .line_height(self.leading)
            .letter_spacing(self.tracking);
        if self.tabular {
            label.font_tabular()
        } else {
            label
        }
    }

    /// Text in this style that wraps onto as many lines as it needs.
    pub fn paragraph(self, tokens: &Tokens, text: impl Into<String>) -> Paragraph {
        let paragraph = paragraph()
            .color(self.ink.color(tokens))
            .font_size(self.size)
            .font_weight(self.weight)
            .line_height(self.leading)
            .letter_spacing(self.tracking)
            .child(text.into());
        if self.tabular {
            paragraph.font_tabular()
        } else {
            paragraph
        }
    }
}

/// A single large value: a clock, a percentage, the figure a page is about.
pub const HERO: TextStyle = TextStyle {
    size: TEXT_HERO,
    weight: FontWeight::LIGHT,
    ink: Ink::Text,
    leading: LEADING_TIGHT,
    tracking: 0.0,
    tabular: false,
};

/// A page or detail title: the name at the head of an identity block or a
/// subpage.
pub const TITLE: TextStyle = TextStyle {
    size: TEXT_DISPLAY,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Text,
    leading: LEADING_TIGHT,
    tracking: 0.0,
    tabular: false,
};

/// A group title, above its card. Clearly larger and heavier than row text.
/// A group title above its rows: the shell's `SettingsHeading`, 14 px
/// semibold.
pub const SECTION: TextStyle = TextStyle {
    size: TEXT_MD,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
};

pub const HEADING: TextStyle = TextStyle {
    size: TEXT_HEADING,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
};

/// Row titles and body text.
pub const BODY: TextStyle = TextStyle {
    size: TEXT_MD,
    weight: FontWeight::NORMAL,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
};

/// Body text that must stand out in its row: a name or a state.
pub const EMPHASIS: TextStyle = TextStyle {
    size: TEXT_MD,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
};

/// A read-only value on the right of a row.
pub const VALUE: TextStyle = TextStyle {
    size: TEXT_MD,
    weight: FontWeight::NORMAL,
    ink: Ink::Muted,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
};

/// A number that changes, such as a count or a level.
pub const NUMERIC: TextStyle = TextStyle {
    size: TEXT_MD,
    weight: FontWeight::NORMAL,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: true,
};

/// A subtitle, caption or note under a title.
pub const CAPTION: TextStyle = TextStyle {
    size: TEXT_SM,
    weight: FontWeight::NORMAL,
    ink: Ink::Muted,
    leading: LEADING_LOOSE,
    tracking: 0.0,
    tabular: false,
};

/// A badge or a count after a title.
pub const BADGE: TextStyle = TextStyle {
    size: TEXT_XS,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Accent,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
};

/// Small capitals above a block of text, tracked out.
pub const OVERLINE: TextStyle = TextStyle {
    size: TEXT_XS,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Muted,
    leading: LEADING,
    tracking: TRACKING_OVERLINE,
    tabular: false,
};

/// A link or an action set in text.
pub const LINK: TextStyle = TextStyle {
    size: TEXT_MD,
    weight: FontWeight::MEDIUM,
    ink: Ink::Accent,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
};

/// A single line of text in a style, for a component that has no tokens.
pub fn styled_label(style: TextStyle, text: impl Into<String>) -> Element {
    StyledLabel {
        style,
        text: text.into(),
    }
    .into()
}

#[derive(PartialEq)]
struct StyledLabel {
    style: TextStyle,
    text: String,
}

impl Component for StyledLabel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        self.style.label(&tokens, self.text.clone())
    }
}

/// Wrapping text in a style, for a component that has no tokens.
pub fn styled_paragraph(style: TextStyle, text: impl Into<String>) -> Element {
    StyledParagraph {
        style,
        text: text.into(),
    }
    .into()
}

#[derive(PartialEq)]
struct StyledParagraph {
    style: TextStyle,
    text: String,
}

impl Component for StyledParagraph {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        self.style.paragraph(&tokens, self.text.clone())
    }
}
