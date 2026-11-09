//! The colours of highlighted code, derived from the tokens so they follow
//! light and dark. Each kind of text has a hue (the families Xcode and GNOME
//! Builder use: keywords pink, strings red-orange, numbers violet, types
//! teal, functions blue, comments grey) at a lightness for the scheme, and
//! is then moved in lightness until it reads at 4.5:1 on the editor's
//! background and on the current-line band. The colour is data computed from
//! tokens, not a token, as a chart's series colours are.

use freya::prelude::*;

use bingux_text::TokenKind;

use crate::contrast::{Oklch, ensure_contrast};
use crate::style::Paint;
use crate::theme::Tokens;

/// How a kind of text is set: its colour and whether it is bold or italic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub colour: Color,
    pub bold: bool,
    pub italic: bool,
}

/// Every kind's colour for one set of tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct SyntaxPalette {
    inks: Vec<(TokenKind, Ink)>,
    pub plain: Color,
    pub faint: Color,
}

/// The least contrast of highlighted text on the editor background.
const TEXT_CONTRAST: f32 = 4.5;

/// Hue (OKLCH degrees) and chroma of each kind; `None` is the text colour.
fn hue(kind: TokenKind) -> Option<(f32, f32)> {
    use TokenKind::*;
    Some(match kind {
        Keyword => (350.0, 0.16),
        String => (35.0, 0.14),
        Escape => (60.0, 0.15),
        Number => (300.0, 0.14),
        Constant => (300.0, 0.14),
        Type => (195.0, 0.11),
        Function => (250.0, 0.14),
        Property => (220.0, 0.09),
        Tag => (250.0, 0.14),
        Attribute => (70.0, 0.12),
        Heading => (250.0, 0.14),
        Link => (250.0, 0.14),
        Code => (35.0, 0.12),
        ListMarker => (350.0, 0.14),
        Inserted => (150.0, 0.14),
        Deleted => (25.0, 0.16),
        Changed => (250.0, 0.12),
        Invalid => (25.0, 0.18),
        Comment | Operator | Variable | Emphasis | Strong | Quote => return None,
    })
}

impl SyntaxPalette {
    pub fn new(tokens: &Tokens) -> Self {
        let dark = crate::contrast::luminance(tokens.bg) < 0.2;
        let surfaces = [
            Paint::Bg.color(tokens),
            Paint::Hover
                .color(tokens)
                .blend_over(Paint::Bg.color(tokens)),
        ];
        let plain = Paint::SyntaxText.color(tokens);
        let muted = Paint::SyntaxComment.color(tokens);
        let lightness = if dark { 0.80 } else { 0.50 };
        let inks = TokenKind::ALL
            .iter()
            .map(|kind| {
                let colour = match (kind, hue(*kind)) {
                    (TokenKind::Comment, _) | (TokenKind::Quote, _) => muted,
                    (_, Some((h, c))) => ensure_contrast(
                        Oklch { l: lightness, c, h }.to_colour(),
                        &surfaces,
                        TEXT_CONTRAST,
                    ),
                    (_, None) => plain,
                };
                let ink = Ink {
                    colour,
                    bold: matches!(kind, TokenKind::Heading | TokenKind::Strong),
                    italic: matches!(
                        kind,
                        TokenKind::Emphasis | TokenKind::Comment | TokenKind::Quote
                    ),
                };
                (*kind, ink)
            })
            .collect();
        Self {
            inks,
            plain,
            faint: Paint::MutedFaint.color(tokens),
        }
    }

    pub fn ink(&self, kind: TokenKind) -> Ink {
        self.inks
            .iter()
            .find(|(candidate, _)| *candidate == kind)
            .map(|(_, ink)| *ink)
            .unwrap_or(Ink {
                colour: self.plain,
                bold: false,
                italic: false,
            })
    }
}

trait BlendOver {
    fn blend_over(self, below: Color) -> Color;
}

/// A run of code in `ink` at `font` px, in the monospace face. The colour
/// and weight are data from the palette, which is why this file sets them.
pub(crate) fn code_span(text: String, ink: Ink, font: f32) -> Span<'static> {
    let span = Span::new(text)
        .color(ink.colour)
        .font_size(font)
        .font_family(crate::typography::mono_family());
    let span = if ink.bold {
        span.font_weight(FontWeight::BOLD)
    } else {
        span
    };
    if ink.italic {
        span.font_slant(FontSlant::Italic)
    } else {
        span
    }
}

/// One row of code: a single line of the monospace face, `font` px, laid
/// out `row_height` px tall so rows sit on a grid.
pub(crate) fn code_row(font: f32, row_height: f32) -> Paragraph {
    paragraph()
        .font_family(crate::typography::mono_family())
        .font_size(font)
        .line_height(row_height / font)
        .max_lines(1)
}

impl BlendOver for Color {
    fn blend_over(self, below: Color) -> Color {
        crate::contrast::composite(self, below)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_reads_on_the_background() {
        for scheme in [
            crate::theme::Scheme::PreferDark,
            crate::theme::Scheme::PreferLight,
        ] {
            let tokens = Tokens::new("blue", scheme, false);
            let palette = SyntaxPalette::new(&tokens);
            for kind in TokenKind::ALL {
                let ink = palette.ink(kind);
                let ratio = crate::contrast::ratio(ink.colour, tokens.bg);
                assert!(ratio >= 4.0, "{kind:?} in {scheme:?}: {ratio:.2}");
            }
        }
    }
}
