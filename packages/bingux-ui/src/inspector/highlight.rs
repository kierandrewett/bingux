//! Draws the hovered node's box on the application canvas, as Chromium
//! DevTools does: the content, padding and margin bands, a solid outline around
//! the border box, and a hover card with the node's size, style and
//! accessibility facts, pointing its arrow at the element.

use freya::engine::prelude::{
    BlurStyle, Canvas, Font, FontMgr, FontStyle, MaskFilter, Paint, PaintStyle, PathBuilder, Rect,
};
use freya::prelude::Color;

use super::snapshot::Node;
use crate::style;

/// Type size of the hover card, in logical pixels.
const TEXT: f32 = 12.0;
/// Inner padding of the card.
const PAD: f32 = 12.0;
/// Height of one text line.
const LINE: f32 = 18.0;
/// Height of a separator rule.
const RULE: f32 = 9.0;
/// Gap between a label and its value, and between a swatch and its text.
const GAP: f32 = 6.0;
/// Side of a colour swatch.
const SWATCH: f32 = 10.0;
/// Side of the check mark.
const CHECK: f32 = 10.0;
/// Corner radius of the card.
const RADIUS: f32 = 4.0;
/// Width and height of the arrow that points at the element.
const ARROW_W: f32 = 12.0;
const ARROW_H: f32 = 6.0;
/// Distance kept between the card and the window edges.
const EDGE: f32 = 8.0;
/// The narrowest card, so short rows still read as a panel.
const MIN_WIDTH: f32 = 232.0;

/// Draws the hovered node of the latest snapshot, if it has an area.
pub fn draw(canvas: &Canvas) {
    let Some((snapshot, id)) = super::hovered() else {
        return;
    };
    let Some(tokens) = super::tokens() else {
        return;
    };
    let Some(node) = snapshot.node(id) else {
        return;
    };
    let Some(area) = node.area else {
        return;
    };
    let accent = style::Paint::Accent.color(&tokens);

    let border = rect(area.min_x(), area.min_y(), area.max_x(), area.max_y());
    let (padding, margin) = edges(node);
    let padding_box = inset(border, padding);
    let margin_box = inset(border, [-margin[0], -margin[1], -margin[2], -margin[3]]);

    fill_ring(
        canvas,
        margin_box,
        border,
        style::Paint::InspectMargin.color(&tokens),
    );
    fill_ring(
        canvas,
        border,
        padding_box,
        style::Paint::InspectPadding.color(&tokens),
    );
    fill(
        canvas,
        padding_box,
        style::Paint::InspectContent.color(&tokens),
    );
    canvas.draw_rect(border, &stroke(accent, 2.0));
    card(canvas, &tokens, border, node);
}

/// The padding and margin of a node as `[top, right, bottom, left]`; zero when
/// the node has no editable box.
fn edges(node: &Node) -> ([f32; 4], [f32; 4]) {
    match &node.props {
        Some(props) => (props.padding, props.margin),
        None => ([0.0; 4], [0.0; 4]),
    }
}

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> Rect {
    Rect::from_ltrb(left, top, right, bottom)
}

/// Moves each side of `rect` inward by `sides` (`[top, right, bottom, left]`).
/// Negative values move outward.
fn inset(rect: Rect, sides: [f32; 4]) -> Rect {
    Rect::from_ltrb(
        rect.left + sides[3],
        rect.top + sides[0],
        rect.right - sides[1],
        rect.bottom - sides[2],
    )
}

fn ink(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    paint
}

fn stroke(color: Color, width: f32) -> Paint {
    let mut paint = ink(color);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(width);
    paint
}

fn fill(canvas: &Canvas, rect: Rect, color: Color) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    canvas.draw_rect(rect, &ink(color));
}

/// Fills the part of `outer` that lies outside `inner`, as up to four strips so
/// the bands never overlap and the alpha stays exact.
fn fill_ring(canvas: &Canvas, outer: Rect, inner: Rect, color: Color) {
    fill(
        canvas,
        rect(outer.left, outer.top, outer.right, inner.top),
        color,
    );
    fill(
        canvas,
        rect(outer.left, inner.bottom, outer.right, outer.bottom),
        color,
    );
    fill(
        canvas,
        rect(outer.left, inner.top, inner.left, inner.bottom),
        color,
    );
    fill(
        canvas,
        rect(inner.right, inner.top, outer.right, inner.bottom),
        color,
    );
}

/// One value in the card: plain text, a colour with its CSS text, or a check
/// mark that reads `Yes` or `No`.
enum Value {
    Text(String),
    Swatch(Color, String),
    Check(bool),
}

/// One row of the card.
enum Line {
    /// The element's tag name and its `W × H` size.
    Header(String, String),
    Rule,
    Heading(&'static str),
    Row(&'static str, Value),
}

fn card_lines(node: &Node, border: Rect) -> Vec<Line> {
    let mut lines = vec![
        Line::Header(
            node.kind.name().to_string(),
            format!("{} × {}", border.width().round(), border.height().round()),
        ),
        Line::Rule,
    ];
    if let Some(props) = &node.props {
        lines.push(Line::Row(
            "Color",
            match props.text_color {
                Some(color) => Value::Swatch(color, css_colour(color)),
                None => Value::Text("inherited".to_string()),
            },
        ));
        lines.push(Line::Row(
            "Font",
            Value::Text(match props.font_size {
                Some(size) => format!("{}px", size.round()),
                None => "inherited".to_string(),
            }),
        ));
        lines.push(Line::Row(
            "Background",
            match props.background {
                Some(color) => Value::Swatch(color, css_colour(color)),
                None => Value::Text("none".to_string()),
            },
        ));
        lines.push(Line::Row("Padding", Value::Text(sides(props.padding))));
        lines.push(Line::Row("Margin", Value::Text(sides(props.margin))));
        lines.push(Line::Rule);
    }
    lines.push(Line::Heading("ACCESSIBILITY"));
    lines.push(Line::Row(
        "Name",
        Value::Text(node.a11y.name.clone().unwrap_or_else(|| "none".to_string())),
    ));
    lines.push(Line::Row("Role", Value::Text(node.a11y.role.clone())));
    lines.push(Line::Row(
        "Keyboard-focusable",
        Value::Check(node.a11y.focusable == Some(true)),
    ));
    lines
}

/// A colour as CSS would write it: `#rrggbb`, or `rgba()` when translucent.
fn css_colour(color: Color) -> String {
    if color.a() == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
    } else {
        format!(
            "rgba({}, {}, {}, {:.2})",
            color.r(),
            color.g(),
            color.b(),
            f32::from(color.a()) / 255.0
        )
    }
}

/// A box's sides as CSS shorthand, `[top, right, bottom, left]` in pixels.
fn sides(sides: [f32; 4]) -> String {
    let [top, right, bottom, left] = sides.map(|side| side.round() as i32);
    if top == right && right == bottom && bottom == left {
        format!("{top}px")
    } else if top == bottom && right == left {
        format!("{top}px {right}px")
    } else {
        format!("{top}px {right}px {bottom}px {left}px")
    }
}

fn measure(font: &Font, text: &str) -> f32 {
    font.measure_str(text, None).0
}

fn line_height(line: &Line) -> f32 {
    match line {
        Line::Rule => RULE,
        _ => LINE,
    }
}

fn value_width(font: &Font, value: &Value) -> f32 {
    match value {
        Value::Text(text) => measure(font, text),
        Value::Swatch(_, text) => SWATCH + GAP + measure(font, text),
        Value::Check(true) => CHECK + GAP + measure(font, "Yes"),
        Value::Check(false) => measure(font, "No"),
    }
}

fn draw_value(
    canvas: &Canvas,
    font: &Font,
    tokens: &crate::theme::Tokens,
    value: &Value,
    x: f32,
    top: f32,
) {
    let base = top + LINE * 0.72;
    let text = ink(style::Paint::Text.color(tokens));
    match value {
        Value::Text(label) => {
            canvas.draw_str(label, (x, base), font, &text);
        }
        Value::Swatch(color, label) => {
            let chip_top = top + (LINE - SWATCH) / 2.0;
            let chip = rect(x, chip_top, x + SWATCH, chip_top + SWATCH);
            fill(canvas, chip, *color);
            canvas.draw_rect(chip, &stroke(style::Paint::Separator.color(tokens), 1.0));
            canvas.draw_str(label, (x + SWATCH + GAP, base), font, &text);
        }
        Value::Check(true) => {
            let mid = top + LINE / 2.0;
            let mut tick = PathBuilder::new();
            tick.move_to((x, mid));
            tick.line_to((x + CHECK * 0.4, mid + CHECK * 0.4));
            tick.line_to((x + CHECK, mid - CHECK * 0.5));
            let tick = tick.detach();
            canvas.draw_path(
                &tick,
                &stroke(style::Paint::Success.color(tokens), 1.6),
            );
            canvas.draw_str("Yes", (x + CHECK + GAP, base), font, &text);
        }
        Value::Check(false) => {
            canvas.draw_str("No", (x, base), font, &text);
        }
    }
}

/// The hover card: the element's tag and size in its header, then its style
/// and accessibility facts, one line each. It sits just below the element, or
/// above it when there is no room, and its arrow points at the element.
fn card(canvas: &Canvas, tokens: &crate::theme::Tokens, border: Rect, node: &Node) {
    let Some(bounds) = canvas.local_clip_bounds() else {
        return;
    };
    let Some(typeface) = FontMgr::default().legacy_make_typeface(None, FontStyle::normal()) else {
        return;
    };
    let font = Font::from_typeface(typeface, TEXT);
    let lines = card_lines(node, border);

    let label_width = lines
        .iter()
        .filter_map(|line| match line {
            Line::Row(label, _) => Some(measure(&font, label)),
            _ => None,
        })
        .fold(0.0, f32::max)
        + GAP * 2.0;
    let content_width = lines
        .iter()
        .map(|line| match line {
            Line::Header(tag, size) => measure(&font, tag) + GAP * 2.0 + measure(&font, size),
            Line::Row(_, value) => label_width + value_width(&font, value),
            Line::Heading(heading) => measure(&font, heading),
            Line::Rule => 0.0,
        })
        .fold(0.0, f32::max);
    let width = content_width.max(MIN_WIDTH - PAD * 2.0) + PAD * 2.0;
    let height = PAD * 2.0 + lines.iter().map(line_height).sum::<f32>();

    let center_x = (border.left + border.right) / 2.0;
    let left = (center_x - width / 2.0).clamp(
        bounds.left + EDGE,
        (bounds.right - EDGE - width).max(bounds.left + EDGE),
    );
    let below = border.bottom + ARROW_H + 1.0;
    let above = border.top - ARROW_H - 1.0 - height;
    let fits_below = below + height <= bounds.bottom - EDGE;
    let fits_above = above >= bounds.top + EDGE;
    let top = if fits_below {
        below
    } else if fits_above {
        above
    } else {
        (bounds.bottom - EDGE - height).max(bounds.top + EDGE)
    };
    let card = rect(left, top, left + width, top + height);
    let arrow_up = top > border.bottom;

    let mut shadow = ink(style::Paint::InspectShadow.color(tokens));
    if let Some(filter) = MaskFilter::blur(BlurStyle::Normal, 4.0, None) {
        shadow.set_mask_filter(filter);
    }
    let lifted = rect(card.left, card.top + 2.0, card.right, card.bottom + 2.0);
    canvas.draw_round_rect(lifted, RADIUS, RADIUS, &shadow);
    canvas.draw_round_rect(
        card,
        RADIUS,
        RADIUS,
        &ink(style::Paint::Floating.color(tokens)),
    );
    canvas.draw_round_rect(
        card,
        RADIUS,
        RADIUS,
        &stroke(style::Paint::FloatingOutline.color(tokens), 1.0),
    );

    let arrow_x = center_x.clamp(
        card.left + RADIUS + ARROW_W / 2.0,
        card.right - RADIUS - ARROW_W / 2.0,
    );
    let (base, tip) = if arrow_up {
        (card.top, border.bottom + 1.0)
    } else {
        (card.bottom, border.top - 1.0)
    };
    let mut arrow = PathBuilder::new();
    arrow.move_to((arrow_x - ARROW_W / 2.0, base));
    arrow.line_to((arrow_x + ARROW_W / 2.0, base));
    arrow.line_to((arrow_x, tip));
    arrow.close();
    let arrow = arrow.detach();
    canvas.draw_path(&arrow, &ink(style::Paint::Floating.color(tokens)));

    let mut y = card.top + PAD;
    for line in &lines {
        match line {
            Line::Header(tag, size) => {
                let base = y + LINE * 0.72;
                canvas.draw_str(
                    tag,
                    (card.left + PAD, base),
                    &font,
                    &ink(style::Paint::SyntaxTag.color(tokens)),
                );
                let size_x = card.right - PAD - measure(&font, size);
                canvas.draw_str(
                    size,
                    (size_x, base),
                    &font,
                    &ink(style::Paint::Muted.color(tokens)),
                );
                y += LINE;
            }
            Line::Rule => {
                let mid = y + RULE / 2.0;
                canvas.draw_line(
                    (card.left, mid),
                    (card.right, mid),
                    &stroke(style::Paint::Separator.color(tokens), 1.0),
                );
                y += RULE;
            }
            Line::Heading(heading) => {
                canvas.draw_str(
                    heading,
                    (card.left + PAD, y + LINE * 0.72),
                    &font,
                    &ink(style::Paint::Muted.color(tokens)),
                );
                y += LINE;
            }
            Line::Row(label, value) => {
                canvas.draw_str(
                    label,
                    (card.left + PAD, y + LINE * 0.72),
                    &font,
                    &ink(style::Paint::Muted.color(tokens)),
                );
                draw_value(
                    canvas,
                    &font,
                    tokens,
                    value,
                    card.left + PAD + label_width,
                    y,
                );
                y += LINE;
            }
        }
    }
}
