//! A small SVG writer for chart scenes: paths, dots, gradients and clip
//! rectangles, with numbers kept short.

use std::fmt::Write as _;

use freya::prelude::Color;

/// One chart's SVG: definitions (gradients, clip paths) and drawing, in order.
pub(crate) struct Svg {
    width: f32,
    height: f32,
    defs: String,
    body: String,
    ids: u32,
}

impl Svg {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            defs: String::new(),
            body: String::new(),
            ids: 0,
        }
    }

    /// A fresh id for a definition, unique within this chart.
    pub fn id(&mut self, prefix: &str) -> String {
        self.ids += 1;
        format!("{prefix}{}", self.ids)
    }

    pub fn def(&mut self, markup: impl AsRef<str>) {
        self.defs.push_str(markup.as_ref());
    }

    pub fn push(&mut self, markup: impl AsRef<str>) {
        self.body.push_str(markup.as_ref());
    }

    pub fn finish(self) -> String {
        let (w, h) = (num(self.width), num(self.height));
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs>{}</defs>{}</svg>"#,
            self.defs, self.body
        )
    }
}

/// A number for SVG: two decimals, no trailing zeros, never `NaN`.
pub(crate) fn num(value: f32) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == 0.0 {
        "0".into()
    } else {
        format!("{rounded}")
    }
}

/// `#rrggbb` for a colour.
pub(crate) fn hex(colour: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b())
}

/// Opacity in `0..=1`.
pub(crate) fn alpha(colour: Color) -> f32 {
    f32::from(colour.a()) / 255.0
}

/// The attributes that paint `attribute` (`fill` or `stroke`) with `colour`.
pub(crate) fn paint(attribute: &str, colour: Color) -> String {
    format!(
        "{attribute}=\"{}\" {attribute}-opacity=\"{}\"",
        hex(colour),
        num(alpha(colour))
    )
}

/// A straight stroked line.
pub(crate) fn stroke_line(x1: f32, y1: f32, x2: f32, y2: f32, colour: Color, width: f32) -> String {
    format!(
        r#"<path d="M{} {}L{} {}" fill="none" {} stroke-width="{}"/>"#,
        num(x1),
        num(y1),
        num(x2),
        num(y2),
        paint("stroke", colour),
        num(width)
    )
}

/// A filled circle, with an optional outline of `(colour, width)`.
pub(crate) fn dot(
    cx: f32,
    cy: f32,
    radius: f32,
    fill: Color,
    ring: Option<(Color, f32)>,
) -> String {
    let outline = match ring {
        Some((colour, width)) => format!(
            " {} stroke-width=\"{}\"",
            paint("stroke", colour),
            num(width)
        ),
        None => String::new(),
    };
    format!(
        r#"<circle cx="{}" cy="{}" r="{}" {}{}/>"#,
        num(cx),
        num(cy),
        num(radius),
        paint("fill", fill),
        outline
    )
}

/// A linear gradient from `(offset, colour)` stops. Vertical runs top to
/// bottom, otherwise left to right, across the bounding box of what it paints.
pub(crate) fn linear_gradient(id: &str, stops: &[(f32, Color)], vertical: bool) -> String {
    let (x2, y2) = if vertical { ("0", "1") } else { ("1", "0") };
    let mut out = format!(r#"<linearGradient id="{id}" x1="0" y1="0" x2="{x2}" y2="{y2}">"#);
    for (offset, colour) in stops {
        let _ = write!(
            out,
            "<stop offset=\"{}\" stop-color=\"{}\" stop-opacity=\"{}\"/>",
            num(*offset),
            hex(*colour),
            num(alpha(*colour))
        );
    }
    out.push_str("</linearGradient>");
    out
}

/// A clip path covering `0..width` by `0..height`, used to reveal a mark
/// from the left as it draws in.
pub(crate) fn clip_rect(id: &str, width: f32, height: f32) -> String {
    format!(
        r#"<clipPath id="{id}"><rect x="0" y="0" width="{}" height="{}"/></clipPath>"#,
        num(width),
        num(height)
    )
}
