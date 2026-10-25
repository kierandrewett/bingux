//! A copy of an application's element tree, taken after layout, with the
//! node kind, label, text, area, the editable properties and the
//! accessibility data the inspector shows.

use std::collections::HashMap;

use freya::prelude::*;
use freya::torin::prelude::{Alignment, Direction, Gaps, Size};
use freya_core::accessibility::focusable::Focusable;
use freya_core::elements::label::LabelElement;
use freya_core::elements::paragraph::ParagraphElement;
use freya_core::elements::rect::RectElement;
use freya_core::node_id::NodeId;
use freya_core::tree::Tree;

/// What kind of element a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rect,
    Label,
    Paragraph,
    Other,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Rect => "rect",
            Kind::Label => "label",
            Kind::Paragraph => "paragraph",
            Kind::Other => "element",
        }
    }
}

/// The layout and style values of a node, as the inspector shows and edits
/// them. Sizes read `auto`, `fill`, a pixel count or a percentage; alignments
/// and directions use the keys the editor offers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props {
    pub width: String,
    pub height: String,
    pub min_width: String,
    pub min_height: String,
    pub max_width: String,
    pub max_height: String,
    /// Top, right, bottom and left, in pixels.
    pub padding: [f32; 4],
    pub margin: [f32; 4],
    pub gap: f32,
    /// `vertical` or `horizontal`.
    pub direction: &'static str,
    pub main_align: &'static str,
    pub cross_align: &'static str,
    /// The solid background colour, `None` for gradients and shaders.
    pub background: Option<Color>,
    pub corner_radius: f32,
    pub font_size: Option<f32>,
    pub text_color: Option<Color>,
}

/// The accessibility facts of a node.
#[derive(Clone, Debug, Default)]
pub struct A11y {
    pub role: String,
    pub name: Option<String>,
    pub focusable: Option<bool>,
}

/// One element of the tree.
#[derive(Clone, Debug)]
pub struct Node {
    pub id: u64,
    pub parent: Option<u64>,
    pub kind: Kind,
    /// The text of a label or paragraph, `None` for other elements.
    pub text: Option<String>,
    /// The area the layout gave the node, `None` if it was not laid out.
    pub area: Option<Area>,
    pub children: Vec<u64>,
    /// The editable values; `None` for kinds the inspector cannot edit.
    pub props: Option<Props>,
    pub a11y: A11y,
}

/// The whole tree in preorder, with an index by node id.
#[derive(Default)]
pub struct Snapshot {
    pub roots: Vec<u64>,
    pub nodes: Vec<Node>,
    index: HashMap<u64, usize>,
}

impl Snapshot {
    /// Copies `tree`, starting at the root node.
    pub fn capture(tree: &Tree) -> Self {
        let mut snapshot = Snapshot::default();
        if let Some(root) = visit(tree, NodeId::ROOT, None, &mut snapshot) {
            snapshot.roots.push(root);
        }
        snapshot
    }

    pub fn node(&self, id: u64) -> Option<&Node> {
        self.index.get(&id).map(|&index| &self.nodes[index])
    }

    /// The smallest laid-out node under the point `(x, y)` in logical pixels.
    /// Full-window wrappers that sit later in preorder would otherwise hide the
    /// control under them; among equal areas the later (deeper) node wins.
    pub fn hit(&self, x: f32, y: f32) -> Option<u64> {
        let mut best: Option<(f32, u64)> = None;
        for node in &self.nodes {
            let Some(area) = node.area else { continue };
            let inside =
                x >= area.min_x() && x < area.max_x() && y >= area.min_y() && y < area.max_y();
            let size = area.width() * area.height();
            if inside && best.is_none_or(|(best_size, _)| size <= best_size) {
                best = Some((size, node.id));
            }
        }
        best.map(|(_, id)| id)
    }
}

fn visit(tree: &Tree, id: NodeId, parent: Option<u64>, out: &mut Snapshot) -> Option<u64> {
    let key = id.0.get();
    let mut node = Node {
        id: key,
        parent,
        kind: Kind::Other,
        text: None,
        area: tree.layout.get(&id).map(|layout| layout.area),
        children: Vec::new(),
        props: None,
        a11y: A11y::default(),
    };
    if let Some(element) = tree.elements.get(&id) {
        let element = element.as_ref();
        let any = element as &dyn std::any::Any;
        if let Some(label) = any.downcast_ref::<LabelElement>() {
            node.kind = Kind::Label;
            node.text = Some(label.text.to_string());
            node.props = Some(label_props(label));
            node.a11y = a11y_of(&label.accessibility);
        } else if let Some(rect) = any.downcast_ref::<RectElement>() {
            node.kind = Kind::Rect;
            node.props = Some(rect_props(rect));
            node.a11y = a11y_of(&rect.accessibility);
        } else if let Some(paragraph) = any.downcast_ref::<ParagraphElement>() {
            node.kind = Kind::Paragraph;
            node.a11y = a11y_of(&paragraph.accessibility);
        }
    }
    let index = out.nodes.len();
    out.nodes.push(node);
    out.index.insert(key, index);
    let children = tree.children.get(&id).cloned().unwrap_or_default();
    for child in children {
        if let Some(child_key) = visit(tree, child, Some(key), out) {
            out.nodes[index].children.push(child_key);
        }
    }
    Some(key)
}

fn a11y_of(data: &freya_core::data::AccessibilityData) -> A11y {
    A11y {
        role: format!("{:?}", data.builder.role()),
        name: data.builder.label().map(str::to_owned),
        focusable: match data.a11y_focusable {
            Focusable::Unknown => None,
            Focusable::Disabled => Some(false),
            Focusable::Enabled => Some(true),
        },
    }
}

fn rect_props(rect: &RectElement) -> Props {
    let layout = &rect.layout.layout;
    let background = match &rect.style.background {
        Fill::Color(color) => Some(*color),
        _ => None,
    };
    Props {
        width: size_text(&layout.width),
        height: size_text(&layout.height),
        min_width: size_text(&layout.minimum_width),
        min_height: size_text(&layout.minimum_height),
        max_width: size_text(&layout.maximum_width),
        max_height: size_text(&layout.maximum_height),
        padding: gaps(&layout.padding),
        margin: gaps(&layout.margin),
        gap: layout.spacing.get(),
        direction: direction_key(layout.direction),
        main_align: alignment_key(&layout.main_alignment),
        cross_align: alignment_key(&layout.cross_alignment),
        background,
        corner_radius: f32::from(rect.style.corner_radius.top_left()),
        font_size: rect.text_style_data.font_size.map(f32::from),
        text_color: match &rect.text_style_data.color {
            Some(Fill::Color(color)) => Some(*color),
            _ => None,
        },
    }
}

fn label_props(label: &LabelElement) -> Props {
    let layout = &label.layout.layout;
    Props {
        width: size_text(&layout.width),
        height: size_text(&layout.height),
        min_width: size_text(&layout.minimum_width),
        min_height: size_text(&layout.minimum_height),
        max_width: size_text(&layout.maximum_width),
        max_height: size_text(&layout.maximum_height),
        padding: gaps(&layout.padding),
        margin: gaps(&layout.margin),
        gap: layout.spacing.get(),
        direction: direction_key(layout.direction),
        main_align: alignment_key(&layout.main_alignment),
        cross_align: alignment_key(&layout.cross_alignment),
        background: None,
        corner_radius: 0.0,
        font_size: label.text_style_data.font_size.map(f32::from),
        text_color: match &label.text_style_data.color {
            Some(Fill::Color(color)) => Some(*color),
            _ => None,
        },
    }
}

fn gaps(gaps: &Gaps) -> [f32; 4] {
    [gaps.top(), gaps.right(), gaps.bottom(), gaps.left()]
}

/// `auto`, `fill`, a pixel count, or a percentage, the form the editor reads
/// back.
pub fn size_text(size: &Size) -> String {
    match size {
        Size::Inner => "auto".into(),
        Size::Fill => "fill".into(),
        Size::Pixels(px) => format!("{}", px.get()),
        Size::Percentage(percent) => format!("{}%", percent.get()),
        other => format!("{other:?}"),
    }
}

fn direction_key(direction: Direction) -> &'static str {
    match direction {
        Direction::Vertical => "vertical",
        Direction::Horizontal => "horizontal",
    }
}

fn alignment_key(alignment: &Alignment) -> &'static str {
    match alignment {
        Alignment::Start => "start",
        Alignment::Center => "center",
        Alignment::End => "end",
        Alignment::SpaceBetween => "space-between",
        Alignment::SpaceEvenly => "space-evenly",
        Alignment::SpaceAround => "space-around",
    }
}
