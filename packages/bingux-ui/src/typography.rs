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

use crate::style::*;
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
    /// Text on an accent fill.
    OnAccent,
    /// Text on a danger, success or warning fill, and on the floating
    /// surface of a tooltip or toast.
    OnDanger,
    OnSuccess,
    OnWarning,
    OnFloating,
    /// Errors on the floating surface.
    DangerOnFloating,
    Success,
    Warning,
    /// Text and icons on the fill of a banner or toast of this kind.
    Notice(crate::theme::BannerKind),
}

impl Ink {
    pub fn color(self, tokens: &Tokens) -> Color {
        match self {
            Ink::Text => tokens.text,
            Ink::Muted => tokens.muted,
            Ink::Accent => tokens.accent_ink,
            Ink::Danger => tokens.danger_ink,
            Ink::OnAccent => tokens.on_accent,
            Ink::OnDanger => tokens.on_danger,
            Ink::OnSuccess => tokens.on_success,
            Ink::OnWarning => tokens.on_warning,
            Ink::OnFloating => tokens.on_floating,
            Ink::DangerOnFloating => tokens.danger_on_floating,
            Ink::Success => tokens.success_ink,
            Ink::Warning => tokens.warning_ink,
            Ink::Notice(kind) => tokens.notice(kind).ink,
        }
    }
}

/// One step of the ramp. To tint a step, use [`TextStyle::in_ink`] rather
/// than inventing a new size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: TextSize,
    pub weight: FontWeight,
    pub ink: Ink,
    /// Line height as a multiple of `size`.
    pub leading: f32,
    /// Extra space between letters, in px.
    pub tracking: f32,
    /// Digits share one width, so a changing number does not shift the text.
    pub tabular: bool,
    /// The monospace face: markup, attribute names and values, code.
    pub mono: bool,
}

impl TextStyle {
    /// The same style in another colour.
    pub const fn in_ink(self, ink: Ink) -> Self {
        Self { ink, ..self }
    }

    /// A single line in this style. Long text is cut by the caller with
    /// `max_lines` and `text_overflow`.
    pub fn label(self, tokens: &Tokens, text: impl Into<String>) -> Label {
        self.label_in_color(self.ink.color(tokens), text.into())
    }

    /// [`Self::label`] with the font size scaled by `scale`, rounded to the
    /// nearest quarter pixel. Leading and tracking stay as they are. A view
    /// that zooms its text uses this.
    pub fn label_scaled(self, tokens: &Tokens, text: impl Into<String>, scale: f32) -> Label {
        let size = (self.size.px() * scale * 4.0).round() / 4.0;
        self.label(tokens, text).font_size(size)
    }

    /// A paragraph of this style with the font size scaled by `scale`,
    /// rounded to the nearest quarter pixel, as [`Self::label_scaled`] does.
    pub fn scale_paragraph(self, paragraph: Paragraph, scale: f32) -> Paragraph {
        paragraph.font_size((self.size.px() * scale * 4.0).round() / 4.0)
    }

    /// A single line in this style, coloured by a surface paint rather than
    /// an ink. Developer markup uses it for its syntax colours.
    pub fn label_in_paint(self, tokens: &Tokens, text: impl Into<String>, paint: Paint) -> Label {
        self.label_in_color(paint.color(tokens), text.into())
    }

    fn label_in_color(self, color: Color, text: String) -> Label {
        let label = label()
            .text(crate::stress::localised(text))
            .color(color)
            .font_size(self.size.px())
            .font_weight(self.weight)
            .line_height(self.leading)
            .letter_spacing(self.tracking);
        let label = if self.mono { label.font_family(mono_family()) } else { label };
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
            .font_size(self.size.px())
            .font_weight(self.weight)
            .line_height(self.leading)
            .letter_spacing(self.tracking)
            // A span, not a child: paragraph highlights (search marks) are
            // only painted for the paragraph's own spans.
            .span(crate::stress::localised(text.into()));
        let paragraph = if self.mono { paragraph.font_family(mono_family()) } else { paragraph };
        if self.tabular {
            paragraph.font_tabular()
        } else {
            paragraph
        }
    }

    /// A styled piece of text for a [`SelectableText`], which takes spans
    /// rather than a paragraph, so the text can be selected and copied.
    pub fn span(self, tokens: &Tokens, text: impl Into<String>) -> Span<'static> {
        self.span_in_color(self.ink.color(tokens), text.into())
    }

    /// A span in this style, coloured by a surface paint rather than an ink,
    /// as [`Self::label_in_paint`] does for a label.
    pub fn span_in_paint(self, tokens: &Tokens, text: impl Into<String>, paint: Paint) -> Span<'static> {
        self.span_in_color(paint.color(tokens), text.into())
    }

    fn span_in_color(self, color: Color, text: String) -> Span<'static> {
        let span = Span::new(crate::stress::localised(text))
            .color(color)
            .font_size(self.size.px())
            .font_weight(self.weight)
            .letter_spacing(self.tracking);
        let span = if self.mono { span.font_family(mono_family()) } else { span };
        if self.tabular {
            span.font_tabular()
        } else {
            span
        }
    }

    /// Text in this style that can be selected and copied with the mouse,
    /// wrapping like a paragraph. Use it wherever the text is worth copying,
    /// such as a value, a caption or a status. It takes no search marks, as a
    /// selection takes the only highlight slot.
    pub fn selectable(self, tokens: &Tokens, text: impl Into<String>) -> SelectableText {
        SelectableText::new()
            .span(self.span(tokens, text))
            .line_height(self.leading)
    }
}

/// A single large value: a clock, a percentage, the figure a page is about.
pub const HERO: TextStyle = TextStyle {
    size: TextSize::Hero,
    weight: FontWeight::LIGHT,
    ink: Ink::Text,
    leading: LEADING_TIGHT,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// A page or detail title: the name at the head of an identity block or a
/// subpage.
pub const TITLE: TextStyle = TextStyle {
    size: TextSize::Display,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Text,
    leading: LEADING_TIGHT,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// The name of a page, at the head of its column.
pub const PAGE_TITLE: TextStyle = TextStyle {
    size: TextSize::Display,
    weight: FontWeight::MEDIUM,
    ink: Ink::Text,
    leading: LEADING_TIGHT,
    tracking: -0.3,
    tabular: false,
    mono: false,
};

/// A group title, above its card. Clearly larger and heavier than row text:
/// 16 px semibold.
pub const SECTION: TextStyle = TextStyle {
    size: TextSize::Lg,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

pub const HEADING: TextStyle = TextStyle {
    size: TextSize::Heading,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// Row titles and body text.
pub const BODY: TextStyle = TextStyle {
    size: TextSize::Md,
    weight: FontWeight::NORMAL,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// Body text that must stand out in its row: a name or a state.
pub const EMPHASIS: TextStyle = TextStyle {
    size: TextSize::Md,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// A read-only value on the right of a row.
pub const VALUE: TextStyle = TextStyle {
    size: TextSize::Md,
    weight: FontWeight::NORMAL,
    ink: Ink::Muted,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// A number that changes, such as a count or a level.
pub const NUMERIC: TextStyle = TextStyle {
    size: TextSize::Md,
    weight: FontWeight::NORMAL,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: true,
    mono: false,
};

/// A subtitle, caption or note under a title.
pub const CAPTION: TextStyle = TextStyle {
    size: TextSize::Sm,
    weight: FontWeight::NORMAL,
    ink: Ink::Muted,
    leading: LEADING_LOOSE,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// A badge or a count after a title.
pub const BADGE: TextStyle = TextStyle {
    size: TextSize::Xs,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Accent,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// Small capitals above a block of text, tracked out.
pub const OVERLINE: TextStyle = TextStyle {
    size: TextSize::Xs,
    weight: FontWeight::SEMI_BOLD,
    ink: Ink::Muted,
    leading: LEADING,
    tracking: TRACKING_OVERLINE,
    tabular: false,
    mono: false,
};

/// A link or an action set in text.
pub const LINK: TextStyle = TextStyle {
    size: TextSize::Md,
    weight: FontWeight::MEDIUM,
    ink: Ink::Accent,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
    mono: false,
};

/// Code: markup, attribute names and values, in the monospace face at
/// 13 px. Developer tools use it; app pages never do.
pub const CODE: TextStyle = TextStyle {
    size: TextSize::Sm,
    weight: FontWeight::NORMAL,
    ink: Ink::Text,
    leading: LEADING,
    tracking: 0.0,
    tabular: false,
    mono: true,
};

/// The monospace family: Adwaita Mono, the shell's own, or the fontconfig
/// monospace default when Adwaita Mono is not installed.
static MONO_FAMILY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    if !crate::theme::font_files("Adwaita Mono", 1).is_empty() {
        return "Adwaita Mono".to_string();
    }
    std::process::Command::new("fc-match")
        .args(["-f", "%{family}", "monospace"])
        .output()
        .ok()
        .and_then(|output| {
            let text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.split(',').next().map(|name| name.trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "monospace".to_string())
});

/// The monospace family name, for [`TextStyle::mono`] text.
pub fn mono_family() -> &'static str {
    MONO_FAMILY.as_str()
}

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
