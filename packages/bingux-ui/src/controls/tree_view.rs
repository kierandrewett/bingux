//! Tree view: nested nodes that open and close, as the element list in
//! Chromium's DevTools. Every row is one line of `Dim::MenuItem` height (or
//! `Dim::CodeRow` in the code face), so the view can place the selected row
//! and scroll it into view without measuring the rows.
//!
//! The view takes its open branches and its selection as state the caller
//! owns, so an inspector can keep them across a rebuild. The arrow keys move
//! the selection: Up and Down go to the previous and next visible row, Right
//! opens a closed branch or enters an open one, Left closes an open branch or
//! goes to the parent. Enter or Space opens or closes a branch and activates
//! the row. Clicking a row selects it, and clicking a selected row activates
//! it; clicking the chevron opens or closes the branch, and Alt-clicking it
//! opens or closes the whole subtree. A selection the caller changes (a
//! reveal from a picker, a find) opens the row's ancestors and scrolls it
//! into view. A right click selects the row and reports where it was made, for
//! the caller's context menu.
//!
//! A row has a label, an optional detail after it, muted chips and a badge.
//! The label and the detail take their own paints, so a tree can colour a name
//! and its text as a code view does. `TreeView::find` marks a search text
//! inside the rows, and `TreeView::code` sets the rows in the code face.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

use super::tree_motion::{Slot, Timeline};
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::motion::{Focusable, MOVE_MS, use_tween};
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::theme::Tokens;
use crate::typography::{BADGE, BODY, CAPTION, CODE, Ink, TextStyle};

/// One node of a tree. `id` must be unique within the tree: it keys the open
/// branches and the selection.
#[derive(Clone, PartialEq)]
pub struct TreeNode {
    pub id: u64,
    /// The name of the node, e.g. `Label`.
    pub label: String,
    /// Text after the label, e.g. `1100 x 820`; muted unless `detail_paint`
    /// says otherwise.
    pub detail: Option<String>,
    pub icon: Option<Icon>,
    /// A short tag at the end of the row, e.g. `shift`, drawn in the paint.
    pub badge: Option<(String, Paint)>,
    /// Muted values after the detail, each in its own small box, e.g. the
    /// size and state of an element. A chip that starts with `! ` is an alert
    /// and is drawn in the danger tint. A row never wraps: chips past its end
    /// are cut.
    pub chips: Vec<String>,
    /// The label's paint; the text paint when `None`.
    pub label_paint: Option<Paint>,
    /// The detail's paint; the muted paint when `None`.
    pub detail_paint: Option<Paint>,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// A node with only its label.
    pub fn new(id: u64, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            detail: None,
            icon: None,
            badge: None,
            chips: Vec::new(),
            label_paint: None,
            detail_paint: None,
            children: Vec::new(),
        }
    }

    /// Text after the label.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// An icon before the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A tag at the end of the row.
    pub fn badge(mut self, text: impl Into<String>, paint: Paint) -> Self {
        self.badge = Some((text.into(), paint));
        self
    }

    /// Muted values after the detail.
    pub fn chips(mut self, chips: Vec<String>) -> Self {
        self.chips = chips;
        self
    }

    /// Paints the label and the detail, as a code view colours a tag and its
    /// text.
    pub fn paints(mut self, label: Paint, detail: Paint) -> Self {
        self.label_paint = Some(label);
        self.detail_paint = Some(detail);
        self
    }

    /// The nodes below this one.
    pub fn children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }
}

/// The roots of a tree, compared by identity first: a caller that keeps its
/// nodes in an `Rc` does not pay for a deep comparison of a big tree.
#[derive(Clone)]
struct Roots(Rc<[TreeNode]>);

impl PartialEq for Roots {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

/// A tree of `roots`. `expanded` holds the ids of the branches that are open,
/// and `selected` the id of the selected node.
#[derive(PartialEq)]
pub struct TreeView {
    roots: Roots,
    expanded: State<HashSet<u64>>,
    selected: State<Option<u64>>,
    on_hover: EventHandler<Option<u64>>,
    on_activate: EventHandler<u64>,
    on_context: EventHandler<(u64, (f32, f32))>,
    code: bool,
    counts: bool,
    needle: String,
    current: Option<u64>,
}

/// A tree of `roots`, with the open branches in `expanded` and the selected
/// node in `selected`. `roots` is a `Vec<TreeNode>` or an `Rc<[TreeNode]>`.
pub fn tree_view(
    roots: impl Into<Rc<[TreeNode]>>,
    expanded: State<HashSet<u64>>,
    selected: State<Option<u64>>,
) -> TreeView {
    TreeView {
        roots: Roots(roots.into()),
        expanded,
        selected,
        on_hover: EventHandler::new(|_: Option<u64>| {}),
        on_activate: EventHandler::new(|_: u64| {}),
        on_context: EventHandler::new(|_: (u64, (f32, f32))| {}),
        code: false,
        counts: false,
        needle: String::new(),
        current: None,
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

    /// Called after a right click selects a node, with its id and the window
    /// position of the click, to open a context menu there.
    pub fn on_context(mut self, handler: impl Into<EventHandler<(u64, (f32, f32))>>) -> Self {
        self.on_context = handler.into();
        self
    }

    /// Sets the rows in the code face, in a code-height row, as an element
    /// inspector's tree is.
    pub fn code(mut self) -> Self {
        self.code = true;
        self
    }

    /// Shows the number of children on a closed branch.
    pub fn child_counts(mut self) -> Self {
        self.counts = true;
        self
    }

    /// Marks `needle`, ignoring case, inside the rows' text. The `current`
    /// node's marks take the current-match colour.
    pub fn find(mut self, needle: impl Into<String>, current: Option<u64>) -> Self {
        self.needle = needle.into();
        self.current = current;
        self
    }

    fn row_height(&self) -> f32 {
        if self.code {
            Dim::CodeRow.px()
        } else {
            Dim::MenuItem.px()
        }
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
    chips: Vec<String>,
    label_paint: Option<Paint>,
    detail_paint: Option<Paint>,
    children: usize,
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
            chips: node.chips.clone(),
            label_paint: node.label_paint,
            detail_paint: node.detail_paint,
            children: node.children.len(),
            depth,
            branch,
        });
        if branch == Branch::Open {
            flatten(&node.children, depth + 1, expanded, out);
        }
    }
}

/// The node `id`, wherever it is.
fn find_node(nodes: &[TreeNode], id: u64) -> Option<&TreeNode> {
    nodes.iter().find_map(|node| {
        if node.id == id {
            Some(node)
        } else {
            find_node(&node.children, id)
        }
    })
}

/// The ids from a root down to `id`, `id` last, or `None` when it is not in
/// the tree.
fn path_to(nodes: &[TreeNode], id: u64, out: &mut Vec<u64>) -> bool {
    for node in nodes {
        out.push(node.id);
        if node.id == id || path_to(&node.children, id, out) {
            return true;
        }
        out.pop();
    }
    false
}

/// Opens or closes every branch of `node` and below.
fn set_subtree(node: &TreeNode, open: bool, expanded: &mut HashSet<u64>) {
    if node.children.is_empty() {
        return;
    }
    if open {
        expanded.insert(node.id);
    } else {
        expanded.remove(&node.id);
    }
    for child in &node.children {
        set_subtree(child, open, expanded);
    }
}

/// Scroll offset that keeps row `index` in view, as `dropdown` computes it.
/// The offset is negative, as the scroll controller stores it.
fn offset_for(index: usize, current: i32, count: usize, viewport: f32, row: f32) -> i32 {
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

/// The selection the view has already scrolled to, so a selection is revealed
/// once when it changes and never again while the user scrolls.
#[derive(Default)]
struct SelectionReveal {
    last: Option<u64>,
}

impl SelectionReveal {
    /// Whether `selected` (at row `index`, once the view is `measured`) is a
    /// new selection to scroll to. A selection whose row is not shown yet
    /// stays pending: its ancestors open on the next render.
    fn moves(&mut self, selected: Option<u64>, index: Option<usize>, measured: bool) -> bool {
        if selected.is_none() {
            self.last = None;
            return false;
        }
        if selected == self.last || index.is_none() || !measured {
            return false;
        }
        self.last = selected;
        true
    }
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

/// The character ranges of `plain` where `needle` matches, ignoring case.
pub(crate) fn match_ranges(plain: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle: Vec<char> = needle.trim().chars().map(lower).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let text: Vec<char> = plain.chars().map(lower).collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at + needle.len() <= text.len() {
        if text[at..at + needle.len()] == needle[..] {
            out.push((at, at + needle.len()));
            at += needle.len();
        } else {
            at += 1;
        }
    }
    out
}

/// Lower-cases one character, keeping the character count of the text.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

impl Component for TreeView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let a11y_id = use_hook(AccessibilityId::new_unique);
        let focus = use_focus(a11y_id);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut viewport = use_state(Area::default);
        let mut revealed = use_state(|| None::<u64>);
        let expanded = self.expanded;
        let selected = self.selected;
        let row_height = self.row_height();
        let roots = self.roots.clone();

        // A newly selected node opens its ancestors. Only a change of
        // selection does this, so a branch the user closed stays closed when
        // the roots are rebuilt.
        let chosen = *selected.read();
        use_side_effect_with_deps(
            &(chosen, roots.clone()),
            move |dep: &(Option<u64>, Roots)| {
                let previous = *revealed.peek();
                revealed.set(dep.0);
                let Some(id) = dep.0 else {
                    return;
                };
                if previous == Some(id) {
                    return;
                }
                let mut path = Vec::new();
                if !path_to(&dep.1.0, id, &mut path) {
                    return;
                }
                path.pop();
                let mut open = expanded;
                let mut open = open.write();
                for ancestor in path {
                    open.insert(ancestor);
                }
            },
        );

        let mut rows = Vec::new();
        flatten(&roots.0, 0, &expanded.read(), &mut rows);
        let viewport_height = viewport.read().height();
        let page = ((viewport_height / row_height) as usize).max(1);
        let selected_id = *selected.read();
        let selected_index = selected_id.and_then(|id| rows.iter().position(|row| row.id == id));

        // Keeps the selected row in view when the selection changes from
        // outside (a key, a find, a reveal). Decided here, from what render is
        // given, and not in an effect: `scroll_to_y` reads the controller's
        // bounds, so an effect that called it ran again after every wheel notch
        // and snapped a user's scroll back to the selection. Render reads the
        // offset from a plain cell it keeps current. Below one row the viewport
        // is not measured yet, so the reveal waits for a real height.
        let scroll_y = use_hook(|| Rc::new(Cell::new(0_i32)));
        scroll_y.set(<(i32, i32)>::from(scroll).1);
        let reveal = use_hook(|| Rc::new(RefCell::new(SelectionReveal::default())));
        let measured = viewport_height >= row_height;
        let moved = reveal
            .borrow_mut()
            .moves(selected_id, selected_index, measured);
        if let Some(index) = selected_index.filter(|_| moved) {
            let current = scroll_y.get();
            let offset = offset_for(index, current, rows.len(), viewport_height, row_height);
            if offset != current {
                spawn(async move {
                    let mut scroll = scroll;
                    scroll.scroll_to_y(offset);
                });
            }
        }

        let on_toggle = EventHandler::new({
            let roots = roots.clone();
            move |(id, subtree): (u64, bool)| {
                let mut expanded = expanded;
                let mut open = expanded.write();
                let was_open = open.contains(&id);
                match (subtree, find_node(&roots.0, id)) {
                    (true, Some(node)) => set_subtree(node, !was_open, &mut open),
                    _ if was_open => {
                        open.remove(&id);
                    }
                    _ => {
                        open.insert(id);
                    }
                }
            }
        });
        let on_press = EventHandler::new({
            let on_activate = self.on_activate.clone();
            move |id: u64| {
                let mut selected = selected;
                let was_selected = *selected.peek() == Some(id);
                selected.set(Some(id));
                if was_selected {
                    on_activate.call(id);
                }
            }
        });
        let on_context = EventHandler::new({
            let on_context = self.on_context.clone();
            move |(id, at): (u64, (f32, f32))| {
                let mut selected = selected;
                selected.set(Some(id));
                on_context.call((id, at));
            }
        });

        let on_key_down = {
            let rows = rows.clone();
            let on_activate = self.on_activate.clone();
            move |event: Event<KeyboardEventData>| {
                let mut expanded = expanded;
                let mut selected = selected;
                let current = selected_index;
                if is_activation_key(&event.key) {
                    if let Some(row) = current.and_then(|index| rows.get(index)) {
                        event.stop_propagation();
                        if row.branch != Branch::Leaf {
                            let mut open = expanded.write();
                            if !open.remove(&row.id) {
                                open.insert(row.id);
                            }
                        }
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
                                event.stop_propagation();
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
                        event.stop_propagation();
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
                if let Some(row) = target.and_then(|index| rows.get(index)) {
                    selected.set(Some(row.id));
                }
            }
        };

        let needle = self.needle.clone();
        // The rows to draw: the visible ones, plus rows that have just left
        // the list and are shrinking away where they were.
        let motion = tokens.motion(MOVE_MS);
        let tick = use_state(|| 0_u32);
        let _ = tick.read();
        let (epoch, timeline) = use_hook(|| {
            (
                Instant::now(),
                Rc::new(RefCell::new(Timeline::<Flat>::new())),
            )
        });
        let drawn = timeline.borrow_mut().sync(
            rows.iter().map(|row| (row.id, row.clone())),
            epoch.elapsed(),
            motion,
        );
        // A leaving row is dropped from the list on the first draw after its
        // motion, so one draw is asked for then.
        let leaving: Vec<u64> = drawn
            .iter()
            .filter(|(_, _, slot)| *slot == Slot::Leaving)
            .map(|(id, _, _)| *id)
            .collect();
        use_side_effect_with_deps(&leaving, move |leaving: &Vec<u64>| {
            if leaving.is_empty() {
                return;
            }
            let mut tick = tick;
            spawn(async move {
                timer(Duration::from_millis(motion + 20)).await;
                let next = tick.peek().wrapping_add(1);
                tick.set(next);
            });
        });
        let children = drawn.into_iter().map(|(id, row, slot)| {
            let is_selected = selected_id == Some(row.id);
            let mark = if self.current == Some(row.id) {
                Paint::SearchCurrent
            } else {
                Paint::SearchHit
            };
            let line = TreeRow {
                row,
                selected: is_selected,
                code: self.code,
                counts: self.counts,
                needle: needle.clone(),
                mark,
                focus_id: a11y_id,
                on_toggle: on_toggle.clone(),
                on_press: on_press.clone(),
                on_hover: self.on_hover.clone(),
                on_context: on_context.clone(),
            };
            // Keyed, so a row that moves in the list keeps its own motion.
            rect()
                .key(id)
                .tw_layout(&[w_full()])
                .child(TreeSlot {
                    line: Element::from(line),
                    height: row_height,
                    slot,
                })
                .into_element()
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
            // `focusable_inside` returns a wrapper that sizes to its content,
            // so the wrapper is sized too.
            .focusable_inside(focus(), Radius::Card)
            .tw(&tokens, &[w_full(), h_full()])
    }
}

/// The box a tree row lives in: its height grows from nothing as the row
/// enters and shrinks to nothing as it leaves, clipping the row, so the rows
/// below move with its edge. The height tween is interruptible: a row that
/// returns mid-exit turns around from the height it has.
#[derive(Clone, PartialEq)]
struct TreeSlot {
    line: Element,
    height: f32,
    slot: Slot,
}

impl Component for TreeSlot {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        // A row that enters mounts at nothing and starts growing on the next
        // frame; every other row mounts at its full height.
        let mut armed = use_state(|| self.slot != Slot::Entering);
        use_side_effect_with_deps(&(), move |_| {
            if !*armed.peek() {
                armed.set(true);
            }
        });
        let open = armed() && self.slot != Slot::Leaving;
        let progress = use_tween(if open { 1.0 } else { 0.0 }, tokens.motion(MOVE_MS));
        let moving = progress < 1.0;
        rect()
            .tw_layout(&[clip(), w_full()])
            .height(Size::px(self.height * progress))
            .interactive(self.slot != Slot::Leaving)
            .maybe(moving, |slot| slot.opacity(progress))
            .child(self.line.clone())
    }
}

#[derive(Clone, PartialEq)]
struct TreeRow {
    row: Flat,
    selected: bool,
    code: bool,
    counts: bool,
    needle: String,
    mark: Paint,
    focus_id: AccessibilityId,
    on_toggle: EventHandler<(u64, bool)>,
    on_press: EventHandler<u64>,
    on_hover: EventHandler<Option<u64>>,
    on_context: EventHandler<(u64, (f32, f32))>,
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
        let on_context = self.on_context.clone();

        // A guide line per level, so a deep row shows which branch it is in.
        let indent = (0..self.row.depth).map(|_| {
            rect().tw(
                &tokens,
                &[w(Dim::TreeIndent), h_full(), border(Line::HairlineRight)],
            )
        });
        // One chevron that turns, so opening and closing read as one motion.
        let turn = use_tween(
            if self.row.branch == Branch::Open { 1.0 } else { 0.0 },
            tokens.motion(MOVE_MS),
        );
        let chevron = (self.row.branch != Branch::Leaf).then_some(Icon::PanEnd);
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
                .child(rect().rotate(turn * 90.0).child(icons::icon(
                    icon,
                    IconSize::Text,
                    Ink::Muted.color(&tokens),
                )))
                .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
                .on_press(move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    // Alt opens or closes the whole subtree, as Chrome does.
                    let subtree = freya_winit::modifiers::live().contains(Modifiers::ALT);
                    on_toggle.call((id, subtree));
                });
        }

        // The label and the detail are one run of text, so a find mark can
        // span them, and an ellipsis ends the run rather than wrapping it.
        let (label_style, detail_style): (TextStyle, TextStyle) = if self.code {
            (CODE, CODE)
        } else {
            (BODY, CAPTION)
        };
        let label_paint = self.row.label_paint.unwrap_or(Paint::Text);
        let detail_paint = self.row.detail_paint.unwrap_or(Paint::Muted);
        let separator = if self.code { "  " } else { " " };
        let mut plain = self.row.label.clone();
        let mut text = paragraph()
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .span(label_style.span_in_paint(&tokens, self.row.label.clone(), label_paint));
        if let Some(detail) = &self.row.detail {
            let run = format!("{separator}{detail}");
            plain.push_str(&run);
            text = text.span(detail_style.span_in_paint(&tokens, run, detail_paint));
        }
        let text = text
            .highlights(match_ranges(&plain, &self.needle))
            .highlight_color(self.mark.color(&tokens));

        let chips = self
            .row
            .chips
            .iter()
            .map(|chip| chip_element(&tokens, chip, &self.needle, self.mark));
        let count = (self.counts && self.row.branch == Branch::Closed)
            .then(|| count_badge(&tokens, self.row.children));

        // The line is exactly the row's height: a long label or a run of
        // chips is cut at the right edge, never wrapped, so the chevron slot
        // stays centred on the line and indentation only moves the row
        // sideways.
        let mut line = rect().tw(
            &tokens,
            &[
                flex_row(),
                items(Align::Center),
                w_full(),
                h_full(),
                gap(S1),
                px(S2),
                clip(),
            ],
        );
        line = line.children(indent).child(toggle_slot);
        if let Some(icon) = self.row.icon.clone() {
            line = line.child(icons::icon(icon, IconSize::Row, Ink::Muted.color(&tokens)));
        }
        line = line.child(
            rect()
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        items(Align::Center),
                        gap(S1),
                        w_flex(),
                        h_full(),
                        clip(),
                    ],
                )
                .child(rect().tw(&tokens, &[clip()]).child(text))
                .children(chips)
                .maybe_child(count),
        );
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
                            .label_in_paint(&tokens, text, paint.on().unwrap_or(Paint::Text))
                            .max_lines(1),
                    ),
            );
        }

        let height = if self.code { Dim::CodeRow } else { Dim::MenuItem };
        rect()
            .a11y_role(AccessibilityRole::TreeItem)
            .a11y_builder({
                let selected = self.selected;
                move |builder| builder.set_selected(selected)
            })
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(height),
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
                let right = matches!(
                    event.data(),
                    PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
                );
                if right {
                    event.stop_propagation();
                    let at = event.global_location();
                    on_context.call((id, (at.x as f32, at.y as f32)));
                    return;
                }
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

/// The chip for one value: muted type on a rounded control fill, with the
/// find marks inside it. A chip that starts with `! ` is an alert: the text
/// after it, in the danger tint.
fn chip_element(tokens: &Tokens, chip: &str, needle: &str, mark: Paint) -> Element {
    let (text, alert) = match chip.strip_prefix("! ") {
        Some(text) => (text, true),
        None => (chip, false),
    };
    let (fill, ink) = if alert {
        (Paint::DangerSoft, Paint::Danger)
    } else {
        (Paint::Control, Paint::Muted)
    };
    rect()
        .tw(tokens, &[px(S1), rounded(Radius::Mark), center(), bg(fill)])
        .child(
            paragraph()
                .max_lines(1)
                .highlights(match_ranges(text, needle))
                .highlight_color(mark.color(tokens))
                .span(CAPTION.span_in_paint(tokens, text, ink)),
        )
        .into()
}

/// The child count a closed branch shows at the end of its line.
fn count_badge(tokens: &Tokens, count: usize) -> Element {
    rect()
        .tw(
            tokens,
            &[
                px(S1),
                rounded(Radius::Mark),
                center(),
                bg(Paint::MutedFaint),
            ],
        )
        .child(CODE.label(tokens, &count.to_string()))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_every_case_insensitive_substring() {
        assert_eq!(match_ranges("row  \"Box\"", "box"), vec![(6, 9)]);
        assert_eq!(match_ranges("aaaa", "aa"), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn empty_or_absent_needle_matches_nothing() {
        assert!(match_ranges("row", "").is_empty());
        assert!(match_ranges("row", "   ").is_empty());
        assert!(match_ranges("row", "circle").is_empty());
    }

    fn sample() -> Vec<TreeNode> {
        vec![TreeNode::new(1, "a").children(vec![
            TreeNode::new(2, "b").children(vec![TreeNode::new(3, "c")]),
            TreeNode::new(4, "d"),
        ])]
    }

    #[test]
    fn a_path_runs_from_the_root_to_the_node() {
        let mut path = Vec::new();
        assert!(path_to(&sample(), 3, &mut path));
        assert_eq!(path, vec![1, 2, 3]);
        let mut none = Vec::new();
        assert!(!path_to(&sample(), 9, &mut none));
        assert!(none.is_empty());
    }

    #[test]
    fn a_subtree_opens_and_closes_as_one() {
        let roots = sample();
        let mut open = HashSet::new();
        set_subtree(&roots[0], true, &mut open);
        assert_eq!(open, HashSet::from([1, 2]));
        set_subtree(&roots[0], false, &mut open);
        assert!(open.is_empty());
    }

    #[test]
    fn only_open_branches_show_their_children() {
        let roots = sample();
        let mut rows = Vec::new();
        flatten(&roots, 0, &HashSet::from([1]), &mut rows);
        let shown: Vec<(u64, usize)> = rows.iter().map(|row| (row.id, row.depth)).collect();
        assert_eq!(shown, vec![(1, 0), (2, 1), (4, 1)]);
    }

    #[test]
    fn a_new_selection_is_revealed_once() {
        let mut reveal = SelectionReveal::default();
        assert!(reveal.moves(Some(3), Some(2), true));
        assert!(!reveal.moves(Some(3), Some(2), true));
        // The rows changing under the same selection is not a new selection.
        assert!(!reveal.moves(Some(3), Some(9), true));
    }

    #[test]
    fn a_selection_waits_for_its_row_and_a_measured_view() {
        let mut reveal = SelectionReveal::default();
        assert!(!reveal.moves(Some(3), None, true));
        assert!(!reveal.moves(Some(3), Some(2), false));
        assert!(reveal.moves(Some(3), Some(2), true));
    }

    #[test]
    fn clearing_the_selection_lets_the_same_row_reveal_again() {
        let mut reveal = SelectionReveal::default();
        assert!(reveal.moves(Some(3), Some(2), true));
        assert!(!reveal.moves(None, None, true));
        assert!(reveal.moves(Some(3), Some(2), true));
    }

    #[test]
    fn a_row_already_in_view_is_not_moved() {
        // A viewport of five 20 px rows at the top: row 2 is shown, row 9 is not.
        assert_eq!(offset_for(2, 0, 10, 100.0, 20.0), 0);
        assert_eq!(offset_for(9, 0, 10, 100.0, 20.0), -100);
    }
}
