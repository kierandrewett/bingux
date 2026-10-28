//! A copy of an application's element tree, taken after layout, with the
//! node kind, label, text, area, the editable properties and the
//! accessibility data the inspector shows.

use std::collections::HashMap;

use freya::prelude::*;
use freya::engine::prelude::SkParagraph;
use freya::torin::prelude::{Alignment, Area, Direction, Gaps, Point2D, Size, Size2D};
use freya_core::accessibility::id::AccessibilityId;
use freya_core::data::TextStyleData;
use freya_core::accessibility::focusable::Focusable;
use freya_core::elements::label::LabelElement;
use freya_core::elements::paragraph::ParagraphElement;
use freya_core::elements::rect::RectElement;
use freya_core::node_id::NodeId;
use freya_core::tree::Tree;

use crate::component::Published;

/// What kind of element a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rect,
    Label,
    Paragraph,
    Other,
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
    /// The top left radius, which is what the editor shows and sets.
    pub corner_radius: f32,
    /// All four radii as painted: top left, top right, bottom right, bottom
    /// left. The focus ring check compares them corner by corner.
    pub corner_radii: [f32; 4],
    pub font_size: Option<f32>,
    pub text_color: Option<Color>,
    pub font_weight: Option<FontWeight>,
    /// Whether the text is set in the monospace face.
    pub mono: bool,
    pub opacity: Option<f32>,
    /// Whether the box clips its children.
    pub clip: bool,
    pub shadows: Vec<Shadow>,
    pub borders: Vec<Border>,
}

/// The accessibility facts of a node.
#[derive(Clone, Debug, Default)]
pub struct A11y {
    pub role: String,
    pub name: Option<String>,
    pub focusable: Option<bool>,
    /// The component that built the element, from [`ComponentName`].
    ///
    /// [`ComponentName`]: crate::component::ComponentName
    pub component: Option<String>,
    pub selected: Option<bool>,
    /// The accessibility id the element was built with, if it has one.
    pub id: Option<AccessibilityId>,
    /// Whether the window's keyboard focus is on this element.
    pub focused: bool,
    /// Whether this element is a modal scope: the page outside it is inert.
    pub modal: bool,
    /// Whether a modal scope elsewhere makes this element inert: it takes no
    /// focus and no keys until the scope closes.
    pub inert: bool,
    /// The call that built the component, if it published one.
    pub published: Option<Published>,
}

/// What the layout engine resolved for one node, read from its layout results
/// when the snapshot was taken, in logical pixels. Nothing here is the
/// declared style: `auto`, `fill`, percentages and flex shares are already
/// numbers. A field that is `None` is something the engine did not expose.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Used {
    /// The border box: the node's area without its margin.
    pub border: Option<Area>,
    /// The padding the layout applied, top, right, bottom and left. Padding is
    /// a pixel count, so the layout applies exactly the number it was given.
    pub padding: Option<[f32; 4]>,
    /// The content box: the border box inside the padding.
    pub content: Option<Area>,
    /// The accumulated size of the node's children.
    pub inner_sizes: Option<(f32, f32)>,
    /// Top, right, bottom and left margin as the layout applied it.
    pub margin: [f32; 4],
    /// The offset the layout applied to the node's children (a scroll).
    pub offset: (f32, f32),
    /// Hidden nodes are not painted and take no events.
    pub hidden: bool,
    /// The paint layer, higher is in front.
    pub layer: Option<i16>,
    /// The node's opacity with every ancestor's opacity folded in.
    pub opacity: Option<f32>,
    /// `clip` or `visible`, as the effect state resolved it.
    pub overflow: Option<&'static str>,
    /// How many ancestors (the node included) scroll it.
    pub scrollers: Option<usize>,
    /// Whether the layout takes the node out of its parent's flow
    /// (`position: absolute` or global), as an overlay is.
    pub out_of_flow: bool,
    pub text: Option<UsedText>,
}

impl Used {
    /// Records the padding the node's layout applied and the content box it
    /// leaves inside the border box. The layout's own inner area is not used
    /// for this: for an auto-sized node it holds the space available before
    /// the children were measured, and a scroll offset moves its origin.
    pub fn set_padding(&mut self, padding: [f32; 4]) {
        self.padding = Some(padding);
        self.content = self.border.map(|border| {
            let [top, right, bottom, left] = padding;
            Area::new(
                Point2D::new(border.min_x() + left, border.min_y() + top),
                Size2D::new(
                    (border.width() - left - right).max(0.0),
                    (border.height() - top - bottom).max(0.0),
                ),
            )
        });
    }
}

/// The text style a node resolved to after inheritance, and for a label or a
/// paragraph what the text layout measured.
#[derive(Clone, Debug, PartialEq)]
pub struct UsedText {
    pub families: Vec<String>,
    pub size: f32,
    pub weight: i32,
    pub slant: &'static str,
    pub width: i32,
    pub letter_spacing: f32,
    pub colour: Option<Color>,
    pub align: &'static str,
    /// Lines the text laid out in, for a label or paragraph.
    pub lines: Option<usize>,
    /// The measured text: its longest line and its height.
    pub measured: Option<(f32, f32)>,
    /// The width the text would take on one line, with no limit. A one-line
    /// text laid out in less than this is cut off.
    pub intrinsic: Option<f32>,
    /// How the text was broken: the limits it was given and the lines it
    /// came out in. Read by the overflow checks.
    pub wrap: Wrap,
}

/// What a text layout says about how it was broken into lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wrap {
    /// The most lines the element allows, `None` for no limit.
    pub max_lines: Option<usize>,
    /// Whether text that does not fit ends in an ellipsis (or other mark)
    /// rather than being cut. Read from the element's own style, so an
    /// overflow set on an ancestor is not seen.
    pub ellipsis: bool,
    /// The width of the longest unbreakable word, the least width at which
    /// the text does not break inside a word.
    pub longest_word: Option<f32>,
    /// The length of each line in UTF-16 units, trailing spaces left out.
    pub line_chars: Vec<usize>,
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
    /// What the layout engine resolved, as numbers.
    pub used: Used,
    pub children: Vec<u64>,
    /// The editable values; `None` for kinds the inspector cannot edit.
    pub props: Option<Props>,
    pub a11y: A11y,
    /// Whether this box's fill repeats the one its ancestor or the window
    /// already paints, so the chips leave it out.
    pub repeats_fill: bool,
}

impl Node {
    /// The name the tree shows: the component that built the element, else
    /// its accessibility role, else its layout role.
    pub fn title(&self) -> &str {
        if let Some(component) = self.a11y.component.as_deref() {
            return component;
        }
        self.meaningful_role().unwrap_or_else(|| self.layout_role())
    }

    /// The accessibility role, unless it says nothing about the element.
    pub fn meaningful_role(&self) -> Option<&str> {
        match self.a11y.role.as_str() {
            "" | "Unknown" | "GenericContainer" => None,
            role => Some(role),
        }
    }

    /// The role the layout gives the element: `row` or `column` for a box by
    /// its direction, `text` for a label or paragraph, `element` otherwise.
    pub fn layout_role(&self) -> &'static str {
        match self.kind {
            Kind::Label | Kind::Paragraph => "text",
            Kind::Rect => match self.props.as_ref().map(|props| props.direction) {
                Some("horizontal") => "row",
                _ => "column",
            },
            Kind::Other => "element",
        }
    }
}

/// The whole tree in preorder, with an index by node id.
#[derive(Default)]
pub struct Snapshot {
    /// The window's scale factor: areas and sizes here are logical pixels,
    /// the layout's physical pixels divided by it.
    pub scale: f32,
    pub roots: Vec<u64>,
    pub nodes: Vec<Node>,
    index: HashMap<u64, usize>,
    /// Per node: whether the Elements tree leaves it out unless its wrapper
    /// toggle is on. Set by [`Snapshot::settle`].
    hidden: Vec<bool>,
}

impl Snapshot {
    /// Copies `tree`, starting at the root node.
    pub fn capture(tree: &Tree, scale: f32) -> Self {
        let mut snapshot = Snapshot {
            scale,
            ..Snapshot::default()
        };
        if let Some(root) = visit(tree, NodeId::ROOT, None, &mut snapshot) {
            snapshot.roots.push(root);
        }
        snapshot.settle();
        snapshot
    }

    /// A snapshot of hand-made nodes, for tests that render the tree without a
    /// window.
    #[cfg(test)]
    pub(crate) fn from_nodes(roots: Vec<u64>, nodes: Vec<Node>) -> Self {
        let index = nodes
            .iter()
            .enumerate()
            .map(|(at, node)| (node.id, at))
            .collect();
        let mut snapshot = Snapshot {
            scale: 1.0,
            roots,
            nodes,
            index,
            hidden: Vec::new(),
        };
        snapshot.settle();
        snapshot
    }

    /// Whether the Elements tree leaves `id` out unless its wrapper toggle is
    /// on.
    pub fn is_wrapper(&self, id: u64) -> bool {
        self.index.get(&id).is_some_and(|&index| self.hidden[index])
    }

    /// Marks every node outside the topmost modal scope (the last one in
    /// tree order) inert: the page behind a dialog, a sheet or a popup takes
    /// no focus and no keys. The scope's own subtree and the ancestors that
    /// hold it are not inert.
    fn mark_inert(&mut self) {
        let Some(modal) = self.nodes.iter().rposition(|node| node.a11y.modal) else {
            return;
        };
        let modal_id = self.nodes[modal].id;
        let mut live = vec![false; self.nodes.len()];
        // Ancestors of the scope hold it, so they are live.
        let mut up = Some(modal_id);
        while let Some(id) = up {
            if let Some(at) = self.index.get(&id).copied() {
                live[at] = true;
                up = self.nodes[at].parent;
            } else {
                break;
            }
        }
        // Preorder: a parent precedes its children.
        for at in modal..self.nodes.len() {
            let in_scope = at == modal
                || self.nodes[at]
                    .parent
                    .and_then(|id| self.index.get(&id).copied())
                    .is_some_and(|parent| live[parent] && parent >= modal);
            if in_scope {
                live[at] = true;
            }
        }
        for (at, node) in self.nodes.iter_mut().enumerate() {
            node.a11y.inert = !live[at];
        }
    }

    /// Works out, once per capture, which boxes only forward their children
    /// (no name, role, text, fill, spacing or corner of their own), and which
    /// fills repeat what an ancestor or the window already paints.
    fn settle(&mut self) {
        self.mark_inert();
        let count = self.nodes.len();
        let window_fill = self.nodes.iter().find_map(fill_of);
        // Preorder: a parent precedes its children, so its fill is known first.
        let mut from_above: Vec<Option<Color>> = vec![None; count];
        let mut parent_fill: Vec<Option<Color>> = vec![None; count];
        for index in 0..count {
            let parent = self.nodes[index]
                .parent
                .and_then(|id| self.index.get(&id).copied());
            let above = parent.and_then(|parent| from_above[parent]);
            let own = fill_of(&self.nodes[index]);
            parent_fill[index] = above;
            from_above[index] = own.or(above);
            self.nodes[index].repeats_fill =
                own.is_some_and(|colour| Some(colour) == above || Some(colour) == window_fill);
        }
        let mut hidden = vec![false; count];
        for index in (0..count).rev() {
            let node = &self.nodes[index];
            let Some(props) = &node.props else {
                continue;
            };
            if node.kind != Kind::Rect || has_identity(node) {
                continue;
            }
            let spaced = props
                .padding
                .iter()
                .chain(&props.margin)
                .any(|edge| *edge != 0.0)
                || props.gap != 0.0;
            let painted = fill_of(node).is_some_and(|colour| Some(colour) != parent_fill[index])
                || props.corner_radius != 0.0
                || spaced;
            if painted {
                continue;
            }
            hidden[index] = match node.children.as_slice() {
                [_] => true,
                children => children
                    .iter()
                    .all(|child| self.index.get(child).is_some_and(|&c| hidden[c])),
            };
        }
        self.hidden = hidden;
    }

    /// Marks the nodes built with accessibility id `focus` as focused.
    pub fn mark_focused(&mut self, focus: AccessibilityId) {
        for node in &mut self.nodes {
            if node.a11y.id == Some(focus) {
                node.a11y.focused = true;
            }
        }
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
            // The border box: a click in a margin is on the parent.
            let Some(area) = node.used.border.or(node.area) else {
                continue;
            };
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
    let scale = out.scale;
    let mut node = Node {
        id: key,
        parent,
        kind: Kind::Other,
        text: None,
        area: tree
            .layout
            .get(&id)
            .map(|layout| logical(layout.area, scale)),
        used: used_of(tree, id, scale),
        children: Vec::new(),
        props: None,
        a11y: A11y::default(),
        repeats_fill: false,
    };
    if let Some(element) = tree.elements.get(&id) {
        let layout_paragraph = tree
            .layout
            .get(&id)
            .and_then(|layout| layout.data.as_ref())
            .and_then(|data| data.downcast_ref::<SkParagraph>());
        read_element(
            &mut node,
            element.as_ref() as &dyn std::any::Any,
            layout_paragraph,
            scale,
        );
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

/// Fills in what an element says about itself: its kind, text, declared
/// props, accessibility data and padding. `layout_paragraph` is the text
/// layout of a label, which the layout engine holds.
fn read_element(
    node: &mut Node,
    any: &dyn std::any::Any,
    layout_paragraph: Option<&SkParagraph>,
    scale: f32,
) {
    if let Some(label) = any.downcast_ref::<LabelElement>() {
        node.kind = Kind::Label;
        node.used.set_padding(gaps(&label.layout.layout.padding));
        node.used.out_of_flow = out_of_flow(&label.layout.layout);
        if let Some(text) = node.used.text.as_mut()
            && let Some(paragraph) = layout_paragraph
        {
            measure_text(text, paragraph, scale);
            text.wrap.max_lines = label.max_lines;
            text.wrap.ellipsis |= label
                .text_style_data
                .text_overflow
                .as_ref()
                .is_some_and(|overflow| overflow.get_ellipsis().is_some());
        }
        node.text = Some(label.text.to_string());
        node.props = Some(label_props(label));
        node.a11y = a11y_of(&label.accessibility);
    } else if let Some(rect) = any.downcast_ref::<RectElement>() {
        node.kind = Kind::Rect;
        node.used.set_padding(gaps(&rect.layout.layout.padding));
        node.used.out_of_flow = out_of_flow(&rect.layout.layout);
        node.props = Some(rect_props(rect));
        node.a11y = a11y_of(&rect.accessibility);
    } else if let Some(paragraph) = any.downcast_ref::<ParagraphElement>() {
        node.kind = Kind::Paragraph;
        node.used
            .set_padding(gaps(&paragraph.layout.layout.padding));
        node.used.out_of_flow = out_of_flow(&paragraph.layout.layout);
        if let Some(text) = node.used.text.as_mut()
            && let Some(holder) = paragraph.sk_paragraph.0.borrow().as_ref()
        {
            measure_text(text, &holder.paragraph, scale);
            text.wrap.max_lines = paragraph.max_lines;
            text.wrap.ellipsis |= paragraph
                .text_style_data
                .text_overflow
                .as_ref()
                .is_some_and(|overflow| overflow.get_ellipsis().is_some());
        }
        node.text = Some(
            paragraph
                .spans
                .iter()
                .map(|span| span.text.as_ref())
                .collect(),
        );
        node.props = Some(paragraph_props(paragraph));
        node.a11y = a11y_of(&paragraph.accessibility);
    }
}

/// Whether the layout takes the node out of the flow of its siblings: an
/// overlay, a tooltip, a badge on a corner. It is laid on its siblings by
/// design.
fn out_of_flow(layout: &freya::torin::node::Node) -> bool {
    matches!(
        layout.position,
        freya::torin::prelude::Position::Absolute(_) | freya::torin::prelude::Position::Global(_)
    )
}

/// A snapshot of a headless run's tree, for the layout lint: the same nodes
/// as [`Snapshot::capture`] takes of a window, from what a testing runner
/// exposes. It has no effect or layer states, so a node's `overflow` and
/// `layer` are unknown and its text style is blank; the layout checks do not
/// need them.
#[cfg(feature = "testing")]
impl Snapshot {
    pub fn capture_headless(runner: &freya_testing::TestingRunner, scale: f32) -> Self {
        let mut snapshot = Snapshot {
            scale,
            ..Snapshot::default()
        };
        if let Some(root) = runner.find(|node, _| Some(node)) {
            let mut next = 1;
            let root_id = visit_headless(&root, None, &mut next, &mut snapshot);
            snapshot.roots.push(root_id);
        }
        snapshot.settle();
        snapshot
    }
}

#[cfg(feature = "testing")]
fn visit_headless(
    testing: &freya_testing::TestingNode,
    parent: Option<u64>,
    next: &mut u64,
    out: &mut Snapshot,
) -> u64 {
    let key = *next;
    *next += 1;
    let scale = out.scale;
    let layout = testing.layout();
    let mut used = Used {
        border: Some(logical(layout.visible_area(), scale)),
        inner_sizes: Some((
            layout.inner_sizes.width / scale,
            layout.inner_sizes.height / scale,
        )),
        margin: gaps(&layout.margin).map(|edge| edge / scale),
        offset: (layout.offset_x.get() / scale, layout.offset_y.get() / scale),
        hidden: layout.hidden,
        ..Used::default()
    };
    used.text = Some(UsedText {
        families: Vec::new(),
        size: 0.0,
        weight: 400,
        slant: "upright",
        width: 5,
        letter_spacing: 0.0,
        colour: None,
        align: "left",
        lines: None,
        measured: None,
        intrinsic: None,
        wrap: Wrap::default(),
    });
    let mut node = Node {
        id: key,
        parent,
        kind: Kind::Other,
        text: None,
        area: Some(logical(layout.area, scale)),
        used,
        children: Vec::new(),
        props: None,
        a11y: A11y::default(),
        repeats_fill: false,
    };
    let element = testing.element();
    let paragraph = layout
        .data
        .as_ref()
        .and_then(|data| data.downcast_ref::<SkParagraph>());
    read_element(&mut node, element.as_ref() as &dyn std::any::Any, paragraph, scale);
    if let Some(clip) = node.props.as_ref().map(|props| props.clip) {
        node.used.overflow = Some(if clip { "clip" } else { "visible" });
    }
    let index = out.nodes.len();
    out.nodes.push(node);
    out.index.insert(key, index);
    for child in testing.children() {
        let child_key = visit_headless(&child, Some(key), next, out);
        out.nodes[index].children.push(child_key);
    }
    key
}

/// An area the layout measured in physical pixels, in logical pixels.
fn logical(area: Area, scale: f32) -> Area {
    Area::new(
        Point2D::new(area.origin.x / scale, area.origin.y / scale),
        Size2D::new(area.size.width / scale, area.size.height / scale),
    )
}

/// What the layout, the effect, layer and text style states resolved for a
/// node.
fn used_of(tree: &Tree, id: NodeId, scale: f32) -> Used {
    let mut used = Used::default();
    if let Some(layout) = tree.layout.get(&id) {
        used.border = Some(logical(layout.visible_area(), scale));
        used.inner_sizes = Some((
            layout.inner_sizes.width / scale,
            layout.inner_sizes.height / scale,
        ));
        used.margin = gaps(&layout.margin).map(|edge| edge / scale);
        used.offset = (layout.offset_x.get() / scale, layout.offset_y.get() / scale);
        used.hidden = layout.hidden;
    }
    used.layer = tree.layer_state.get(&id).map(|state| state.layer);
    if let Some(effect) = tree.effect_state.get(&id) {
        used.opacity = Some(effect.opacities.iter().product());
        used.overflow = Some(match effect.overflow {
            Overflow::Clip => "clip",
            _ => "visible",
        });
        used.scrollers = Some(effect.scrollables.len());
    }
    if let Some(style) = tree.text_style_state.get(&id) {
        used.text = Some(UsedText {
            families: style.font_families.iter().map(|f| f.to_string()).collect(),
            size: f32::from(style.font_size),
            weight: i32::from(style.font_weight),
            slant: match style.font_slant {
                FontSlant::Upright => "upright",
                FontSlant::Italic => "italic",
                FontSlant::Oblique => "oblique",
            },
            width: i32::from(style.font_width),
            letter_spacing: f32::from(style.letter_spacing),
            colour: style.color.as_color(),
            align: match style.text_align {
                TextAlign::Left => "left",
                TextAlign::Right => "right",
                TextAlign::Center => "center",
                TextAlign::Justify => "justify",
                TextAlign::Start => "start",
                TextAlign::End => "end",
            },
            lines: None,
            measured: None,
            intrinsic: None,
            wrap: Wrap {
                ellipsis: style.text_overflow.get_ellipsis().is_some(),
                ..Wrap::default()
            },
        });
    }
    used
}

/// Fills in what the text layout measured for a label or paragraph.
fn measure_text(text: &mut UsedText, paragraph: &SkParagraph, scale: f32) {
    text.lines = Some(paragraph.line_number());
    text.measured = Some((paragraph.longest_line() / scale, paragraph.height() / scale));
    text.intrinsic = Some(paragraph.max_intrinsic_width() / scale);
    text.wrap.longest_word = Some(paragraph.min_intrinsic_width() / scale);
    text.wrap.line_chars = paragraph
        .get_line_metrics()
        .iter()
        .map(|line| line.end_excluding_whitespaces.saturating_sub(line.start_index))
        .collect();
}

/// The solid fill a node paints, if any.
fn fill_of(node: &Node) -> Option<Color> {
    node.props.as_ref().and_then(|props| props.background).filter(|colour| colour.a() != 0)
}

/// Whether a node says what it is: a name, a component, a meaningful role or
/// a focus stop.
fn has_identity(node: &Node) -> bool {
    node.a11y.name.is_some()
        || node.a11y.component.is_some()
        || node.meaningful_role().is_some()
        || node.a11y.focusable == Some(true)
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
        component: data.builder.class_name().map(str::to_owned),
        selected: data.builder.is_selected(),
        id: data.a11y_id,
        focused: false,
        modal: data.builder.is_modal(),
        inert: false,
        published: data.builder.author_id().and_then(Published::decode),
    }
}

/// The values every element kind shares: its layout, and the text style it
/// declares or passes down.
fn base_props(layout: &freya::torin::node::Node, text: &TextStyleData) -> Props {
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
        font_size: text.font_size.map(f32::from),
        text_color: match &text.color {
            Some(Fill::Color(color)) => Some(*color),
            _ => None,
        },
        font_weight: text.font_weight,
        mono: text
            .font_families
            .iter()
            .any(|family| family.as_ref() == crate::typography::mono_family()),
        ..Props::default()
    }
}

fn rect_props(rect: &RectElement) -> Props {
    let background = match &rect.style.background {
        Fill::Color(color) => Some(*color),
        _ => None,
    };
    Props {
        background,
        corner_radius: f32::from(rect.style.corner_radius.top_left()),
        corner_radii: crate::style::Radii::of(&rect.style.corner_radius).array(),
        opacity: rect.effect.as_ref().and_then(|effect| effect.opacity),
        clip: rect
            .effect
            .as_ref()
            .is_some_and(|effect| effect.overflow == Overflow::Clip),
        shadows: rect.style.shadows.clone(),
        borders: rect.style.borders.clone(),
        ..base_props(&rect.layout.layout, &rect.text_style_data)
    }
}

fn label_props(label: &LabelElement) -> Props {
    Props {
        opacity: label.effect.as_ref().and_then(|effect| effect.opacity),
        ..base_props(&label.layout.layout, &label.text_style_data)
    }
}

fn paragraph_props(paragraph: &ParagraphElement) -> Props {
    Props {
        opacity: paragraph.effect.as_ref().and_then(|effect| effect.opacity),
        ..base_props(&paragraph.layout.layout, &paragraph.text_style_data)
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
        Size::Percentage(percent) | Size::RootPercentage(percent) => format!("{}%", percent.get()),
        Size::FillMinimum => "fill-min".into(),
        Size::Flex(flex) => format!("flex {}", flex.get()),
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
