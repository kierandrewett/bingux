//! The Computed tab's model: what the layout engine actually used for a node,
//! beside what the node declared.
//!
//! Every row is a *used* value read from the layout results of the target
//! window ([`Used`](super::snapshot::Used), taken when the snapshot was
//! captured): the border box the layout measured, the content box inside the
//! padding, the margin it applied, the text style after inheritance, the
//! folded opacity, the layer. `auto`, `fill`, a percentage and a flex share
//! are numbers here. When the declared value differs from the used one (and
//! only then) the row carries it as a `declared` chip, as Chrome's Computed
//! pane shows what the Styles pane said.
//!
//! A value the engine does not expose is [`Value::Unknown`]: the tab draws an
//! em dash with a faint marker. Nothing is defaulted or guessed.

use freya::prelude::Color;

use super::editor::number_text;
use super::snapshot::{Kind, Node, Props, Snapshot, Used};
use crate::style::colour_to_hex;

/// A used value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Colour(Color),
    /// The engine does not expose it for this node.
    Unknown,
}

/// One row: a name, its used value and, when the declared value differs, that.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: &'static str,
    pub value: Value,
    pub declared: Option<String>,
}

/// A titled run of rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub title: &'static str,
    pub entries: Vec<Entry>,
}

fn entry(name: &'static str, value: impl Into<String>) -> Entry {
    Entry {
        name,
        value: Value::Text(value.into()),
        declared: None,
    }
}

fn unknown(name: &'static str) -> Entry {
    Entry {
        name,
        value: Value::Unknown,
        declared: None,
    }
}

impl Entry {
    fn declared(mut self, declared: Option<String>) -> Self {
        self.declared = declared;
        self
    }
}

/// A number as a CSS-like length: `12`, `12.5`.
fn px(value: f32) -> String {
    number_text(value)
}

fn parse(text: &str) -> Option<f32> {
    text.trim().trim_end_matches("px").trim().parse().ok()
}

/// The declared value as a chip, unless it is the number that was used. A
/// declared `128` on a box that came out 128 wide is no news; `fill`, `auto`,
/// `50%` or `flex 1` are, and so is a `128` the layout could not honour.
pub fn declared_chip(declared: &str, used: &str) -> Option<String> {
    let declared = declared.trim();
    if declared.is_empty() || declared == used {
        return None;
    }
    match (parse(declared), parse(used)) {
        (Some(a), Some(b)) if (a - b).abs() < 0.01 => None,
        _ => Some(declared.to_string()),
    }
}

/// Four sides as `top right bottom left`, in the short CSS form.
pub fn sides_text(sides: [f32; 4]) -> String {
    let [top, right, bottom, left] = sides.map(px);
    if top == right && right == bottom && bottom == left {
        top
    } else if top == bottom && right == left {
        format!("{top} {right}")
    } else {
        format!("{top} {right} {bottom} {left}")
    }
}

/// The declared sides as a chip, unless they equal the used ones.
pub fn sides_chip(declared: [f32; 4], used: [f32; 4]) -> Option<String> {
    let same = declared
        .iter()
        .zip(used)
        .all(|(declared, used)| (declared - used).abs() < 0.01);
    (!same).then(|| sides_text(declared))
}

/// What a declared minimum or maximum comes to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Constraint {
    /// Not set: the engine applies no limit.
    Unset,
    Px(f32),
    /// Set, but to something the engine does not resolve per node: `fill`, a
    /// flex share, or a percentage with no parent to take it of.
    Unresolved,
}

/// Resolves a declared min or max size: a pixel count is itself, a
/// percentage is that share of `basis` (the parent's content box on that
/// axis), `auto` is no limit, and anything else is unresolved.
pub fn resolve_constraint(declared: &str, basis: Option<f32>) -> Constraint {
    let declared = declared.trim();
    if declared == "auto" || declared.is_empty() {
        return Constraint::Unset;
    }
    if let Some(percent) = declared.strip_suffix('%').and_then(parse) {
        return match basis {
            Some(basis) => Constraint::Px(basis * percent / 100.0),
            None => Constraint::Unresolved,
        };
    }
    match parse(declared) {
        Some(value) => Constraint::Px(value),
        None => Constraint::Unresolved,
    }
}

fn constraint_entry(name: &'static str, declared: &str, basis: Option<f32>) -> Entry {
    match resolve_constraint(declared, basis) {
        Constraint::Unset => entry(name, "none"),
        Constraint::Px(value) => {
            let used = px(value);
            let chip = declared_chip(declared, &used);
            entry(name, used).declared(chip)
        }
        Constraint::Unresolved => Entry {
            name,
            value: Value::Unknown,
            declared: Some(declared.trim().to_string()),
        },
    }
}

/// A used length against the declared size that asked for it.
fn size_entry(name: &'static str, declared: Option<&str>, used: Option<f32>) -> Entry {
    let Some(used) = used else {
        return Entry {
            name,
            value: Value::Unknown,
            declared: declared.map(str::to_string),
        };
    };
    let text = px(used);
    let chip = declared.and_then(|declared| declared_chip(declared, &text));
    entry(name, text).declared(chip)
}

/// A used text value against what the node declared. A node that declares
/// nothing took the value from its ancestors or the default: `inherited`.
fn inherited(declared: Option<String>, used: &str) -> Option<String> {
    match declared {
        Some(declared) => declared_chip(&declared, used),
        None => Some("inherited".to_string()),
    }
}

/// The rows for `node`, grouped. `snapshot` gives the parent, whose content
/// box percentages resolve against and the node's offset in it.
pub fn groups(snapshot: &Snapshot, node: &Node) -> Vec<Group> {
    let used = &node.used;
    let parent = node.parent.and_then(|id| snapshot.node(id));
    let mut groups = vec![
        Group {
            title: "Element",
            entries: element_entries(node),
        },
        Group {
            title: "Box",
            entries: box_entries(node, used, parent),
        },
    ];
    if let Some(props) = &node.props {
        groups.push(Group {
            title: "Layout",
            entries: layout_entries(props, used),
        });
        if node.kind == Kind::Rect {
            groups.push(Group {
                title: "Paint",
                entries: paint_entries(props, used),
            });
        }
    }
    if used.text.is_some() {
        groups.push(Group {
            title: "Text",
            entries: text_entries(node),
        });
    }
    groups
}

fn element_entries(node: &Node) -> Vec<Entry> {
    let used = &node.used;
    let mut entries = vec![
        entry("element", node.title()),
        entry("layout role", node.layout_role()),
        entry("node id", node.id.to_string()),
        entry(
            "parent",
            node.parent.map_or("none".to_string(), |id| id.to_string()),
        ),
        entry("children", node.children.len().to_string()),
        entry("hidden", used.hidden.to_string()),
    ];
    entries.push(match used.layer {
        Some(layer) => entry("layer", layer.to_string()),
        None => unknown("layer"),
    });
    if let Some(text) = &node.text {
        entries.push(entry("text", text.clone()));
    }
    entries
}

fn box_entries(node: &Node, used: &Used, parent: Option<&Node>) -> Vec<Entry> {
    let props = node.props.as_ref();
    let basis_width = parent.and_then(|parent| parent.used.content.map(|area| area.width()));
    let basis_height = parent.and_then(|parent| parent.used.content.map(|area| area.height()));
    let mut entries = vec![
        size_entry(
            "width",
            props.map(|props| props.width.as_str()),
            used.border.map(|area| area.width()),
        ),
        size_entry(
            "height",
            props.map(|props| props.height.as_str()),
            used.border.map(|area| area.height()),
        ),
    ];
    entries.push(match used.content {
        Some(area) => entry(
            "content",
            format!("{} × {}", px(area.width()), px(area.height())),
        ),
        None => unknown("content"),
    });
    if let Some(props) = props {
        entries.push(constraint_entry("min width", &props.min_width, basis_width));
        entries.push(constraint_entry(
            "min height",
            &props.min_height,
            basis_height,
        ));
        entries.push(constraint_entry("max width", &props.max_width, basis_width));
        entries.push(constraint_entry(
            "max height",
            &props.max_height,
            basis_height,
        ));
    }
    entries.push(match used.padding {
        Some(padding) => entry("padding", sides_text(padding))
            .declared(props.and_then(|props| sides_chip(props.padding, padding))),
        None => unknown("padding"),
    });
    entries.push(if used.border.is_some() {
        entry("margin", sides_text(used.margin))
            .declared(props.and_then(|props| sides_chip(props.margin, used.margin)))
    } else {
        unknown("margin")
    });
    if let Some(props) = props {
        entries.push(entry("gap", px(props.gap)));
    }
    entries.push(match used.border {
        Some(area) => entry(
            "position",
            format!("{}, {}", px(area.min_x()), px(area.min_y())),
        ),
        None => unknown("position"),
    });
    // Where the layout put the box inside its parent's content: the margin
    // plus whatever the alignment added.
    entries.push(
        match (used.border, parent.and_then(|parent| parent.used.content)) {
            (Some(own), Some(inside)) => entry(
                "offset in parent",
                format!(
                    "{}, {}",
                    px(own.min_x() - inside.min_x()),
                    px(own.min_y() - inside.min_y())
                ),
            ),
            _ => unknown("offset in parent"),
        },
    );
    entries.push(match used.inner_sizes {
        Some((width, height)) => entry("children size", format!("{} × {}", px(width), px(height))),
        None => unknown("children size"),
    });
    entries.push(entry(
        "scroll offset",
        format!("{}, {}", px(used.offset.0), px(used.offset.1)),
    ));
    entries
}

fn layout_entries(props: &Props, used: &Used) -> Vec<Entry> {
    vec![
        entry("direction", props.direction),
        entry("main axis", props.main_align),
        entry("cross axis", props.cross_align),
        match used.overflow {
            Some(overflow) => {
                let declared = if props.clip { "clip" } else { "visible" };
                entry("overflow", overflow).declared(declared_chip(declared, overflow))
            }
            None => unknown("overflow"),
        },
        match used.scrollers {
            Some(count) => entry("scrolled by", count.to_string()),
            None => unknown("scrolled by"),
        },
    ]
}

fn paint_entries(props: &Props, used: &Used) -> Vec<Entry> {
    let mut entries = vec![match props.background {
        Some(colour) => Entry {
            name: "background",
            value: Value::Colour(colour),
            declared: None,
        },
        None => entry("background", "gradient"),
    }];
    entries.push(entry("corner radius", px(props.corner_radius)));
    entries.push(entry("shadows", props.shadows.len().to_string()));
    entries.push(entry("borders", props.borders.len().to_string()));
    entries.push(match used.opacity {
        Some(opacity) => {
            // The product of this node's opacity and every ancestor's.
            let text = number_text((opacity * 100.0).round() / 100.0);
            let declared = props.opacity.map(number_text);
            entry("opacity", text.clone())
                .declared(declared.and_then(|declared| declared_chip(&declared, &text)))
        }
        None => unknown("opacity"),
    });
    entries
}

fn text_entries(node: &Node) -> Vec<Entry> {
    let Some(text) = &node.used.text else {
        return Vec::new();
    };
    let props = node.props.as_ref();
    let mut entries = Vec::new();
    entries.push(if text.families.is_empty() {
        // No family reached the node: the engine falls back to the app's
        // default font, which the layout results do not name.
        unknown("font family")
    } else {
        entry("font family", text.families.join(", "))
    });
    let size = px(text.size);
    entries.push(entry("font size", size.clone()).declared(inherited(
        props.and_then(|props| props.font_size).map(px),
        &size,
    )));
    let weight = text.weight.to_string();
    entries.push(entry("font weight", weight.clone()).declared(inherited(
        props
            .and_then(|props| props.font_weight)
            .map(|weight| i32::from(weight).to_string()),
        &weight,
    )));
    entries.push(entry("font style", text.slant));
    entries.push(entry("font stretch", text.width.to_string()));
    entries.push(entry("letter spacing", px(text.letter_spacing)));
    entries.push(match text.colour {
        Some(colour) => {
            let used = colour_to_hex(colour);
            let declared = props
                .and_then(|props| props.text_color)
                .map(colour_to_hex);
            Entry {
                name: "colour",
                value: Value::Colour(colour),
                declared: inherited(declared, &used),
            }
        }
        // A gradient or shader fill has no single colour.
        None => unknown("colour"),
    });
    entries.push(entry("text align", text.align));
    if matches!(node.kind, Kind::Label | Kind::Paragraph) {
        entries.push(match text.lines {
            Some(lines) => entry("lines", lines.to_string()),
            None => unknown("lines"),
        });
        entries.push(match text.measured {
            Some((width, height)) => entry("text size", format!("{} × {}", px(width), px(height))),
            None => unknown("text size"),
        });
        entries.push(match (text.lines, text.measured) {
            (Some(lines), Some((_, height))) if lines > 0 => {
                entry("line height", px(height / lines as f32))
            }
            _ => unknown("line height"),
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspector::snapshot::{A11y, UsedText};
    use freya::torin::prelude::Area;

    fn area(x: f32, y: f32, w: f32, h: f32) -> Area {
        Area::new((x, y).into(), (w, h).into())
    }

    fn find<'a>(groups: &'a [Group], title: &str, name: &str) -> &'a Entry {
        groups
            .iter()
            .find(|group| group.title == title)
            .unwrap_or_else(|| panic!("no group {title}"))
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("no row {name} in {title}"))
    }

    fn props() -> Props {
        Props {
            width: "auto".into(),
            height: "auto".into(),
            min_width: "auto".into(),
            min_height: "auto".into(),
            max_width: "auto".into(),
            max_height: "auto".into(),
            direction: "vertical",
            main_align: "start",
            cross_align: "start",
            ..Props::default()
        }
    }

    fn node(id: u64, parent: Option<u64>, props: Props, used: Used) -> Node {
        Node {
            id,
            parent,
            kind: Kind::Rect,
            text: None,
            area: used.border,
            used,
            children: Vec::new(),
            props: Some(props),
            a11y: A11y::default(),
            repeats_fill: false,
        }
    }

    fn used_box(x: f32, y: f32, w: f32, h: f32, padding: f32) -> Used {
        let mut used = Used {
            border: Some(area(x, y, w, h)),
            inner_sizes: Some((10.0, 12.0)),
            overflow: Some("visible"),
            scrollers: Some(0),
            opacity: Some(1.0),
            layer: Some(3),
            ..Used::default()
        };
        used.set_padding([padding; 4]);
        used
    }

    fn tree(parent: Node, child: Node) -> (Snapshot, Node) {
        let child_copy = child.clone();
        (
            Snapshot::from_nodes(vec![parent.id], vec![parent, child]),
            child_copy,
        )
    }

    #[test]
    fn auto_fill_and_flex_resolve_to_the_measured_pixels() {
        let mut declared = props();
        declared.width = "fill".into();
        declared.height = "auto".into();
        let parent = node(1, None, props(), used_box(0.0, 0.0, 400.0, 300.0, 0.0));
        let child = node(2, Some(1), declared, used_box(0.0, 0.0, 400.0, 56.0, 0.0));
        let (snapshot, child) = tree(parent, child);
        let rows = groups(&snapshot, &child);
        let width = find(&rows, "Box", "width");
        assert_eq!(width.value, Value::Text("400".into()));
        assert_eq!(width.declared.as_deref(), Some("fill"));
        let height = find(&rows, "Box", "height");
        assert_eq!(height.value, Value::Text("56".into()));
        assert_eq!(height.declared.as_deref(), Some("auto"));
    }

    #[test]
    fn a_declared_number_that_was_honoured_has_no_chip() {
        assert_eq!(declared_chip("128", "128"), None);
        assert_eq!(declared_chip("128", "128.00"), None);
        assert_eq!(declared_chip("128px", "128"), None);
        assert_eq!(declared_chip("128", "100").as_deref(), Some("128"));
        assert_eq!(declared_chip("50%", "200").as_deref(), Some("50%"));
        assert_eq!(declared_chip("flex 1", "73").as_deref(), Some("flex 1"));
        assert_eq!(declared_chip("", "73"), None);
    }

    #[test]
    fn percentages_resolve_against_the_parents_content_box() {
        assert_eq!(
            resolve_constraint("50%", Some(400.0)),
            Constraint::Px(200.0)
        );
        assert_eq!(
            resolve_constraint("12.5%", Some(80.0)),
            Constraint::Px(10.0)
        );
        assert_eq!(resolve_constraint("50%", None), Constraint::Unresolved);
        assert_eq!(resolve_constraint("auto", Some(1.0)), Constraint::Unset);
        assert_eq!(resolve_constraint("96", None), Constraint::Px(96.0));
        assert_eq!(
            resolve_constraint("fill", Some(1.0)),
            Constraint::Unresolved
        );
        assert_eq!(
            resolve_constraint("flex 2", Some(1.0)),
            Constraint::Unresolved
        );

        let mut declared = props();
        declared.min_width = "50%".into();
        declared.max_height = "fill".into();
        // A parent padded by 20 on each side: its content box is 360 wide.
        let parent = node(1, None, props(), used_box(0.0, 0.0, 400.0, 300.0, 20.0));
        let child = node(2, Some(1), declared, used_box(20.0, 20.0, 100.0, 40.0, 0.0));
        let (snapshot, child) = tree(parent, child);
        let rows = groups(&snapshot, &child);
        let min = find(&rows, "Box", "min width");
        assert_eq!(min.value, Value::Text("180".into()));
        assert_eq!(min.declared.as_deref(), Some("50%"));
        let max = find(&rows, "Box", "max height");
        assert_eq!(max.value, Value::Unknown);
        assert_eq!(max.declared.as_deref(), Some("fill"));
        // An unset limit is no limit, not a made-up number.
        assert_eq!(
            find(&rows, "Box", "max width").value,
            Value::Text("none".into())
        );
    }

    #[test]
    fn padding_and_margin_are_what_the_layout_applied() {
        let mut declared = props();
        declared.padding = [8.0; 4];
        declared.margin = [0.0; 4];
        let mut used = used_box(10.0, 10.0, 100.0, 50.0, 8.0);
        used.margin = [4.0, 0.0, 4.0, 0.0];
        let (snapshot, child) = tree(
            node(1, None, props(), used_box(0.0, 0.0, 400.0, 300.0, 0.0)),
            node(2, Some(1), declared, used),
        );
        let rows = groups(&snapshot, &child);
        let padding = find(&rows, "Box", "padding");
        assert_eq!(padding.value, Value::Text("8".into()));
        assert_eq!(padding.declared, None);
        let margin = find(&rows, "Box", "margin");
        assert_eq!(margin.value, Value::Text("4 0".into()));
        // Declared zero, used four: the layout disagreed, so the chip says so.
        assert_eq!(margin.declared.as_deref(), Some("0"));
        // Offset inside the parent's content box (the parent has no padding).
        assert_eq!(
            find(&rows, "Box", "offset in parent").value,
            Value::Text("10, 10".into())
        );
    }

    #[test]
    fn what_the_engine_did_not_give_is_unknown_not_defaulted() {
        let (snapshot, child) = tree(
            node(1, None, props(), Used::default()),
            node(2, Some(1), props(), Used::default()),
        );
        let rows = groups(&snapshot, &child);
        for name in ["width", "height", "content", "padding", "margin", "position"] {
            assert_eq!(find(&rows, "Box", name).value, Value::Unknown, "{name}");
        }
        assert_eq!(find(&rows, "Element", "layer").value, Value::Unknown);
        assert_eq!(find(&rows, "Layout", "overflow").value, Value::Unknown);
    }

    #[test]
    fn text_shows_the_inherited_style_with_an_inherited_chip() {
        let mut used = used_box(0.0, 0.0, 90.0, 40.0, 0.0);
        used.text = Some(UsedText {
            families: vec!["Adwaita Sans".into()],
            size: 14.0,
            weight: 400,
            slant: "upright",
            width: 5,
            letter_spacing: 0.0,
            colour: crate::style::colour_from_hex("#0a141e"),
            align: "left",
            lines: Some(2),
            measured: Some((88.0, 36.0)),
            intrinsic: None,
            wrap: Default::default(),
        });
        let mut label = node(2, Some(1), props(), used);
        label.kind = Kind::Label;
        label.text = Some("two lines of text".into());
        let mut declared = props();
        declared.font_size = Some(14.0);
        label.props = Some(declared);
        let (snapshot, label) = tree(
            node(1, None, props(), used_box(0.0, 0.0, 400.0, 300.0, 0.0)),
            label,
        );
        let rows = groups(&snapshot, &label);
        // Declared 14 and used 14: no chip. The weight was never declared.
        let size = find(&rows, "Text", "font size");
        assert_eq!(size.value, Value::Text("14".into()));
        assert_eq!(size.declared, None);
        let weight = find(&rows, "Text", "font weight");
        assert_eq!(weight.declared.as_deref(), Some("inherited"));
        assert_eq!(find(&rows, "Text", "lines").value, Value::Text("2".into()));
        assert_eq!(
            find(&rows, "Text", "line height").value,
            Value::Text("18".into())
        );
        assert_eq!(
            find(&rows, "Text", "text size").value,
            Value::Text("88 × 36".into())
        );
        assert_eq!(
            find(&rows, "Text", "colour").declared.as_deref(),
            Some("inherited")
        );
    }

    #[test]
    fn sides_use_the_short_form() {
        assert_eq!(sides_text([8.0; 4]), "8");
        assert_eq!(sides_text([8.0, 4.0, 8.0, 4.0]), "8 4");
        assert_eq!(sides_text([1.0, 2.0, 3.0, 4.0]), "1 2 3 4");
        assert_eq!(sides_chip([8.0; 4], [8.0; 4]), None);
        assert_eq!(
            sides_chip([8.0; 4], [8.0, 8.0, 16.0, 8.0]).as_deref(),
            Some("8")
        );
    }
}
