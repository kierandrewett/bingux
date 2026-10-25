//! Style properties of a node: what each one shows, how it is typed, and how
//! an edit becomes a live override on the selected node of the app window, so
//! layout and paint change at once.

use std::any::Any;
use std::rc::Rc;

use freya::prelude::*;
use freya::torin::geometry::Length;
use freya::torin::node::Node as LayoutNode;
use freya::torin::prelude::{Alignment, Direction, Gaps, Size};
use freya::winit::window::WindowId;

use super::overrides::{self, Edit};
use super::snapshot::{Kind, Node};
use crate::style::{colour_from_hex, colour_to_hex};

/// How a property's value is typed and edited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// `auto`, `fill`, `50%` or pixels.
    Size,
    /// Pixels.
    Pixels,
    /// One value, or top right bottom left.
    Gaps,
    /// `#rrggbb` or `#aarrggbb`.
    Colour,
    /// `vertical` or `horizontal`.
    Direction,
    /// `start`, `center`, `end`, `space-between`, `space-evenly` or `space-around`.
    Alignment,
    /// Shown but not editable.
    ReadOnly,
}

/// One property a node declares, as the styles pane shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Declared {
    pub name: &'static str,
    pub value: String,
    pub input: Input,
}

/// Every property the node declares, in display order: layout, then the box,
/// then text. Read-only limits are listed too, so a node's sizing is visible.
pub fn declared(node: &Node) -> Vec<Declared> {
    let Some(props) = node.props.as_ref() else {
        return Vec::new();
    };
    let is_rect = matches!(node.kind, Kind::Rect);
    let mut out = vec![
        declared_row("width", props.width.clone(), Input::Size),
        declared_row("height", props.height.clone(), Input::Size),
        declared_row("min-width", props.min_width.clone(), Input::ReadOnly),
        declared_row("min-height", props.min_height.clone(), Input::ReadOnly),
        declared_row("max-width", props.max_width.clone(), Input::ReadOnly),
        declared_row("max-height", props.max_height.clone(), Input::ReadOnly),
        declared_row("padding", gaps_text(&props.padding), Input::Gaps),
        declared_row("margin", gaps_text(&props.margin), Input::Gaps),
        declared_row("gap", px_text(props.gap), Input::Pixels),
        declared_row("direction", props.direction.to_string(), Input::Direction),
        declared_row("main-align", props.main_align.to_string(), Input::Alignment),
        declared_row(
            "cross-align",
            props.cross_align.to_string(),
            Input::Alignment,
        ),
    ];
    if is_rect {
        out.push(declared_row(
            "background",
            props
                .background
                .map(colour_to_hex)
                .unwrap_or_else(|| "gradient".to_string()),
            if props.background.is_some() {
                Input::Colour
            } else {
                Input::ReadOnly
            },
        ));
        out.push(declared_row(
            "corner-radius",
            px_text(props.corner_radius),
            Input::Pixels,
        ));
    }
    if !is_rect || props.text_color.is_some() {
        out.push(declared_row(
            "text-color",
            props
                .text_color
                .map(colour_to_hex)
                .unwrap_or_default(),
            Input::Colour,
        ));
    }
    if !is_rect || props.font_size.is_some() {
        out.push(declared_row(
            "font-size",
            props.font_size.map(px_text).unwrap_or_default(),
            Input::Pixels,
        ));
    }
    out
}

fn declared_row(name: &'static str, value: String, input: Input) -> Declared {
    Declared { name, value, input }
}

/// The edit a typed value makes to `name`, or `None` when the text does not
/// parse for that property.
pub fn parse(name: &str, text: &str) -> Option<Edit> {
    let text = text.trim();
    Some(match name {
        "width" => {
            let size = parse_size(text)?;
            layout_edit(move |node| node.width = size.clone())
        }
        "height" => {
            let size = parse_size(text)?;
            layout_edit(move |node| node.height = size.clone())
        }
        "padding" => {
            let gaps = parse_gaps(text)?;
            layout_edit(move |node| node.padding = gaps.clone())
        }
        "margin" => {
            let gaps = parse_gaps(text)?;
            layout_edit(move |node| node.margin = gaps.clone())
        }
        "gap" => {
            let px = parse_px(text)?;
            layout_edit(move |node| node.spacing = Length::new(px))
        }
        "direction" => {
            let direction = parse_direction(text)?;
            layout_edit(move |node| node.direction = direction.clone())
        }
        "main-align" => {
            let align = parse_alignment(text)?;
            layout_edit(move |node| node.main_alignment = align.clone())
        }
        "cross-align" => {
            let align = parse_alignment(text)?;
            layout_edit(move |node| node.cross_alignment = align.clone())
        }
        "background" => background_edit(text)?,
        "corner-radius" => radius_edit(text)?,
        "text-color" => text_color_edit(text)?,
        "font-size" => font_size_edit(text)?,
        _ => return None,
    })
}

/// Commits `text` as the value of `name` on `node`. `original` is the text the
/// property had before, shown as "was" beside the override. Returns whether the
/// text parsed.
pub fn commit(window: WindowId, node: u64, name: &'static str, text: &str, original: &str) -> bool {
    let Some(edit) = parse(name, text) else {
        return false;
    };
    super::edit(
        window,
        node,
        name,
        text.trim().to_string(),
        edit,
        original.to_string(),
    );
    true
}

fn background_edit(text: &str) -> Option<Edit> {
    let color = colour_from_hex(text)?;
    Some(Rc::new(move |any: &mut dyn Any| {
        overrides::with_rect(any, |rect| rect.style.background = Fill::Color(color));
    }))
}

fn radius_edit(text: &str) -> Option<Edit> {
    let px = parse_px(text)?;
    Some(Rc::new(move |any: &mut dyn Any| {
        overrides::with_rect(any, |rect| {
            rect.style.corner_radius = CornerRadius::new_all(px);
        });
    }))
}

fn text_color_edit(text: &str) -> Option<Edit> {
    let color = colour_from_hex(text)?;
    Some(text_edit(Some(color), None))
}

fn font_size_edit(text: &str) -> Option<Edit> {
    let px = parse_px(text)?;
    Some(text_edit(None, Some(px)))
}

/// Overrides the layout of a rect or label.
fn layout_edit(set: impl Fn(&mut LayoutNode) + 'static) -> Edit {
    Rc::new(move |any: &mut dyn Any| {
        overrides::with_rect(any, |rect| set(&mut rect.layout.layout));
        overrides::with_label(any, |label| set(&mut label.layout.layout));
    })
}

/// Overrides the text colour and/or size of a rect or label.
fn text_edit(color: Option<Color>, size: Option<f32>) -> Edit {
    Rc::new(move |any: &mut dyn Any| {
        overrides::with_rect(any, |rect| {
            if let Some(color) = color {
                rect.text_style_data.color = Some(Fill::Color(color));
            }
            if let Some(size) = size {
                rect.text_style_data.font_size = Some(FontSize::from(size));
            }
        });
        overrides::with_label(any, |label| {
            if let Some(color) = color {
                label.text_style_data.color = Some(Fill::Color(color));
            }
            if let Some(size) = size {
                label.text_style_data.font_size = Some(FontSize::from(size));
            }
        });
    })
}

/// A number as the editor shows it.
pub fn px_text(value: f32) -> String {
    format!("{value}")
}

/// Four sides in one value when they match, otherwise `top right bottom left`.
pub fn gaps_text(sides: &[f32; 4]) -> String {
    if sides.iter().all(|side| *side == sides[0]) {
        px_text(sides[0])
    } else {
        sides
            .iter()
            .map(|side| px_text(*side))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A non-negative pixel count, with an optional `px` suffix.
fn parse_px(text: &str) -> Option<f32> {
    let value: f32 = text.trim().trim_end_matches("px").trim().parse().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// `auto`, `fill`, a percentage such as `50%`, or pixels.
fn parse_size(text: &str) -> Option<Size> {
    match text {
        "auto" => Some(Size::auto()),
        "fill" => Some(Size::fill()),
        _ => match text.strip_suffix('%') {
            Some(percent) => {
                let value: f32 = percent.trim().parse().ok()?;
                (value.is_finite() && value >= 0.0).then(|| Size::percent(value))
            }
            None => parse_px(text).map(Size::px),
        },
    }
}

/// One value for all sides, two for vertical and horizontal, or four for top,
/// right, bottom and left.
fn parse_gaps(text: &str) -> Option<Gaps> {
    let values = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .map(parse_px)
        .collect::<Option<Vec<f32>>>()?;
    match values[..] {
        [all] => Some(Gaps::new_all(all)),
        [vertical, horizontal] => Some(Gaps::new(vertical, horizontal, vertical, horizontal)),
        [top, right, bottom, left] => Some(Gaps::new(top, right, bottom, left)),
        _ => None,
    }
}

/// `vertical` (a column) or `horizontal` (a row).
fn parse_direction(text: &str) -> Option<Direction> {
    match text {
        "vertical" => Some(Direction::Vertical),
        "horizontal" => Some(Direction::Horizontal),
        _ => None,
    }
}

fn parse_alignment(text: &str) -> Option<Alignment> {
    match text {
        "start" => Some(Alignment::Start),
        "center" => Some(Alignment::Center),
        "end" => Some(Alignment::End),
        "space-between" => Some(Alignment::SpaceBetween),
        "space-evenly" => Some(Alignment::SpaceEvenly),
        "space-around" => Some(Alignment::SpaceAround),
        _ => None,
    }
}
