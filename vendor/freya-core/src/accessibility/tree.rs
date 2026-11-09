use std::any::Any;

use accesskit::{
    Action,
    ActionRequest,
    Affine,
    Node,
    Rect,
    Role,
    TreeId,
    TreeUpdate,
};
use ragnarok::ProcessedEvents;
use rustc_hash::FxHashMap;
use torin::prelude::{
    CursorPoint,
    LayoutNode,
};

use crate::{
    accessibility::{
        focus_strategy::AccessibilityFocusStrategy,
        focusable::Focusable,
        id::AccessibilityId,
    },
    data::Overflow,
    element::ElementExt,
    elements::{
        label::LabelElement,
        paragraph::ParagraphElement,
    },
    events::emittable::EmmitableEvent,
    integration::{
        EventName,
        EventsChunk,
    },
    node_id::NodeId,
    prelude::{
        AccessibilityFocusMovement,
        Color,
        EventType,
        FontSlant,
        TextAlign,
        TextDecoration,
        WheelEventData,
        WheelSource,
    },
    tree::Tree,
};

pub const ACCESSIBILITY_ROOT_ID: AccessibilityId = AccessibilityId(0);

pub struct AccessibilityTree {
    pub map: FxHashMap<AccessibilityId, NodeId>,
    // Current focused Accessibility Node.
    pub focused_id: AccessibilityId,
}

impl Default for AccessibilityTree {
    fn default() -> Self {
        Self::new(ACCESSIBILITY_ROOT_ID)
    }
}

/// Bingux patch: the room, in logical pixels, a focused node keeps from the edge of the scrollable
/// that scrolls it into view. A focus ring is drawn outside its face (a 2 px gap and a 2 px line in
/// bingux-ui) and a clip cuts it, so a node flush with an edge lost one side of its ring.
pub const FOCUS_SCROLL_MARGIN: f32 = 6.0;

/// How far to move a scrollable's content along one axis so that a node `size` long, whose near
/// edge is `position` from the viewport's near edge, sits [`FOCUS_SCROLL_MARGIN`] inside it.
/// A node out of view is brought to the near edge; one in view moves only when it is closer
/// than the margin to either edge and would fit with it.
fn scroll_delta(position: f32, size: f32, view: f32, outside: bool) -> f32 {
    let margin = FOCUS_SCROLL_MARGIN;
    if outside || position < margin {
        margin - position
    } else if size + 2.0 * margin <= view && position + size > view - margin {
        view - margin - size - position
    } else {
        0.0
    }
}

impl AccessibilityTree {
    pub fn new(focused_id: AccessibilityId) -> Self {
        Self {
            focused_id,
            map: FxHashMap::default(),
        }
    }

    pub fn focused_node_id(&self) -> Option<NodeId> {
        self.map.get(&self.focused_id).cloned()
    }

    /// Dispatch an action requested by assistive technology.
    pub fn handle_action(
        &self,
        request: ActionRequest,
        tree: &mut Tree,
        events_sender: &futures_channel::mpsc::UnboundedSender<EventsChunk>,
    ) -> bool {
        if request.target_tree != TreeId::ROOT {
            return false;
        }
        let Some(node_id) = self.map.get(&request.target_node).copied() else {
            return false;
        };
        let Some(element) = tree.elements.get(&node_id) else {
            return false;
        };
        let mut builder = element.accessibility().builder.clone();
        element.finish_accessibility(&mut builder);
        if builder.is_disabled() {
            return false;
        }
        match request.action {
            Action::Focus => {
                if !element.accessibility().a11y_focusable.is_enabled() {
                    return false;
                }
                tree.accessibility_diff
                    .request_focus(AccessibilityFocusStrategy::Node(request.target_node));
                true
            }
            Action::Blur => {
                if self.focused_id == request.target_node {
                    tree.accessibility_diff
                        .request_focus(AccessibilityFocusStrategy::Node(ACCESSIBILITY_ROOT_ID));
                }
                true
            }
            Action::ScrollIntoView => {
                self.scroll_to(node_id, tree, events_sender);
                true
            }
            _ => events_sender
                .unbounded_send(EventsChunk::Batch(vec![EmmitableEvent {
                    name: EventName::AccessibilityAction,
                    source_event: EventName::AccessibilityAction,
                    node_id,
                    data: EventType::AccessibilityAction(request),
                    bubbles: true,
                }]))
                .is_ok(),
        }
    }

    /// Initialize the Accessibility Tree
    pub fn init(&mut self, tree: &mut Tree, title: &str) -> TreeUpdate {
        tree.accessibility_diff.clear();

        let mut nodes = vec![];

        tree.traverse_depth(|node_id| {
            let accessibility_state = tree.accessibility_state.get(&node_id).unwrap();
            let layout_node = tree.layout.get(&node_id).unwrap();
            let accessibility_node = Self::create_node(node_id, layout_node, tree, title);
            nodes.push((accessibility_state.a11y_id, accessibility_node));
            self.map.insert(accessibility_state.a11y_id, node_id);
        });

        #[cfg(debug_assertions)]
        tracing::info!(
            "Initialized the Accessibility Tree with {} nodes.",
            nodes.len()
        );

        if !self.map.contains_key(&self.focused_id) {
            self.focused_id = ACCESSIBILITY_ROOT_ID;
        }

        TreeUpdate {
            tree_id: TreeId::ROOT,
            nodes,
            tree: Some(accesskit::Tree::new(ACCESSIBILITY_ROOT_ID)),
            focus: self.focused_id,
        }
    }

    /// Process any pending Accessibility Tree update
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn process_updates(
        &mut self,
        tree: &mut Tree,
        events_sender: &futures_channel::mpsc::UnboundedSender<EventsChunk>,
        title: &str,
    ) -> TreeUpdate {
        let requested_focus = tree.accessibility_diff.requested_focus.take();
        let anchor_focus = tree.accessibility_diff.anchor_focus.take();
        let removed_ids = std::mem::take(&mut tree.accessibility_diff.removed);
        let mut added_or_updated_ids =
            std::mem::take(&mut tree.accessibility_diff.added_or_updated);

        #[cfg(debug_assertions)]
        if !removed_ids.is_empty() || !added_or_updated_ids.is_empty() {
            tracing::info!(
                "Updating the Accessibility Tree with {} removals and {} additions/modifications",
                removed_ids.len(),
                added_or_updated_ids.len()
            );
        }

        // Remove all the removed nodes from the update list
        for node_id in removed_ids.keys() {
            added_or_updated_ids.remove(node_id);
            self.map.retain(|_, id| id != node_id);
        }

        // Mark the parent of the removed nodes as updated
        for parent_id in removed_ids.values() {
            if !removed_ids.contains_key(parent_id) {
                added_or_updated_ids.insert(*parent_id);
            }
        }

        // Register the created/updated nodes
        for node_id in added_or_updated_ids.clone() {
            let accessibility_state = tree.accessibility_state.get(&node_id).unwrap();
            self.map.insert(accessibility_state.a11y_id, node_id);

            let node_parent_id = tree.parents.get(&node_id).unwrap_or(&NodeId::ROOT);
            added_or_updated_ids.insert(*node_parent_id);
        }

        // Bingux patch: a control named by its content is named again when text deep inside
        // it changes, not only when a direct child does.
        for node_id in added_or_updated_ids.clone() {
            if let Some(ancestor) = named_ancestor(node_id, tree) {
                added_or_updated_ids.insert(ancestor);
            }
        }

        // Create the updated nodes
        let mut nodes = Vec::new();
        for node_id in added_or_updated_ids {
            let accessibility_state = tree.accessibility_state.get(&node_id).unwrap();
            let layout_node = tree.layout.get(&node_id).unwrap();
            let accessibility_node = Self::create_node(node_id, layout_node, tree, title);
            nodes.push((accessibility_state.a11y_id, accessibility_node));
        }

        let has_request_focus = requested_focus.is_some();

        // Fallback the focused id to the root if the focused node no longer exists
        if !self.map.contains_key(&self.focused_id) {
            self.focused_id = ACCESSIBILITY_ROOT_ID;
        }

        // Focus the requested node id if there is one, from the node a pending absolute focus
        // chose when a relative move came behind it.
        if let Some(anchor) = anchor_focus {
            self.focus_node_with_strategy(anchor, tree);
        }
        if let Some(requested_focus) = requested_focus {
            self.focus_node_with_strategy(requested_focus, tree);
        }

        if let Some(node_id) = self.focused_node_id()
            && has_request_focus
        {
            self.scroll_to(node_id, tree, events_sender);
        }

        TreeUpdate {
            tree_id: TreeId::ROOT,
            nodes,
            tree: Some(accesskit::Tree::new(ACCESSIBILITY_ROOT_ID)),
            focus: self.focused_id,
        }
    }

    /// Focus a Node given the strategy.
    pub fn focus_node_with_strategy(
        &mut self,
        strategy: AccessibilityFocusStrategy,
        tree: &mut Tree,
    ) {
        if let AccessibilityFocusStrategy::Node(id) = strategy {
            if self.map.contains_key(&id) {
                self.focused_id = id;
            }
            return;
        }

        let (navigable_nodes, focused_id) = if strategy.mode()
            == Some(AccessibilityFocusMovement::InsideGroup)
        {
            // Get all accessible nodes in the current group
            let mut group_nodes = Vec::new();

            let node_id = self.map.get(&self.focused_id).unwrap();
            let accessibility_state = tree.accessibility_state.get(node_id).unwrap();
            let member_accessibility_id = accessibility_state.a11y_member_of;
            if let Some(member_accessibility_id) = member_accessibility_id {
                group_nodes = tree
                    .accessibility_groups
                    .get(&member_accessibility_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|id| {
                        let node_id = self.map.get(id).unwrap();
                        let accessibility_state = tree.accessibility_state.get(node_id).unwrap();
                        accessibility_state.a11y_focusable == Focusable::Enabled
                    })
                    .collect();
            }
            (group_nodes, self.focused_id)
        } else {
            let mut nodes = Vec::new();

            tree.traverse_depth(|node_id| {
                let accessibility_state = tree.accessibility_state.get(&node_id).unwrap();
                let member_accessibility_id = accessibility_state.a11y_member_of;

                // Exclude nodes that are members of groups except for the parent of the group
                if let Some(member_accessibility_id) = member_accessibility_id
                    && member_accessibility_id != accessibility_state.a11y_id
                {
                    return;
                }
                if accessibility_state.a11y_focusable == Focusable::Enabled {
                    nodes.push(accessibility_state.a11y_id);
                }
            });

            (nodes, self.focused_id)
        };

        let node_index = navigable_nodes
            .iter()
            .position(|accessibility_id| *accessibility_id == focused_id);

        let target_node = match strategy {
            AccessibilityFocusStrategy::Forward(_) => {
                // Find the next Node
                if let Some(node_index) = node_index {
                    if node_index == navigable_nodes.len() - 1 {
                        navigable_nodes.first().cloned()
                    } else {
                        navigable_nodes.get(node_index + 1).cloned()
                    }
                } else {
                    navigable_nodes.first().cloned()
                }
            }
            AccessibilityFocusStrategy::Backward(_) => {
                // Find the previous Node
                if let Some(node_index) = node_index {
                    if node_index == 0 {
                        navigable_nodes.last().cloned()
                    } else {
                        navigable_nodes.get(node_index - 1).cloned()
                    }
                } else {
                    navigable_nodes.last().cloned()
                }
            }
            _ => unreachable!(),
        };

        self.focused_id = target_node.unwrap_or(focused_id);

        #[cfg(debug_assertions)]
        tracing::info!("Focused {:?} node.", self.focused_id);
    }

    /// Send the necessary wheel events to scroll views so that the given focused [NodeId] is visible on screen.
    ///
    /// Bingux patch: the node is kept [`FOCUS_SCROLL_MARGIN`] from the scrollable's edge, so the
    /// focus ring drawn outside its face is not cut by the clip. A node already in view moves only
    /// when it is closer than that to an edge.
    fn scroll_to(
        &self,
        node_id: NodeId,
        tree: &mut Tree,
        events_sender: &futures_channel::mpsc::UnboundedSender<EventsChunk>,
    ) {
        let Some(effect_state) = tree.effect_state.get(&node_id) else {
            return;
        };
        let mut target_node = node_id;
        let mut emmitable_events = Vec::new();
        // Iterate over the inherited scrollables from the closes to the farthest
        for closest_scrollable in effect_state.scrollables.iter().rev() {
            // Every scrollable has a target node, the first scrollable target is the focused node that we want to make visible,
            // the rest scrollables will in the other hand just have the previous scrollable as target
            let target_layout_node = tree.layout.get(&target_node).unwrap();
            let target_area = target_layout_node.area;
            let viewport = tree.layout.get(closest_scrollable).unwrap().visible_area();

            // Where the node's near edge sits from the viewport's, and what it needs to move.
            let outside = !effect_state.is_visible(&tree.layout, &target_area);
            let delta_x = scroll_delta(
                target_area.min_x() - viewport.min_x(),
                target_area.width(),
                viewport.width(),
                outside,
            );
            let delta_y = scroll_delta(
                target_area.min_y() - viewport.min_y(),
                target_area.height(),
                viewport.height(),
                outside,
            );
            if delta_x != 0.0 || delta_y != 0.0 {
                emmitable_events.push(EmmitableEvent {
                    name: EventName::Wheel,
                    source_event: EventName::Wheel,
                    node_id: *closest_scrollable,
                    data: EventType::Wheel(WheelEventData::new(
                        delta_x as f64,
                        delta_y as f64,
                        WheelSource::Custom,
                        CursorPoint::default(),
                        CursorPoint::default(),
                    )),
                    bubbles: false,
                });
                // A node that was out of view makes this scrollable the target of the next one, so
                // that it makes sure this one is visible; a nudge for the ring's margin does not.
                if outside {
                    target_node = *closest_scrollable;
                }
            }
        }
        events_sender
            .unbounded_send(EventsChunk::Processed(ProcessedEvents {
                emmitable_events,
                ..Default::default()
            }))
            .unwrap();
    }

    /// Create an accessibility node
    pub fn create_node(
        node_id: NodeId,
        layout_node: &LayoutNode,
        tree: &Tree,
        title: &str,
    ) -> Node {
        let element = tree.elements.get(&node_id).unwrap();
        let mut accessibility_data = element.accessibility().into_owned();
        element.finish_accessibility(&mut accessibility_data.builder);

        if node_id == NodeId::ROOT {
            accessibility_data.builder.set_role(Role::Window);
            accessibility_data.builder.set_label(title);
        }

        // Set children
        let children = tree
            .children
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            // A child mounted this frame may not have its accessibility state yet (seen at scale 2
            // in the taxonomy's address bar page, scripts/pixel-check.py): leave it out of this
            // update rather than panic; the next update lists it.
            .filter_map(|child| tree.accessibility_state.get(&child).map(|state| state.a11y_id))
            .collect::<Vec<_>>();
        accessibility_data.builder.set_children(children);

        // Set the area
        let area = layout_node.area.to_f64();
        accessibility_data.builder.set_bounds(Rect {
            x0: area.min_x(),
            x1: area.max_x(),
            y0: area.min_y(),
            y1: area.max_y(),
        });

        // Bingux patch: the accessible name. A name the element was given (`a11y_alt`) is never
        // replaced. A node whose role takes its name from its content (WAI-ARIA "name from
        // content": a button, a tab, a check box, a menu item, a list option, a heading) without
        // one is named by all the text inside it, however deep, leaving out what is hidden and
        // the controls nested in it, which name themselves. Upstream took the value of the
        // first child only, so a row with its title one box down, or a dropdown, had no name,
        // and it overwrote a given name with the first child's text.
        let role = accessibility_data.builder.role();
        if role == Role::Label {
            if accessibility_data.builder.value().is_none()
                && let Some(text) = first_child_text(node_id, tree)
            {
                accessibility_data.builder.set_value(text);
            }
        } else if accessibility_data.builder.label().is_none() {
            // AT-SPI reads a paragraph's name, not its value: it names itself with its text.
            let own_paragraph = (role == Role::Paragraph && is_text(element.as_ref()))
                .then(|| accessibility_data.builder.value().map(String::from))
                .flatten();
            let text = if let Some(text) = own_paragraph {
                Some(text)
            } else if names_from_content(role) {
                // A text element given a role (a label that is a heading) is named by its text.
                let mut pieces: Vec<String> = accessibility_data
                    .builder
                    .value()
                    .filter(|_| is_text(element.as_ref()))
                    .map(|text| vec![text.to_string()])
                    .unwrap_or_default();
                content_text(node_id, tree, &mut pieces, 0);
                (!pieces.is_empty()).then(|| pieces.join(", "))
            } else {
                first_child_text(node_id, tree)
            };
            if let Some(text) = text {
                accessibility_data.builder.set_label(text);
            }
        }

        // Bingux patch: text and pictures inside a control that is named by them are
        // presentational (WAI-ARIA "children presentational"): a reader announces "Network,
        // tab" once, not the tab and then a label "Network" again. A picture with no name (an
        // icon in an SVG) is decoration wherever it is.
        let leaf_role = accessibility_data.builder.role();
        let text_leaf = matches!(leaf_role, Role::Label | Role::Paragraph);
        let unnamed_picture = matches!(leaf_role, Role::SvgRoot | Role::Image)
            && accessibility_data.builder.label().is_none();
        if (text_leaf && inside_named_control(node_id, tree)) || unnamed_picture {
            accessibility_data.builder.set_hidden();
        }

        // Set focusable action
        // This will cause assistive technology to offer the user an option
        // to focus the current element if it supports it.
        if accessibility_data.a11y_focusable.is_enabled() {
            accessibility_data.builder.add_action(Action::Focus);
            accessibility_data.builder.add_action(Action::Blur);
        }

        let builder = &mut accessibility_data.builder;

        if let Some(effect_state) = tree.effect_state.get(&node_id) {
            if let Some(rotation) = effect_state.rotation {
                let rotation = (rotation as f64).to_radians();
                let (sin, cos) = rotation.sin_cos();
                builder.set_transform(Affine::new([cos, sin, -sin, cos, 0.0, 0.0]));
            }

            if effect_state.overflow == Overflow::Clip {
                builder.set_clips_children();
            }
        }

        if let Some(background) = element.style().background.as_color() {
            builder.set_background_color(color_to_accesskit(background));
        }

        let element = element.as_ref() as &dyn Any;
        let is_text_element = element.is::<LabelElement>() || element.is::<ParagraphElement>();
        if !is_text_element {
            builder.set_is_line_breaking_object();
        }

        if let Some(text_style) = tree.text_style_state.get(&node_id) {
            if let Some(color) = text_style.color.as_color() {
                builder.set_foreground_color(color_to_accesskit(color));
            }

            builder.set_font_size(f32::from(text_style.font_size));
            builder.set_font_weight(f32::from(text_style.font_weight));
            builder.set_font_family(text_style.font_families.join(", "));

            if matches!(
                text_style.font_slant,
                FontSlant::Italic | FontSlant::Oblique
            ) {
                builder.set_italic();
            }

            builder.set_text_align(match text_style.resolved_align() {
                TextAlign::Center => accesskit::TextAlign::Center,
                TextAlign::Justify => accesskit::TextAlign::Justify,
                TextAlign::Left | TextAlign::Start => accesskit::TextAlign::Left,
                TextAlign::Right | TextAlign::End => accesskit::TextAlign::Right,
            });

            builder.set_text_direction(if text_style.direction.is_rtl() {
                accesskit::TextDirection::RightToLeft
            } else {
                accesskit::TextDirection::LeftToRight
            });

            let decoration = accesskit::TextDecoration {
                style: accesskit::TextDecorationStyle::Solid,
                color: text_style
                    .color
                    .as_color()
                    .map(color_to_accesskit)
                    .unwrap_or(color_to_accesskit(Color::BLACK)),
            };
            match text_style.text_decoration {
                TextDecoration::Underline => builder.set_underline(decoration),
                TextDecoration::Overline => builder.set_overline(decoration),
                TextDecoration::LineThrough => builder.set_strikethrough(decoration),
                TextDecoration::None => {}
            }
        }

        accessibility_data.builder
    }
}

/// Convert a Freya [Color] into its [accesskit::Color] equivalent.
fn color_to_accesskit(color: Color) -> accesskit::Color {
    accesskit::Color {
        red: color.r(),
        green: color.g(),
        blue: color.b(),
        alpha: color.a(),
    }
}

/// Bingux patch: how deep [`content_text`] looks, and how far up a text leaf or an update looks
/// for the control it names.
const NAME_DEPTH: usize = 12;
/// The longest name made from content; a reader does not read a page as one name.
const NAME_MAX: usize = 400;

/// Whether a node of `role` is named by the text inside it when it was given no name.
fn names_from_content(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::DefaultButton
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::Tab
            | Role::MenuItem
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
            | Role::ListBoxOption
            | Role::ListItem
            | Role::TreeItem
            | Role::Link
            | Role::Heading
            | Role::ComboBox
            | Role::Cell
            | Role::GridCell
            | Role::ColumnHeader
            | Role::RowHeader
            | Role::Tooltip
            | Role::Status
            | Role::Alert
    )
}

/// Roles a person operates. Inside a control named by its content, a nested one names itself
/// and its text is left out of the outer name (a row's switch, a field in a list item).
fn is_control(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::DefaultButton
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::ComboBox
            | Role::TextInput
            | Role::MultilineTextInput
            | Role::SearchInput
            | Role::PasswordInput
            | Role::SpinButton
            | Role::Slider
            | Role::Link
            | Role::MenuItem
            | Role::Tab
    )
}

/// Whether `element` draws text itself (a label or a paragraph).
fn is_text(element: &dyn ElementExt) -> bool {
    let element = element as &dyn Any;
    element.is::<LabelElement>() || element.is::<ParagraphElement>()
}

/// The accessibility data of `node_id` as the tree will see it.
fn finished(node_id: NodeId, tree: &Tree) -> Option<Node> {
    let element = tree.elements.get(&node_id)?;
    let mut builder = element.accessibility().builder.clone();
    element.finish_accessibility(&mut builder);
    Some(builder)
}

/// The text of the first direct child that has one (Freya's own rule, kept for containers).
fn first_child_text(node_id: NodeId, tree: &Tree) -> Option<String> {
    tree.children.get(&node_id)?.iter().find_map(|child| {
        let builder = finished(*child, tree)?;
        (!builder.is_hidden())
            .then(|| builder.value().map(String::from))
            .flatten()
    })
}

/// Appends the visible text inside `node_id`, in tree order, to `pieces`.
fn content_text(node_id: NodeId, tree: &Tree, pieces: &mut Vec<String>, depth: usize) {
    if depth > NAME_DEPTH || pieces.iter().map(String::len).sum::<usize>() > NAME_MAX {
        return;
    }
    let Some(children) = tree.children.get(&node_id) else {
        return;
    };
    for child in children {
        let Some(builder) = finished(*child, tree) else {
            continue;
        };
        if builder.is_hidden() || is_control(builder.role()) {
            continue;
        }
        let text = builder
            .label()
            .or_else(|| matches!(builder.role(), Role::Label | Role::Paragraph).then(|| builder.value()).flatten())
            .map(str::trim)
            .filter(|text| !text.is_empty());
        match text {
            Some(text) => pieces.push(text.to_string()),
            None => content_text(*child, tree, pieces, depth + 1),
        }
    }
}

/// The nearest ancestor of `node_id` that is named by its content, within [`NAME_DEPTH`] steps.
fn named_ancestor(node_id: NodeId, tree: &Tree) -> Option<NodeId> {
    let mut current = node_id;
    for _ in 0..NAME_DEPTH {
        let parent = *tree.parents.get(&current)?;
        if parent == current {
            return None;
        }
        let builder = finished(parent, tree)?;
        if names_from_content(builder.role()) {
            return Some(parent);
        }
        if is_control(builder.role()) || builder.role() == Role::Window {
            return None;
        }
        current = parent;
    }
    None
}

/// Whether the text leaf `node_id` is said by a control around it: a button, check box, tab,
/// option or other control whose content is presentational, a text field (whose value is the
/// text), or a heading, cell or status named by its content because it was given no name.
fn inside_named_control(node_id: NodeId, tree: &Tree) -> bool {
    let mut current = node_id;
    for _ in 0..NAME_DEPTH {
        let Some(parent) = tree.parents.get(&current).copied() else {
            return false;
        };
        if parent == current {
            return false;
        }
        let Some(builder) = finished(parent, tree) else {
            return false;
        };
        let role = builder.role();
        if matches!(
            role,
            Role::TextInput | Role::MultilineTextInput | Role::SearchInput | Role::PasswordInput
        ) {
            return true;
        }
        if presentational_children(role) {
            return true;
        }
        if names_from_content(role) {
            return builder.label().is_none();
        }
        if is_control(role) || role == Role::Window {
            return false;
        }
        current = parent;
    }
    false
}

/// Roles whose content is presentational whatever their name (WAI-ARIA): a button's text is
/// its name or a copy of it, never a separate thing to read.
fn presentational_children(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::DefaultButton
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::Tab
            | Role::MenuItem
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
            | Role::ListBoxOption
            | Role::Slider
            | Role::ProgressIndicator
            | Role::Image
    )
}
