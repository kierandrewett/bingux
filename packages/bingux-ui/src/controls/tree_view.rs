//! Tree view: nested nodes that open and close, as the element list in
//! Chromium's DevTools. Every row is one line of `Dim::MenuItem` height, so
//! the view can place the selected row and scroll it into view without
//! measuring the rows.
//!
//! The view takes its open branches and its selection as state the caller
//! owns, so an inspector can keep them across a rebuild. The arrow keys move
//! the selection: Up and Down go to the previous and next visible row, Right
//! opens a closed branch or enters an open one, Left closes an open branch or
//! goes to the parent. Enter or Space activates the selected row. Clicking a
//! row selects it, and clicking a selected row activates it; clicking the
//! chevron opens or closes the branch.

use std::collections::HashSet;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::typography::{BADGE, BODY, CAPTION, Ink};

/// One node of a tree. `id` must be unique within the tree: it keys the open
/// branches and the selection.
#[derive(Clone, PartialEq)]
pub struct TreeNode {
    pub id: u64,
    /// The name of the node, e.g. `rect`.
    pub label: String,
    /// Muted text after the label, e.g. `1100 x 820`.
    pub detail: Option<String>,
    pub icon: Option<Icon>,
    /// A short tag at the end of the row, e.g. `shift`, drawn in the paint.
    pub badge: Option<(String, Paint)>,
    pub children: Vec<TreeNode>,
}

/// A tree of `roots`. `expanded` holds the ids of the branches that are open,
/// and `selected` the id of the selected node.
#[derive(PartialEq)]
pub struct TreeView {
    roots: Vec<TreeNode>,
    expanded: State<HashSet<u64>>,
    selected: State<Option<u64>>,
    on_hover: EventHandler<Option<u64>>,
    on_activate: EventHandler<u64>,
}

/// A tree of `roots`, with the open branches in `expanded` and the selected
/// node in `selected`.
pub fn tree_view(
    roots: Vec<TreeNode>,
    expanded: State<HashSet<u64>>,
    selected: State<Option<u64>>,
) -> TreeView {
    TreeView {
        roots,
        expanded,
        selected,
        on_hover: EventHandler::new(|_: Option<u64>| {}),
        on_activate: EventHandler::new(|_: u64| {}),
    }
}

impl TreeView {
    /// Called with the node under the pointer, or `None` when the pointer
    /// leaves the rows.
    pub fn on_hover(mut self, handler: impl Into<EventHandler<Option<u64>>>) -> Self {
        self.on_hover = handler.into();
        self
    }

    /// Called with the id of a node the user activates: Enter or Space on the
    /// selected node, or a click on a node that is already selected.
    pub fn on_activate(mut self, handler: impl Into<EventHandler<u64>>) -> Self {
        self.on_activate = handler.into();
        self
    }
}

/// How a row's chevron shows its node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Branch {
    Leaf,
    Closed,
    Open,
}

/// One visible row: a node with its depth, as the flattened tree shows it.
#[derive(Clone, PartialEq)]
struct Flat {
    id: u64,
    label: String,
    detail: Option<String>,
    icon: Option<Icon>,
    badge: Option<(String, Paint)>,
    depth: usize,
    branch: Branch,
}

/// The visible rows, depth first, skipping the children of closed branches.
fn flatten(nodes: &[TreeNode], depth: usize, expanded: &HashSet<u64>, out: &mut Vec<Flat>) {
    for node in nodes {
        let branch = match (node.children.is_empty(), expanded.contains(&node.id)) {
            (true, _) => Branch::Leaf,
            (false, true) => Branch::Open,
            (false, false) => Branch::Closed,
        };
        out.push(Flat {
            id: node.id,
            label: node.label.clone(),
            detail: node.detail.clone(),
            icon: node.icon.clone(),
            badge: node.badge.clone(),
            depth,
            branch,
        });
        if branch == Branch::Open {
            flatten(&node.children, depth + 1, expanded, out);
        }
    }
}

/// Scroll offset that keeps row `index` in view, as `dropdown` computes it.
/// The offset is negative, as the scroll controller stores it.
fn offset_for(index: usize, current: i32, count: usize, viewport: f32) -> i32 {
    let row = Dim::MenuItem.px();
    let top = index as f32 * row;
    let furthest = (count as f32 * row - viewport).max(0.0);
    let shown_top = -(current as f32);
    let want = if top < shown_top {
        top
    } else if top + row > shown_top + viewport {
        top + row - viewport
    } else {
        shown_top
    };
    -(want.clamp(0.0, furthest) as i32)
}

/// The rows an arrow, Home, End or a page key moves to from `from`.
fn next_index(key: &NamedKey, from: Option<usize>, count: usize, page: usize) -> Option<usize> {
    let last = count.checked_sub(1)?;
    let next = match key {
        NamedKey::ArrowDown => from.map_or(0, |index| (index + 1).min(last)),
        NamedKey::ArrowUp => from.map_or(0, |index| index.saturating_sub(1)),
        NamedKey::PageDown => from.map_or(0, |index| (index + page).min(last)),
        NamedKey::PageUp => from.map_or(0, |index| index.saturating_sub(page)),
        NamedKey::Home => 0,
        NamedKey::End => last,
        _ => return None,
    };
    Some(next)
}

impl Component for TreeView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let a11y_id = use_hook(AccessibilityId::new_unique);
        let focus = use_focus(a11y_id);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut viewport = use_state(Area::default);
        let expanded = self.expanded.clone();
        let mut selected = self.selected.clone();

        let mut rows = Vec::new();
        flatten(&self.roots, 0, &expanded.read(), &mut rows);
        let viewport_height = viewport.read().height();
        let page = ((viewport_height / Dim::MenuItem.px()) as usize).max(1);
        let selected_id = *selected.read();
        let selected_index = selected_id.and_then(|id| rows.iter().position(|row| row.id == id));

        let on_toggle = EventHandler::new({
            let mut expanded = expanded.clone();
            move |id: u64| {
                let mut open = expanded.write();
                if !open.remove(&id) {
                    open.insert(id);
                }
            }
        });
        let on_press = EventHandler::new({
            let on_activate = self.on_activate.clone();
            move |id: u64| {
                let was_selected = *selected.peek() == Some(id);
                selected.set(Some(id));
                if was_selected {
                    on_activate.call(id);
                }
            }
        });

        let on_key_down = {
            let rows = rows.clone();
            let on_activate = self.on_activate.clone();
            let mut expanded = expanded.clone();
            let mut selected = selected.clone();
            move |event: Event<KeyboardEventData>| {
                let current = selected_index;
                if is_activation_key(&event.key) {
                    if let Some(row) = current.and_then(|index| rows.get(index)) {
                        event.stop_propagation();
                        on_activate.call(row.id);
                    }
                    return;
                }
                let Key::Named(named) = &event.key else {
                    return;
                };
                let target = match named {
                    NamedKey::ArrowRight => {
                        let Some(row) = current.and_then(|index| rows.get(index)) else {
                            return;
                        };
                        match row.branch {
                            Branch::Closed => {
                                expanded.write().insert(row.id);
                                return;
                            }
                            Branch::Open => current.map(|index| index + 1),
                            Branch::Leaf => None,
                        }
                    }
                    NamedKey::ArrowLeft => {
                        let Some(index) = current else {
                            return;
                        };
                        let row = &rows[index];
                        if row.branch == Branch::Open {
                            expanded.write().remove(&row.id);
                            return;
                        }
                        rows[..index]
                            .iter()
                            .rposition(|earlier| earlier.depth + 1 == row.depth)
                    }
                    other => next_index(other, current, rows.len(), page),
                };
                event.stop_propagation();
                let Some(index) = target.filter(|index| *index < rows.len()) else {
                    return;
                };
                selected.set(Some(rows[index].id));
                let offset = offset_for(
                    index,
                    <(i32, i32)>::from(scroll).1,
                    rows.len(),
                    viewport.peek().height(),
                );
                scroll.scroll_to_y(offset);
            }
        };

        let children = rows.into_iter().map(|row| {
            let is_selected = selected_id == Some(row.id);
            Element::from(TreeRow {
                row,
                selected: is_selected,
                focus_id: a11y_id,
                on_toggle: on_toggle.clone(),
                on_press: on_press.clone(),
                on_hover: self.on_hover.clone(),
            })
        });

        rect()
            .component("TreeView")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_enabled(true)
            .a11y_role(AccessibilityRole::Tree)
            .tw(&tokens, &[w_full(), h_full()])
            .on_sized(move |event: Event<SizedEventData>| viewport.set(event.area))
            .on_key_down(on_key_down)
            .child(
                ScrollView::new_controlled(scroll)
                    .height(Size::fill())
                    .children(children),
            )
            .focusable(focus(), Radius::Card)
            .tw(&tokens, &[w_full(), h_full()])
    }
}

#[derive(Clone, PartialEq)]
struct TreeRow {
    row: Flat,
    selected: bool,
    focus_id: AccessibilityId,
    on_toggle: EventHandler<u64>,
    on_press: EventHandler<u64>,
    on_hover: EventHandler<Option<u64>>,
}

impl Component for TreeRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let id = self.row.id;
        let focus_id = self.focus_id;
        let on_press = self.on_press.clone();
        let on_toggle = self.on_toggle.clone();
        let on_hover = self.on_hover.clone();
        let on_hover_leave = self.on_hover.clone();

        let indent =
            (0..self.row.depth).map(|_| rect().tw(&tokens, &[w(Dim::IconControl), h_full()]));
        let chevron = match self.row.branch {
            Branch::Leaf => None,
            Branch::Closed => Some(Icon::PanEnd),
            Branch::Open => Some(Icon::PanDown),
        };
        let mut toggle_slot = rect().tw(
            &tokens,
            &[
                w(Dim::IconControl),
                h_full(),
                center(),
                rounded(Radius::Mark),
            ],
        );
        if let Some(icon) = chevron {
            toggle_slot = toggle_slot
                .child(icons::icon(icon, IconSize::Text, Ink::Muted.color(&tokens)))
                .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
                .on_press(move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    on_toggle.call(id);
                });
        }

        let mut line = rect().tw(
            &tokens,
            &[flex_row(), items(Align::Center), w_full(), gap(S1), px(S2)],
        );
        line = line.children(indent).child(toggle_slot);
        if let Some(icon) = self.row.icon.clone() {
            line = line.child(icons::icon(icon, IconSize::Row, Ink::Muted.color(&tokens)));
        }
        line = line.child(
            rect().tw(&tokens, &[w_flex()]).child(
                BODY.in_ink(Ink::Text)
                    .label(&tokens, self.row.label.clone())
                    .max_lines(1),
            ),
        );
        if let Some(detail) = self.row.detail.clone() {
            line = line.child(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, detail)
                    .max_lines(1),
            );
        }
        if let Some((text, paint)) = self.row.badge.clone() {
            line = line.child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            flex_row(),
                            center(),
                            h(Dim::Box5),
                            px(S2),
                            rounded(Radius::Pill),
                            bg(paint),
                        ],
                    )
                    .child(
                        BADGE
                            .in_ink(Ink::OnAccent)
                            .label(&tokens, text)
                            .max_lines(1),
                    ),
            );
        }

        rect()
            .a11y_role(AccessibilityRole::TreeItem)
            .a11y_builder(|builder| builder.set_selected(self.selected))
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::MenuItem),
                    rounded(Radius::Control),
                    surface(Paint::Transparent, hovering(), pressing(), self.selected),
                ],
            )
            .child(line)
            .on_pointer_enter(move |_| {
                hovering.set(true);
                on_hover.call(Some(id));
            })
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
                on_hover_leave.call(None);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                if !event.data().is_primary() {
                    return;
                }
                event.stop_propagation();
                pressing.set(true);
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                focus_id.request_focus();
                on_press.call(id);
            })
    }
}
