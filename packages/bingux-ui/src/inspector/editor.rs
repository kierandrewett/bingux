//! The properties of a node in bingux-ui's own terms.
//!
//! Freya elements carry resolved numbers and colours; bingux-ui code writes
//! tokens: `gap(S3)`, `p(S4)`, `w_full()`, `rounded(Radius::Card)`,
//! `bg(Paint::Card)`. This module reads an element back into the utilities
//! that would build it ([`defs`]), says for each one which token it is and
//! what the token stands for, offers the set of tokens to pick from, and turns
//! a pick into a live edit of the element ([`parse`], [`commit`]). A value
//! that is on no token is kept as a raw value and marked so.

use std::any::Any;
use std::borrow::Cow;
use std::rc::Rc;

use freya::prelude::*;
use freya::torin::geometry::Length;
use freya::torin::prelude::{Alignment, Direction, Gaps, Size};
use freya::winit::window::WindowId;
use freya_core::accessibility::focusable::Focusable;

use super::overrides::{self, Edit};
use super::snapshot::{Kind, Node, Props};
use crate::style::catalogue as tokens_of;
use crate::style::{
    Paint, SizeClass, colour_from_hex, colour_to_hex, elevation_shadow, line_border,
};
use crate::theme::Tokens;
use crate::typography::mono_family;

/// The option key of a value that is on no token.
pub const RAW: &str = "raw";
/// The option key shown when the sides of a spacing differ.
pub const MIXED: &str = "mixed";
/// The option key shown for a value no token in the list matches.
pub const CUSTOM: &str = "custom";
/// The option key shown when a text property is inherited.
pub const INHERIT: &str = "inherit";

/// How the Rust API groups a node's properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Layout,
    Spacing,
    Size,
    Paint,
    Text,
    Accessibility,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::Layout,
        Group::Spacing,
        Group::Size,
        Group::Paint,
        Group::Text,
        Group::Accessibility,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::Layout => "Layout",
            Group::Spacing => "Spacing",
            Group::Size => "Size",
            Group::Paint => "Paint",
            Group::Text => "Text style",
            Group::Accessibility => "Accessibility",
        }
    }
}

/// What edits a property.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Editor {
    /// A dropdown of tokens, and a raw field when the value is on none.
    Choice,
    /// A dropdown of colour tokens, and a swatch with a hex field and picker
    /// when the colour is on none.
    Colour,
    /// Free text.
    Text,
    /// A number the arrow keys step.
    Number { step: f64 },
    /// On or off.
    Flag,
    /// Shown, not editable.
    ReadOnly,
}

/// One token a property can take.
#[derive(Clone, Debug, PartialEq)]
pub struct Opt {
    pub key: String,
    pub label: String,
    pub colour: Option<Color>,
}

impl Opt {
    fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            colour: None,
        }
    }
}

/// One property of a node, as the Properties pane shows and edits it.
#[derive(Clone, Debug, PartialEq)]
pub struct Def {
    /// The utility's name, and the override's key: `gap`, `p`, `bg`.
    pub key: &'static str,
    pub group: Group,
    /// The call the value stands for: `gap(S3) = 12 px`.
    pub hint: String,
    pub editor: Editor,
    pub options: Vec<Opt>,
    /// The option in force, or [`RAW`] when the value is on no token.
    pub selected: String,
    /// The raw text, or the value of a text, number or flag property.
    pub text: String,
    /// The current colour of a colour property.
    pub swatch: Option<Color>,
    /// The utility as source, when the value is on a token and not the
    /// default: `gap(S3)`.
    pub source: Option<String>,
}

impl Def {
    /// The text the property reads back as: the token's key, or the raw text.
    pub fn current(&self) -> String {
        match self.editor {
            Editor::Choice | Editor::Colour if self.selected != RAW => self.selected.clone(),
            _ => self.text.clone(),
        }
    }
}

/// A number as the editor shows it: whole when whole.
pub fn number_text(value: f32) -> String {
    if (value - value.round()).abs() < 0.005 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

fn hex_label(colour: Color) -> String {
    colour_to_hex(colour)
}

/// Every property of the node, in the order the pane lists them. `class` is the
/// density the application window renders at, so sizes read as the tokens they
/// are there; `tokens` are its colours.
pub fn defs(node: &Node, class: SizeClass, tokens: &Tokens) -> Vec<Def> {
    let mut out = Vec::new();
    if let Some(props) = node.props.as_ref() {
        let is_rect = node.kind == Kind::Rect;
        if is_rect {
            out.extend(layout_defs(props, class));
        }
        out.extend(spacing_defs(props, class));
        out.extend(size_defs(props, class));
        out.extend(paint_defs(props, is_rect, class, tokens));
        out.extend(text_defs(node, props, class, tokens));
    }
    out.extend(accessibility_defs(node));
    out
}

fn layout_defs(props: &Props, class: SizeClass) -> Vec<Def> {
    let direction = match props.direction {
        "horizontal" => "row",
        _ => "col",
    };
    let mut out = vec![Def {
        key: "flex",
        group: Group::Layout,
        hint: format!("flex_{direction}()"),
        editor: Editor::Choice,
        options: vec![Opt::new("col", "flex_col()"), Opt::new("row", "flex_row()")],
        selected: direction.to_string(),
        text: String::new(),
        swatch: None,
        source: (direction != "col").then(|| format!("flex_{direction}()")),
    }];
    out.push(align_def("items", props.cross_align));
    out.push(align_def("justify", props.main_align));
    out.push(space_def(
        "gap",
        Group::Layout,
        "gap",
        Some(props.gap),
        class,
    ));
    out.push(Def {
        key: "clip",
        group: Group::Layout,
        hint: if props.clip {
            "clip()".to_string()
        } else {
            "children may paint outside".to_string()
        },
        editor: Editor::Flag,
        options: Vec::new(),
        selected: String::new(),
        text: props.clip.to_string(),
        swatch: None,
        source: props.clip.then(|| "clip()".to_string()),
    });
    out
}

fn align_def(key: &'static str, value: &'static str) -> Def {
    let name = match value {
        "start" => Some("Start"),
        "center" => Some("Center"),
        "end" => Some("End"),
        "space-between" => Some("Between"),
        "space-around" => Some("Around"),
        _ => None,
    };
    let mut options: Vec<Opt> = tokens_of::ALIGNS
        .iter()
        .map(|(name, _)| Opt::new(*name, name.to_string()))
        .collect();
    options.push(Opt::new(RAW, "SpaceEvenly (raw)"));
    Def {
        key,
        group: Group::Layout,
        hint: match name {
            Some(name) => format!("{key}(Align::{name})"),
            None => format!("{key} = {value} (not a token)"),
        },
        editor: Editor::Choice,
        options,
        selected: name.map_or(RAW.to_string(), str::to_string),
        text: value.to_string(),
        swatch: None,
        source: name
            .filter(|name| *name != "Start")
            .map(|name| format!("{key}(Align::{name})")),
    }
}

/// A spacing or gap row. `px` is `None` when the sides it covers differ.
fn space_def(
    key: &'static str,
    group: Group,
    call: &str,
    px: Option<f32>,
    class: SizeClass,
) -> Def {
    let mut options: Vec<Opt> = tokens_of::SPACES
        .iter()
        .map(|(name, space)| {
            Opt::new(
                *name,
                format!("{name} · {} px", number_text(space.px_in(class))),
            )
        })
        .collect();
    options.push(Opt::new(RAW, "Raw value"));
    let Some(px) = px else {
        options.insert(0, Opt::new(MIXED, "sides differ"));
        return Def {
            key,
            group,
            hint: format!("{call}: the sides differ"),
            editor: Editor::Choice,
            options,
            selected: MIXED.to_string(),
            text: String::new(),
            swatch: None,
            source: None,
        };
    };
    let name = tokens_of::space_name(px, class);
    Def {
        key,
        group,
        hint: match name {
            Some(name) => format!("{call}({name}) = {} px", number_text(px)),
            None => format!("{call} = {} px (not a token)", number_text(px)),
        },
        editor: Editor::Choice,
        options,
        selected: name.map_or(RAW.to_string(), str::to_string),
        text: number_text(px),
        swatch: None,
        source: match name {
            Some("S0") => None,
            Some(name) => Some(format!("{call}({name})")),
            None => None,
        },
    }
}

fn same_side(a: f32, b: f32) -> Option<f32> {
    ((a - b).abs() < 0.01).then_some(a)
}

fn spacing_defs(props: &Props, class: SizeClass) -> Vec<Def> {
    let mut out = Vec::new();
    for (all, x, y, call, sides) in [
        ("p", "px", "py", "p", props.padding),
        ("m", "mx", "my", "m", props.margin),
    ] {
        let [top, right, bottom, left] = sides;
        let horizontal = same_side(left, right);
        let vertical = same_side(top, bottom);
        let every = horizontal.zip(vertical).and_then(|(h, v)| same_side(h, v));
        let mut all_row = space_def(all, Group::Spacing, call, every, class);
        let mut x_row = space_def(x, Group::Spacing, x, horizontal, class);
        let mut y_row = space_def(y, Group::Spacing, y, vertical, class);
        // A uniform spacing is written once as `p`, and a mixed one by its
        // pairs; the rows that would only restate it carry no source.
        if every.is_some() {
            x_row.source = None;
            y_row.source = None;
        } else {
            all_row.source = None;
        }
        out.extend([all_row, x_row, y_row]);
    }
    out
}

fn extent_def(key: &'static str, call: &str, text: &str, class: SizeClass, limit: bool) -> Def {
    let none = if limit { "none" } else { "w_auto()" };
    let mut options = vec![
        Opt::new(
            "auto",
            if limit {
                "no limit".to_string()
            } else {
                format!("{call}_auto()")
            },
        ),
        Opt::new("full", format!("{call}_full()")),
        Opt::new("flex", format!("{call}_flex()")),
    ];
    if limit {
        options.remove(2);
        options.remove(1);
    }
    for (name, dim) in tokens_of::dims(class) {
        options.push(Opt::new(
            name,
            format!("{name} · {} px", number_text(dim.px_in(class))),
        ));
    }
    options.push(Opt::new(RAW, "Raw value"));
    let (selected, hint, source) = match text {
        "auto" => (
            "auto".to_string(),
            if limit {
                format!("{call}: no limit")
            } else {
                format!("{call}_auto()")
            },
            None,
        ),
        "fill" if !limit => (
            "full".to_string(),
            format!("{call}_full()"),
            Some(format!("{call}_full()")),
        ),
        "flex 1" if !limit => (
            "flex".to_string(),
            format!("{call}_flex()"),
            Some(format!("{call}_flex()")),
        ),
        other => match other.parse::<f32>() {
            Ok(px) => match tokens_of::dim_name(px, class) {
                Some(name) => {
                    let also: Vec<&str> = tokens_of::dim_names(px, class)
                        .into_iter()
                        .skip(1)
                        .take(3)
                        .collect();
                    let tail = if also.is_empty() {
                        String::new()
                    } else {
                        format!(" · also {}", also.join(", "))
                    };
                    (
                        name.to_string(),
                        format!("{call}(Dim::{name}) = {} px{tail}", number_text(px)),
                        Some(format!("{call}(Dim::{name})")),
                    )
                }
                None => (
                    RAW.to_string(),
                    format!("{call} = {} px (not a token)", number_text(px)),
                    None,
                ),
            },
            Err(_) => (
                RAW.to_string(),
                format!("{call} = {other} (not a token)"),
                None,
            ),
        },
    };
    let _ = none;
    Def {
        key,
        group: Group::Size,
        hint,
        editor: Editor::Choice,
        options,
        selected,
        text: text.to_string(),
        swatch: None,
        source,
    }
}

fn size_defs(props: &Props, class: SizeClass) -> Vec<Def> {
    vec![
        extent_def("w", "w", &props.width, class, false),
        extent_def("h", "h", &props.height, class, false),
        extent_def("min_w", "min_w", &props.min_width, class, true),
        extent_def("min_h", "min_h", &props.min_height, class, true),
        extent_def("max_w", "max_w", &props.max_width, class, true),
        extent_def("max_h", "max_h", &props.max_height, class, true),
    ]
}

/// A colour row: the colour tokens with a chip each, and a custom colour.
/// `colour` is `None` for a text colour the element inherits.
fn paint_def(
    key: &'static str,
    group: Group,
    call: &str,
    colour: Option<Color>,
    tokens: &Tokens,
) -> Def {
    let mut options: Vec<Opt> = Vec::new();
    if colour.is_none() {
        options.push(Opt {
            key: INHERIT.to_string(),
            label: "inherited".to_string(),
            colour: Some(Paint::Transparent.color(tokens)),
        });
    }
    for (name, paint) in tokens_of::PAINTS {
        options.push(Opt {
            key: name.to_string(),
            label: name.to_string(),
            colour: Some(paint.color(tokens)),
        });
    }
    options.push(Opt {
        key: RAW.to_string(),
        label: "Custom colour".to_string(),
        colour: colour.or(Some(Paint::Transparent.color(tokens))),
    });
    let name = colour.and_then(|colour| tokens_of::paint_name(colour, tokens));
    let (selected, hint) = match (colour, name) {
        (None, _) => (INHERIT.to_string(), format!("{call}: inherited")),
        (Some(colour), Some(name)) => (
            name.to_string(),
            format!("{call}(Paint::{name}) = {}", hex_label(colour)),
        ),
        (Some(colour), None) => (
            RAW.to_string(),
            format!("{call} = {} (not a token)", hex_label(colour)),
        ),
    };
    Def {
        key,
        group,
        hint,
        editor: Editor::Colour,
        options,
        selected,
        text: colour.map(hex_label).unwrap_or_default(),
        swatch: colour,
        source: name
            .filter(|name| *name != "Transparent")
            .map(|name| format!("{call}(Paint::{name})")),
    }
}

fn paint_defs(props: &Props, is_rect: bool, class: SizeClass, tokens: &Tokens) -> Vec<Def> {
    let mut out = Vec::new();
    if !is_rect {
        out.push(opacity_def(props));
        return out;
    }
    match props.background {
        Some(colour) => out.push(paint_def("bg", Group::Paint, "bg", Some(colour), tokens)),
        None => out.push(Def {
            key: "bg",
            group: Group::Paint,
            hint: "bg: a gradient or shader, not a token".to_string(),
            editor: Editor::ReadOnly,
            options: Vec::new(),
            selected: CUSTOM.to_string(),
            text: "gradient".to_string(),
            swatch: None,
            source: None,
        }),
    }
    let radius = props.corner_radius;
    let name = if radius == 0.0 {
        Some("none")
    } else {
        tokens_of::radius_name(radius)
    };
    let mut options: Vec<Opt> = vec![Opt::new("none", "none · 0 px")];
    options.extend(tokens_of::RADII.iter().map(|(name, radius)| {
        Opt::new(
            *name,
            format!("{name} · {} px", number_text(radius.px().min(99.0))),
        )
    }));
    options.push(Opt::new(RAW, "Raw value"));
    let _ = class;
    out.push(Def {
        key: "rounded",
        group: Group::Paint,
        hint: match name {
            Some("none") => "no rounding".to_string(),
            Some(name) => format!("rounded(Radius::{name}) = {} px", number_text(radius)),
            None => format!("rounded = {} px (not a token)", number_text(radius)),
        },
        editor: Editor::Choice,
        options,
        selected: name.map_or(RAW.to_string(), str::to_string),
        text: number_text(radius),
        swatch: None,
        source: name
            .filter(|name| *name != "none")
            .map(|name| format!("rounded(Radius::{name})")),
    });

    let shadow = match props.shadows.as_slice() {
        [] => Some("none"),
        [one] => tokens_of::ELEVATIONS
            .iter()
            .find(|(_, elevation)| *one == elevation_shadow(*elevation))
            .map(|(name, _)| *name),
        _ => None,
    };
    let mut options = vec![Opt::new("none", "no shadow")];
    options.extend(
        tokens_of::ELEVATIONS
            .iter()
            .map(|(name, _)| Opt::new(*name, name.to_string())),
    );
    if shadow.is_none() {
        options.push(Opt::new(CUSTOM, "custom shadow"));
    }
    out.push(Def {
        key: "shadow",
        group: Group::Paint,
        hint: match shadow {
            Some("none") => "no shadow".to_string(),
            Some(name) => format!("shadow(Elevation::{name})"),
            None => "a shadow that is not an Elevation".to_string(),
        },
        editor: Editor::Choice,
        options,
        selected: shadow.unwrap_or(CUSTOM).to_string(),
        text: String::new(),
        swatch: None,
        source: shadow
            .filter(|name| *name != "none")
            .map(|name| format!("shadow(Elevation::{name})")),
    });

    let border = match props.borders.as_slice() {
        [] => Some("none"),
        [one] => tokens_of::LINES
            .iter()
            .find(|(_, line)| *one == line_border(tokens, *line))
            .map(|(name, _)| *name),
        _ => None,
    };
    let mut options = vec![Opt::new("none", "no border")];
    options.extend(
        tokens_of::LINES
            .iter()
            .map(|(name, _)| Opt::new(*name, name.to_string())),
    );
    if border.is_none() {
        options.push(Opt::new(CUSTOM, "custom border"));
    }
    out.push(Def {
        key: "border",
        group: Group::Paint,
        hint: match border {
            Some("none") => "no border".to_string(),
            Some(name) => format!("border(Line::{name})"),
            None => "a border that is not a Line".to_string(),
        },
        editor: Editor::Choice,
        options,
        selected: border.unwrap_or(CUSTOM).to_string(),
        text: String::new(),
        swatch: None,
        source: border
            .filter(|name| *name != "none")
            .map(|name| format!("border(Line::{name})")),
    });
    out.push(opacity_def(props));
    out
}

fn opacity_def(props: &Props) -> Def {
    let value = props.opacity.unwrap_or(1.0);
    Def {
        key: "opacity",
        group: Group::Paint,
        hint: format!("opacity({})", number_text(value)),
        editor: Editor::Number { step: 0.1 },
        options: Vec::new(),
        selected: String::new(),
        text: number_text(value),
        swatch: None,
        source: props
            .opacity
            .map(|value| format!("opacity({})", number_text(value))),
    }
}

fn text_defs(node: &Node, props: &Props, class: SizeClass, tokens: &Tokens) -> Vec<Def> {
    let mut out = Vec::new();
    if matches!(node.kind, Kind::Label | Kind::Paragraph) {
        let text = node.text.clone().unwrap_or_default();
        out.push(Def {
            key: "text",
            group: Group::Text,
            hint: format!("{:?}", text),
            editor: Editor::Text,
            options: Vec::new(),
            selected: String::new(),
            text,
            swatch: None,
            source: None,
        });
    }
    let weight = props.font_weight.unwrap_or(tokens_of::REGULAR);
    let style = props
        .font_size
        .and_then(|size| tokens_of::text_style_name(size, weight, props.mono, class));
    let mut options: Vec<Opt> = Vec::new();
    match (props.font_size, style) {
        (None, _) => options.push(Opt::new(INHERIT, "inherited")),
        (Some(size), None) => options.push(Opt::new(
            CUSTOM,
            format!("custom · {} px", number_text(size)),
        )),
        _ => {}
    }
    options.extend(tokens_of::TEXT_STYLES.iter().map(|(name, style)| {
        Opt::new(
            *name,
            format!("{name} · {} px", number_text(style.size.px_in(class))),
        )
    }));
    out.push(Def {
        key: "font",
        group: Group::Text,
        hint: match (props.font_size, style) {
            (None, _) => "font: inherited".to_string(),
            (Some(size), Some(name)) => {
                format!(
                    "font({name}) = {} px · {}",
                    number_text(size),
                    weight_text(weight)
                )
            }
            (Some(size), None) => format!(
                "font = {} px · {} (not a TextStyle)",
                number_text(size),
                weight_text(weight)
            ),
        },
        editor: Editor::Choice,
        options,
        selected: match (props.font_size, style) {
            (None, _) => INHERIT.to_string(),
            (Some(_), Some(name)) => name.to_string(),
            (Some(_), None) => CUSTOM.to_string(),
        },
        text: String::new(),
        swatch: None,
        source: style.map(|name| format!("font({name})")),
    });
    out.push(paint_def(
        "ink",
        Group::Text,
        "ink",
        props.text_color,
        tokens,
    ));
    out
}

fn weight_text(weight: FontWeight) -> String {
    tokens_of::weight_text(weight)
}

fn accessibility_defs(node: &Node) -> Vec<Def> {
    let role = if node.a11y.role.is_empty() {
        "Unknown".to_string()
    } else {
        node.a11y.role.clone()
    };
    let name = node.a11y.name.clone().unwrap_or_default();
    let focusable = node.a11y.focusable.unwrap_or(false);
    vec![
        Def {
            key: "role",
            group: Group::Accessibility,
            hint: format!("a11y_role(AccessibilityRole::{role})"),
            editor: Editor::ReadOnly,
            options: Vec::new(),
            selected: CUSTOM.to_string(),
            text: role,
            swatch: None,
            source: None,
        },
        Def {
            key: "name",
            group: Group::Accessibility,
            hint: if name.is_empty() {
                "no name: a screen reader finds the text".to_string()
            } else {
                format!("a11y_label({name:?})")
            },
            editor: Editor::Text,
            options: Vec::new(),
            selected: String::new(),
            text: name,
            swatch: None,
            source: None,
        },
        Def {
            key: "focusable",
            group: Group::Accessibility,
            hint: format!("a11y_focusable({focusable})"),
            editor: Editor::Flag,
            options: Vec::new(),
            selected: String::new(),
            text: focusable.to_string(),
            swatch: None,
            source: None,
        },
    ]
}

/// The node as the bingux-ui code that builds it: the published call of a
/// component, or the utilities of a box, or the text style of a text.
pub fn rust_source(node: &Node, class: SizeClass, tokens: &Tokens) -> String {
    if let Some(call) = &node.a11y.published {
        return call.source();
    }
    let defs = defs(node, class, tokens);
    let style = defs
        .iter()
        .find(|def| def.key == "font")
        .and_then(|def| def.source.clone())
        .and_then(|source| {
            source
                .strip_prefix("font(")
                .and_then(|rest| rest.strip_suffix(')'))
                .map(str::to_string)
        })
        .unwrap_or_else(|| "BODY".to_string());
    match node.kind {
        Kind::Label => format!(
            "{style}.label(&tokens, {:?})",
            node.text.clone().unwrap_or_default()
        ),
        Kind::Paragraph => format!(
            "{style}.paragraph(&tokens, {:?})",
            node.text.clone().unwrap_or_default()
        ),
        _ => {
            let utils: Vec<String> = defs
                .iter()
                .filter(|def| def.group != Group::Accessibility && def.key != "ink")
                .filter_map(|def| def.source.clone())
                .collect();
            let mut out = String::from("rect()");
            if let Some(name) = node.a11y.component.as_deref() {
                out.push_str(&format!(".component({name:?})"));
            }
            if utils.is_empty() {
                return out;
            }
            out.push_str(".tw(\n    &tokens,\n    &[\n");
            for util in utils {
                out.push_str(&format!("        {util},\n"));
            }
            out.push_str("    ],\n)");
            out
        }
    }
}

/// The edit that setting `key` to `text` makes, or `None` when the text is
/// not a value of that property (including the display-only options such as
/// "sides differ").
pub fn parse(key: &str, text: &str, class: SizeClass, tokens: &Tokens) -> Option<Edit> {
    let text = text.trim();
    Some(match key {
        "flex" => {
            let direction = match text {
                "col" | "vertical" => Direction::Vertical,
                "row" | "horizontal" => Direction::Horizontal,
                _ => return None,
            };
            layout_edit(move |node| node.direction = direction.clone())
        }
        "items" => {
            let alignment = parse_alignment(text)?;
            layout_edit(move |node| node.cross_alignment = alignment.clone())
        }
        "justify" => {
            let alignment = parse_alignment(text)?;
            layout_edit(move |node| node.main_alignment = alignment.clone())
        }
        "gap" => {
            let px = parse_space(text, class)?;
            layout_edit(move |node| node.spacing = Length::new(px))
        }
        "p" => padding_edit(text, class, [true; 4], false)?,
        "px" => padding_edit(text, class, [false, true, false, true], false)?,
        "py" => padding_edit(text, class, [true, false, true, false], false)?,
        "m" => padding_edit(text, class, [true; 4], true)?,
        "mx" => padding_edit(text, class, [false, true, false, true], true)?,
        "my" => padding_edit(text, class, [true, false, true, false], true)?,
        "w" => {
            let size = parse_extent(text, class, false)?;
            layout_edit(move |node| node.width = size.clone())
        }
        "h" => {
            let size = parse_extent(text, class, false)?;
            layout_edit(move |node| node.height = size.clone())
        }
        "min_w" => {
            let size = parse_extent(text, class, true)?;
            layout_edit(move |node| node.minimum_width = size.clone())
        }
        "min_h" => {
            let size = parse_extent(text, class, true)?;
            layout_edit(move |node| node.minimum_height = size.clone())
        }
        "max_w" => {
            let size = parse_extent(text, class, true)?;
            layout_edit(move |node| node.maximum_width = size.clone())
        }
        "max_h" => {
            let size = parse_extent(text, class, true)?;
            layout_edit(move |node| node.maximum_height = size.clone())
        }
        "clip" => {
            let clip = parse_flag(text)?;
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_effect(any, |effect| {
                    effect.overflow = if clip { Overflow::Clip } else { Overflow::None };
                });
            })
        }
        "bg" => {
            let colour = parse_paint(text, tokens)?;
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_rect(any, |rect| rect.style.background = Fill::Color(colour));
            })
        }
        "rounded" => {
            let px = match (text, tokens_of::radius(text)) {
                ("none", _) => 0.0,
                (_, Some(radius)) => radius.px(),
                _ => parse_px(text)?,
            };
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_rect(any, |rect| {
                    rect.style.corner_radius = CornerRadius::new_all(px);
                });
            })
        }
        "shadow" => {
            let shadows = match text {
                "none" => Vec::new(),
                name => vec![elevation_shadow(tokens_of::elevation(name)?)],
            };
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_rect(any, |rect| rect.style.shadows = shadows.clone());
            })
        }
        "border" => {
            let borders = match text {
                "none" => Vec::new(),
                name => vec![line_border(tokens, tokens_of::line(name)?)],
            };
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_rect(any, |rect| rect.style.borders = borders.clone());
            })
        }
        "opacity" => {
            let value: f32 = text.parse().ok()?;
            let value = value.clamp(0.0, 1.0);
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_effect(any, |effect| effect.opacity = Some(value));
            })
        }
        "font" => {
            let style = tokens_of::text_style(text)?;
            let size = style.size.px_in(class);
            let ink = style.ink.color(tokens);
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_text_style(any, |data| {
                    data.font_size = Some(FontSize::from(size));
                    data.font_weight = Some(style.weight);
                    data.color = Some(Fill::Color(ink));
                    data.font_families
                        .retain(|family| family.as_ref() != mono_family());
                    if style.mono {
                        data.font_families
                            .insert(0, Cow::Owned(mono_family().to_string()));
                    }
                });
            })
        }
        "ink" => {
            let colour = parse_paint(text, tokens)?;
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_text_style(any, |data| data.color = Some(Fill::Color(colour)));
            })
        }
        "text" => {
            let text = text.to_string();
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_label(any, |label| label.text = Cow::Owned(text.clone()));
                overrides::with_paragraph(any, |paragraph| {
                    paragraph.spans.truncate(1);
                    match paragraph.spans.first_mut() {
                        Some(span) => span.text = Cow::Owned(text.clone()),
                        None => paragraph.spans.push(Span::new(text.clone())),
                    }
                });
            })
        }
        "name" => {
            let name = text.to_string();
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_accessibility(any, |data| {
                    if name.is_empty() {
                        data.builder.clear_label();
                    } else {
                        data.builder.set_label(name.clone());
                    }
                });
            })
        }
        "focusable" => {
            let on = parse_flag(text)?;
            Rc::new(move |any: &mut dyn Any| {
                overrides::with_accessibility(any, |data| {
                    data.a11y_focusable = if on {
                        Focusable::Enabled
                    } else {
                        Focusable::Disabled
                    };
                });
            })
        }
        _ => return None,
    })
}

/// Commits `text` as the value of `key` on `node`. `original` is the text the
/// property had before, kept with the override so a reset restores it. Returns
/// whether the text was a value of the property.
pub fn commit(
    window: WindowId,
    node: u64,
    key: &'static str,
    text: &str,
    original: &str,
    class: SizeClass,
    tokens: &Tokens,
) -> bool {
    let Some(edit) = parse(key, text, class, tokens) else {
        return false;
    };
    super::edit(window, node, key, edit, original.to_string());
    true
}

/// Overrides the layout of a rect, label or paragraph.
fn layout_edit(set: impl Fn(&mut freya::torin::node::Node) + 'static) -> Edit {
    Rc::new(move |any: &mut dyn Any| overrides::with_layout(any, |node| set(node)))
}

/// Sets the sides of padding or margin that `sides` (top, right, bottom,
/// left) name.
fn padding_edit(text: &str, class: SizeClass, sides: [bool; 4], margin: bool) -> Option<Edit> {
    let px = parse_space(text, class)?;
    Some(layout_edit(move |node| {
        let target = if margin {
            &mut node.margin
        } else {
            &mut node.padding
        };
        let current = [target.top(), target.right(), target.bottom(), target.left()];
        let pick = |index: usize| if sides[index] { px } else { current[index] };
        *target = Gaps::new(pick(0), pick(1), pick(2), pick(3));
    }))
}

/// A non-negative pixel count, with an optional `px` suffix.
fn parse_px(text: &str) -> Option<f32> {
    let value: f32 = text.trim().trim_end_matches("px").trim().parse().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// A space token by name, or a pixel count.
fn parse_space(text: &str, class: SizeClass) -> Option<f32> {
    match tokens_of::space(text) {
        Some(space) => Some(space.px_in(class)),
        None => parse_px(text),
    }
}

/// `auto`, `full`, `flex`, a `Dim` by name, a percentage such as `50%`, or
/// pixels. A limit (`min_w`, `max_w`) takes `auto` for none.
fn parse_extent(text: &str, class: SizeClass, limit: bool) -> Option<Size> {
    match text {
        "auto" => Some(Size::auto()),
        "full" | "fill" if !limit => Some(Size::fill()),
        "flex" if !limit => Some(Size::flex(1.0)),
        _ => {
            if let Some(dim) = tokens_of::dim(text) {
                return Some(Size::px(dim.px_in(class)));
            }
            match text.strip_suffix('%') {
                Some(percent) => {
                    let value: f32 = percent.trim().parse().ok()?;
                    (value.is_finite() && value >= 0.0).then(|| Size::percent(value))
                }
                None => parse_px(text).map(Size::px),
            }
        }
    }
}

fn parse_alignment(text: &str) -> Option<Alignment> {
    if let Some(align) = tokens_of::align(text) {
        return Some(Alignment::from(align));
    }
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

fn parse_flag(text: &str) -> Option<bool> {
    match text {
        "true" | "on" => Some(true),
        "false" | "off" => Some(false),
        _ => None,
    }
}

/// A colour token by name, or `#rrggbb` / `#rrggbbaa`.
fn parse_paint(text: &str, tokens: &Tokens) -> Option<Color> {
    match tokens_of::paint(text) {
        Some(paint) => Some(paint.color(tokens)),
        None => colour_from_hex(text),
    }
}
