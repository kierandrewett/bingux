//! The picture of an album, a song, an artist or a film: [`artwork`].
//!
//! The picture is a file (cover art pulled out of a song, a `cover.jpg`, a
//! frame of a film), decoded on a worker at the size it is drawn and cropped
//! to fill its square (or 16:9 box for a film). Without one, or while it
//! loads, or when it cannot be read, the box shows a fallback that is never
//! empty and never the same grey for everything: a gradient whose two
//! colours come from the name (an album keeps its colours everywhere it
//! appears), with the kind's glyph on it. An artist is round, everything else
//! is a rounded square.

use std::path::PathBuf;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;

/// What the picture stands for: its fallback glyph and its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArtworkKind {
    Album,
    Song,
    /// Round.
    Artist,
    /// 16:9.
    Video,
    Playlist,
}

impl ArtworkKind {
    fn glyph(self) -> Icon {
        match self {
            ArtworkKind::Album => Icon::MediaOptical,
            ArtworkKind::Song => Icon::AudioXGeneric,
            ArtworkKind::Artist => Icon::AvatarDefault,
            ArtworkKind::Video => Icon::VideoGeneric,
            ArtworkKind::Playlist => Icon::ViewListOrdered,
        }
    }
}

/// A picture with its fallback. Build with [`artwork`].
#[derive(PartialEq)]
pub struct Artwork {
    source: Option<PathBuf>,
    seed: String,
    kind: ArtworkKind,
    size: Dim,
    side: Option<f32>,
    label: Option<String>,
}

/// The picture at `source`, or the fallback made from `seed` (the album's or
/// artist's name). 96 px square by default.
pub fn artwork(source: Option<PathBuf>, seed: impl Into<String>) -> Artwork {
    Artwork {
        source,
        seed: seed.into(),
        kind: ArtworkKind::Album,
        size: Dim::Box24,
        side: None,
        label: None,
    }
}

impl Artwork {
    pub fn kind(mut self, kind: ArtworkKind) -> Self {
        self.kind = kind;
        self
    }

    /// The width, as a size token.
    pub fn size(mut self, size: Dim) -> Self {
        self.size = size;
        self.side = None;
        self
    }

    /// The width in logical pixels, for a grid that works out its tile size
    /// from the room it has.
    pub fn side(mut self, side: f32) -> Self {
        self.side = Some(side.max(1.0));
        self
    }

    /// What a screen reader calls it ("Northern Lights, cover"). Without one
    /// the picture is decoration, left out of the accessibility tree.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// The two colours of the fallback for `seed`: one hue from the name, the
/// second a third of the way round from it, both mid-light so white reads on
/// them (WCAG non-text contrast).
pub(crate) fn fallback_colours(seed: &str) -> (Color, Color) {
    // FNV-1a: stable across runs and platforms.
    let hash = seed.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    let hue = (hash % 360) as f32;
    let second = (hue + 40.0 + ((hash >> 16) % 80) as f32) % 360.0;
    (hsl(hue, 0.55, 0.42), hsl(second, 0.60, 0.30))
}

fn hsl(hue: f32, saturation: f32, lightness: f32) -> Color {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let part = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (chroma, part, 0.0),
        1 => (part, chroma, 0.0),
        2 => (0.0, chroma, part),
        3 => (0.0, part, chroma),
        4 => (part, 0.0, chroma),
        _ => (chroma, 0.0, part),
    };
    let base = lightness - chroma / 2.0;
    let channel = |value: f32| ((value + base) * 255.0).round().clamp(0.0, 255.0) as u8;
    // ui-lint: allow(an artwork fallback's colours are data made from a name, not a role)
    Color::from_rgb(channel(r), channel(g), channel(b))
}

impl Component for Artwork {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let width = self.side.unwrap_or_else(|| self.size.px());
        let height = if self.kind == ArtworkKind::Video {
            (width * 9.0 / 16.0).round()
        } else {
            width
        };
        let radius = match self.kind {
            ArtworkKind::Artist => Radius::Pill,
            _ if width >= 96.0 => Radius::Card,
            _ => Radius::Control,
        };
        let (top, bottom) = fallback_colours(&self.seed);
        let glyph_size = if width >= 96.0 {
            IconSize::Hero
        } else if width >= 40.0 {
            IconSize::Row
        } else {
            IconSize::Control
        };
        // ui-lint: allow(the glyph sits on the fallback's own data colours, white reads on both)
        let glyph_colour = Color::from_af32rgb(0.85, 255, 255, 255);
        let fallback: Element = gradient_fill(
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .tw(&tokens, &[center()]),
            LinearGradient::new()
                .angle(315.0)
                .stop(GradientStop::new(top, 0.0))
                .stop(GradientStop::new(bottom, 100.0)),
        )
        .child(icons::icon(self.kind.glyph(), glyph_size, glyph_colour))
        .into();
        let picture: Element = match &self.source {
            Some(path) => {
                let error = fallback.clone();
                ImageViewer::new(path.clone())
                    .width(Size::fill())
                    .height(Size::fill())
                    .aspect_ratio(AspectRatio::Max)
                    .image_cover(ImageCover::Center)
                    .loading_placeholder(fallback.clone())
                    .error_renderer(move |_: String| error.clone())
                    .into()
            }
            None => fallback,
        };
        rect()
            .component("Artwork")
            .maybe(self.label.is_some(), |element| {
                element
                    .a11y_role(AccessibilityRole::Image)
                    .a11y_alt(self.label.clone().unwrap_or_default())
            })
            .width(Size::px(width))
            .height(Size::px(height))
            .tw(&tokens, &[rounded(radius), clip(), bg(Paint::Control)])
            .child(picture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_keeps_its_colours() {
        assert_eq!(
            fallback_colours("Northern Lights"),
            fallback_colours("Northern Lights")
        );
        assert_ne!(
            fallback_colours("Northern Lights"),
            fallback_colours("Sine Waves")
        );
    }
}
