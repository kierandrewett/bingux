//! The Elements tree: every element of the application's snapshot as one
//! outline line, indented by depth. A line gives the element's name (the
//! component that built it, else its accessibility role, else its layout
//! role), its text in quotes, and muted chips for its size, layout and state.
//! A closed branch shows its child count. The arrow keys move the selection,
//! Ctrl+F opens a find bar, a breadcrumb trail shows the selected element's
//! ancestors, and a right click opens a menu.
//!
//! The selection and the open branches live in the window's state, so a
//! reveal from the picker opens the element's ancestors and scrolls to it.

use std::collections::HashSet;
use std::rc::Rc;

use crate::motion::Focusable;
use freya::prelude::*;

use super::snapshot::{Node, Props, Snapshot};
use crate::controls::{breadcrumb, icon_button, text_field};
use crate::icons::{self, Icon};
use crate::shell::menu::{MenuItem, menu_item, menu_separator};
use crate::shell::overlays::Overlays;
use crate::style::*;
use crate::surface::{Interaction, is_activation_key, use_tokens};
use crate::theme::Tokens;
use crate::typography::{CAPTION, CODE, Ink};

/// The longest run of element text shown on an outline line.
const TEXT_LIMIT: usize = 60;

/// A snapshot compared by identity. A new snapshot is a new value, so the
/// tree re-renders and its effects see the current element data.
#[derive(Clone)]
pub struct Shot(pub Rc<Snapshot>);

impl PartialEq for Shot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The tree of the snapshot. `selected` and `expanded` belong to the window,
/// so a reveal from the picker reaches the tree.
#[derive(Clone, PartialEq)]
pub struct DomTree {
    pub snap: Shot,
    pub selected: State<Option<u64>>,
    pub expanded: State<HashSet<u64>>,
    pub wrappers: State<bool>,
}

/// Whether a row has children and whether they are shown.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Branch {
    Leaf,
    Closed,
    Open,
}

/// How a row matches the find bar: the current match, another match, or none.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Hit,
    Current,
}

/// One visible row: an element with its depth, as the tree shows it.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Flat {
    id: u64,
    depth: usize,
    branch: Branch,
}

impl Component for DomTree {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let a11y_id = use_hook(AccessibilityId::new_unique);
        let focus = use_focus(a11y_id);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut viewport = use_state(Area::default);
        let mut query = use_state(String::new);
        let mut find_open = use_state(|| false);
        let mut hit = use_state(|| 0usize);
        let mut revealed = use_state(|| None::<u64>);
        let mut expanded = self.expanded;
        let mut selected = self.selected;

        // A newly selected element opens its ancestors. Only a change of
        // selection does this, so a branch the user closed stays closed when
        // the snapshot refreshes.
        let chosen = *selected.read();
        let shot = self.snap.clone();
        use_side_effect_with_deps(&(chosen, shot), move |dep: &(Option<u64>, Shot)| {
            let previous = *revealed.peek();
            revealed.set(dep.0);
            let Some(id) = dep.0 else {
                return;
            };
            if previous == Some(id) {
                return;
            }
            let mut open = expanded.write();
            for ancestor in ancestors(&dep.1.0, id).iter().rev().skip(1) {
                open.insert(*ancestor);
            }
        });

        let snap = self.snap.0.clone();
        let open = self.expanded.read().clone();
        let mut rows = Vec::new();
        let show_wrappers = *self.wrappers.read();
        flatten(&snap, &snap.roots, 0, &open, show_wrappers, &mut rows);
        let selected_id = *selected.read();
        let selected_index = selected_id.and_then(|id| rows.iter().position(|row| row.id == id));

        // Keeps the selected row in view when the selection, the row count or
        // the measured viewport height changes. Below one row the viewport is
        // not measured yet and `offset_for` flips between two offsets, so the
        // effect waits for a real height rather than looping.
        let viewport_height = viewport.read().height() as i32;
        // The effect must not read the scroll state: a read inside an effect
        // subscribes it, and its own scroll_to_y would wake it again in a loop
        // that never yields to the event loop (input and redraws stop), and a
        // user scroll would snap back to the selection. Render reads the
        // offset and leaves it in a plain cell for the effect.
        let scroll_y = use_hook(|| Rc::new(std::cell::Cell::new(0_i32)));
        scroll_y.set(<(i32, i32)>::from(scroll).1);
        let last_y = scroll_y.clone();
        use_side_effect_with_deps(
            &(selected_index, rows.len(), viewport_height),
            move |dep: &(Option<usize>, usize, i32)| {
                let (Some(index), count, height) = *dep else {
                    return;
                };
                if (height as f32) < Dim::CodeRow.px() {
                    return;
                }
                let current = last_y.get();
                let offset = offset_for(index, current, count, height as f32);
                if offset != current {
                    scroll.scroll_to_y(offset);
                }
            },
        );

        let needle = if find_open() {
            query.read().clone()
        } else {
            String::new()
        };
        let matches = find_matches(&snap, &needle, show_wrappers);
        let matched: HashSet<u64> = matches.iter().copied().collect();
        let hit_index = (*hit.read()).min(matches.len().saturating_sub(1));
        let current = matches.get(hit_index).copied();

        let on_key_down = {
            let rows = rows.clone();
            move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    if let Some(row) = selected_index.and_then(|index| rows.get(index)) {
                        event.stop_propagation();
                        if row.branch != Branch::Leaf {
                            toggle(&mut expanded.write(), row.id);
                        }
                    }
                    return;
                }
                let Key::Named(named) = &event.key else {
                    return;
                };
                let target = match named {
                    NamedKey::ArrowRight => {
                        let Some(row) = selected_index.and_then(|index| rows.get(index)) else {
                            return;
                        };
                        match row.branch {
                            Branch::Closed => {
                                event.stop_propagation();
                                expanded.write().insert(row.id);
                                return;
                            }
                            Branch::Open => selected_index.map(|index| index + 1),
                            Branch::Leaf => None,
                        }
                    }
                    NamedKey::ArrowLeft => {
                        let Some(index) = selected_index else {
                            return;
                        };
                        let row = rows[index];
                        event.stop_propagation();
                        if row.branch == Branch::Open {
                            expanded.write().remove(&row.id);
                            return;
                        }
                        rows[..index]
                            .iter()
                            .rposition(|earlier| earlier.depth + 1 == row.depth)
                    }
                    other => next_index(other, selected_index, rows.len()),
                };
                event.stop_propagation();
                if let Some(row) = target.and_then(|index| rows.get(index)) {
                    selected.set(Some(row.id));
                }
            }
        };

        let breadcrumb: Element = match selected_id {
            Some(id) if snap.node(id).is_some() => {
                let crumbs = ancestors(&snap, id)
                    .into_iter()
                    .filter_map(|ancestor| {
                        let node = snap.node(ancestor)?;
                        if !show_wrappers && snap.is_wrapper(ancestor) {
                            return None;
                        }
                        Some((ancestor, node.title().to_string()))
                    })
                    .collect();
                breadcrumb(crumbs, move |picked: u64| {
                    let mut select = selected;
                    select.set(Some(picked));
                })
                .code()
                .into_element()
            }
            _ => rect().into(),
        };

        let find_bar = find_open().then(|| {
            find_bar(
                &tokens,
                query,
                hit,
                selected,
                find_open,
                &snap,
                matches.clone(),
                hit_index,
                show_wrappers,
            )
        });

        let rows_element = rows.iter().map(|row| {
            Element::from(DomRow {
                snap: snap.clone(),
                id: row.id,
                depth: row.depth,
                branch: row.branch,
                selected: selected_id == Some(row.id),
                mark: match () {
                    _ if current == Some(row.id) => Mark::Current,
                    _ if matched.contains(&row.id) => Mark::Hit,
                    _ => Mark::None,
                },
                needle: needle.clone(),
                selection: selected,
                expanded: self.expanded,
                focus_id: a11y_id,
            })
        });

        let body = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_enabled(true)
            .a11y_role(AccessibilityRole::Tree)
            .tw(&tokens, &[w_full(), h_flex(), flex_col(), grow_children()])
            .min_height(Size::px(0.))
            .on_sized(move |event: Event<SizedEventData>| viewport.set(event.area))
            .on_key_down(on_key_down)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(
                        ScrollView::new_controlled(scroll)
                            .height(Size::fill())
                            .children(rows_element),
                    ),
            )
            // `focusable_inside` returns a wrapper that sizes to its content;
            // the wrapper takes the body's flex share so the breadcrumb below
            // keeps its row.
            .focusable_inside(focus(), Radius::Card)
            .tw(&tokens, &[w_full(), h_flex(), flex_col(), grow_children()])
            .min_height(Size::px(0.));

        let mut root = rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let control_f = event.modifiers == Modifiers::CONTROL
                    && matches!(&event.key, Key::Character(c) if c.eq_ignore_ascii_case("f"));
                if control_f {
                    find_open.set(true);
                } else if *find_open.peek() && event.key == Key::Named(NamedKey::Escape) {
                    find_open.set(false);
                    query.set(String::new());
                    hit.set(0);
                }
            });
        // Freya keys hook state by position, so the find bar always holds its
        // slot; an empty slot keeps the body's scopes in place while it is shut.
        root = root.child(find_bar.unwrap_or_else(|| rect().into()));
        let mut wrappers = self.wrappers;
        let toggle = icon_button(Icon::Filter, move |_| wrappers.set(!show_wrappers))
            .size(Dim::Box7)
            .radius(Radius::Mark)
            .tooltip("Show wrappers")
            .active(show_wrappers);
        let crumbs = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S1),
                    pr(S2),
                ],
            )
            .child(rect().tw(&tokens, &[w_flex(), clip()]).child(breadcrumb))
            .child(toggle.into_element());
        // The hairline is its own box: the crumbs' surface fill would paint
        // over a border on the row.
        root.child(body)
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(crumbs)
    }
}

/// Toggles one branch open or closed.
fn toggle(open: &mut HashSet<u64>, id: u64) {
    if !open.remove(&id) {
        open.insert(id);
    }
}

/// The visible rows, depth first, skipping the children of closed branches.
/// Wrapper boxes are left out, and their children take their depth, unless
/// `wrappers` asks for them.
fn flatten(
    snap: &Snapshot,
    ids: &[u64],
    depth: usize,
    open: &HashSet<u64>,
    wrappers: bool,
    out: &mut Vec<Flat>,
) {
    for &id in ids {
        let Some(node) = snap.node(id) else {
            continue;
        };
        if !wrappers && snap.is_wrapper(id) {
            flatten(snap, &node.children, depth, open, wrappers, out);
            continue;
        }
        let branch = match (node.children.is_empty(), open.contains(&id)) {
            (true, _) => Branch::Leaf,
            (false, true) => Branch::Open,
            (false, false) => Branch::Closed,
        };
        out.push(Flat { id, depth, branch });
        if branch == Branch::Open {
            flatten(snap, &node.children, depth + 1, open, wrappers, out);
        }
    }
}

/// Scroll offset that keeps row `index` in view. The offset is negative, as
/// the scroll controller stores it.
fn offset_for(index: usize, current: i32, count: usize, viewport: f32) -> i32 {
    let row = Dim::CodeRow.px();
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

/// The row an arrow, Home or End moves to from `from`.
fn next_index(key: &NamedKey, from: Option<usize>, count: usize) -> Option<usize> {
    let last = count.checked_sub(1)?;
    match key {
        NamedKey::ArrowDown => Some(from.map_or(0, |index| (index + 1).min(last))),
        NamedKey::ArrowUp => Some(from.map_or(0, |index| index.saturating_sub(1))),
        NamedKey::Home => Some(0),
        NamedKey::End => Some(last),
        _ => None,
    }
}

/// The element and its ancestors, root first.
pub fn ancestors(snap: &Snapshot, id: u64) -> Vec<u64> {
    let mut chain = vec![id];
    let mut current = id;
    while let Some(parent) = snap.node(current).and_then(|node| node.parent) {
        chain.push(parent);
        current = parent;
    }
    chain.reverse();
    chain
}

/// Every element, in document order, that matches `text`. A match is in the
/// name, the text, the accessible label or a chip.
fn find_matches(snap: &Snapshot, text: &str, wrappers: bool) -> Vec<u64> {
    let needle = text.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut stack: Vec<u64> = snap.roots.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        let Some(node) = snap.node(id) else {
            continue;
        };
        if matches_node(node, &needle) && (wrappers || !snap.is_wrapper(id)) {
            found.push(id);
        }
        stack.extend(node.children.iter().rev().copied());
    }
    found
}

fn matches_node(node: &Node, needle: &str) -> bool {
    let line = line_of(node);
    node.kind.name().contains(needle)
        || node.title().to_lowercase().contains(needle)
        || node
            .a11y
            .name
            .as_deref()
            .is_some_and(|name| name.to_lowercase().contains(needle))
        || node
            .text
            .as_deref()
            .is_some_and(|text| text.to_lowercase().contains(needle))
        || line
            .chips
            .iter()
            .any(|chip| chip.to_lowercase().contains(needle))
}

/// One outline line: the name, the element's text and the chips that say its
/// size, layout and state.
struct OutlineLine {
    name: String,
    text: Option<String>,
    chips: Vec<String>,
}

/// The line an element shows: its name, then its text in quotes. The chips
/// carry its size, layout and state.
fn line_of(node: &Node) -> OutlineLine {
    let mut chips = Vec::new();
    match node.area {
        Some(area) => chips.push(format!(
            "{}×{}",
            area.width().round(),
            area.height().round()
        )),
        None => chips.push("hidden".to_string()),
    }
    if node.a11y.selected == Some(true) {
        chips.push("selected".to_string());
    }
    if node.a11y.focused {
        chips.push("focused".to_string());
    }
    if let Some(props) = &node.props {
        sizing_chips(props, node.repeats_fill, &mut chips);
    }
    if let Some(name) = node
        .a11y
        .name
        .as_deref()
        .filter(|name| Some(*name) != node.text.as_deref())
    {
        chips.push(format!("label \"{name}\""));
    }
    let text = node.text.as_deref().map(|text| {
        text.chars()
            .map(|c| if c == '\n' { ' ' } else { c })
            .take(TEXT_LIMIT)
            .collect()
    });
    OutlineLine {
        name: node.title().to_string(),
        text,
        chips,
    }
}

/// The layout and style values that differ from the defaults, as short chips.
fn sizing_chips(props: &Props, repeats_fill: bool, chips: &mut Vec<String>) {
    let sizes = [
        ("w", &props.width),
        ("h", &props.height),
        ("min-w", &props.min_width),
        ("min-h", &props.min_height),
        ("max-w", &props.max_width),
        ("max-h", &props.max_height),
    ];
    for (label, value) in sizes {
        if value != "auto" {
            chips.push(format!("{label} {value}"));
        }
    }
    if props.gap != 0.0 {
        chips.push(format!("gap {}", props.gap));
    }
    if props.padding.iter().any(|edge| *edge != 0.0) {
        chips.push(format!("pad {}", edge_values(props.padding)));
    }
    if props.margin.iter().any(|edge| *edge != 0.0) {
        chips.push(format!("margin {}", edge_values(props.margin)));
    }
    if props.main_align != "start" {
        chips.push(format!("main {}", props.main_align));
    }
    if props.cross_align != "start" {
        chips.push(format!("cross {}", props.cross_align));
    }
    if let Some(colour) = props
        .background
        .filter(|colour| colour.a() > 0 && !repeats_fill)
    {
        chips.push(format!("bg {}", colour_to_hex(colour)));
    }
    if props.corner_radius != 0.0 {
        chips.push(format!("radius {}", props.corner_radius));
    }
    if let Some(size) = props.font_size {
        chips.push(format!("font {size}"));
    }
    if let Some(colour) = props.text_color {
        chips.push(format!("ink {}", colour_to_hex(colour)));
    }
}

/// Four edges as one value when they match, otherwise top, right, bottom, left.
fn edge_values(values: [f32; 4]) -> String {
    if values.iter().all(|edge| *edge == values[0]) {
        return values[0].to_string();
    }
    format!("{} {} {} {}", values[0], values[1], values[2], values[3])
}

/// The line as plain text: the name, the quoted text, then the chips.
fn plain_line(line: &OutlineLine) -> String {
    let mut out = line.name.clone();
    if let Some(text) = &line.text {
        out.push_str(&format!("  \"{text}\""));
    }
    if !line.chips.is_empty() {
        out.push_str("  ");
        out.push_str(&line.chips.join(" · "));
    }
    out
}

/// The tree as outline lines, indented two spaces a level, for the clipboard.
fn serialise(snap: &Snapshot, id: u64, depth: usize, out: &mut String) {
    let Some(node) = snap.node(id) else {
        return;
    };
    out.push_str(&"  ".repeat(depth));
    out.push_str(&plain_line(&line_of(node)));
    out.push('\n');
    for &child in &node.children {
        serialise(snap, child, depth + 1, out);
    }
}

/// Opens `id` and every element below it that has children.
fn expand_all(snap: &Snapshot, id: u64, open: &mut HashSet<u64>) {
    let mut stack = vec![id];
    while let Some(next) = stack.pop() {
        let Some(node) = snap.node(next) else {
            continue;
        };
        if !node.children.is_empty() {
            open.insert(next);
            stack.extend(node.children.iter().copied());
        }
    }
}

/// Closes every element below `id`.
fn collapse_below(snap: &Snapshot, id: u64, open: &mut HashSet<u64>) {
    let mut stack: Vec<u64> = snap
        .node(id)
        .map_or_else(Vec::new, |node| node.children.clone());
    while let Some(next) = stack.pop() {
        open.remove(&next);
        if let Some(node) = snap.node(next) {
            stack.extend(node.children.iter().copied());
        }
    }
}

/// Copies `text` to the clipboard and says whether it worked.
fn copy_text(overlays: Option<Overlays>, text: String) {
    let outcome = freya::clipboard::Clipboard::set(text);
    let Some(overlays) = overlays else {
        return;
    };
    match outcome {
        Ok(()) => overlays.toast("Copied."),
        Err(_) => overlays.toast("The text could not be copied."),
    }
}

/// The menu a right click on a row opens.
fn row_menu(
    snap: Rc<Snapshot>,
    id: u64,
    expanded: State<HashSet<u64>>,
    overlays: Option<Overlays>,
) -> Vec<MenuItem> {
    let mut items = Vec::new();
    items.push({
        let snap = snap.clone();
        let mut expanded = expanded;
        menu_item("Expand recursively", move |_| {
            expand_all(&snap, id, &mut expanded.write());
        })
    });
    items.push({
        let snap = snap.clone();
        let mut expanded = expanded;
        menu_item("Collapse children", move |_| {
            collapse_below(&snap, id, &mut expanded.write());
        })
    });
    items.push(menu_separator());
    if let Some(text) = snap.node(id).and_then(|node| node.text.clone()) {
        items.push(menu_item("Copy text", move |_| {
            copy_text(overlays, text.clone());
        }));
    }
    items.push({
        let snap = snap.clone();
        menu_item("Copy element", move |_| {
            let mut copied = String::new();
            serialise(&snap, id, 0, &mut copied);
            copy_text(overlays, copied);
        })
    });
    items
}

/// One line of the tree: indent, the chevron, then the element's outline line.
#[derive(Clone)]
struct DomRow {
    snap: Rc<Snapshot>,
    id: u64,
    depth: usize,
    branch: Branch,
    selected: bool,
    mark: Mark,
    needle: String,
    selection: State<Option<u64>>,
    expanded: State<HashSet<u64>>,
    focus_id: AccessibilityId,
}

impl PartialEq for DomRow {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.snap, &other.snap)
            && self.id == other.id
            && self.depth == other.depth
            && self.branch == other.branch
            && self.selected == other.selected
            && self.mark == other.mark
            && self.needle == other.needle
            && self.focus_id == other.focus_id
    }
}

impl Component for DomRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let interaction = Interaction::use_interaction();
        let overlays = try_consume_context::<Overlays>();
        let id = self.id;
        let branch = self.branch;
        let selection = self.selection;
        let expanded = self.expanded;
        let focus_id = self.focus_id;

        let Some(node) = self.snap.node(id) else {
            return Element::from(rect());
        };

        let indent = (0..self.depth).map(|_| {
            rect().tw(
                &tokens,
                &[w(Dim::TreeIndent), h_full(), border(Line::HairlineRight)],
            )
        });
        let chevron = match branch {
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
            let mut expanded = expanded;
            let snap = self.snap.clone();
            toggle_slot = toggle_slot
                .child(icons::icon(icon, IconSize::Text, Ink::Muted.color(&tokens)))
                .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
                .on_press(move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    let mut open = expanded.write();
                    if freya_winit::modifiers::live().contains(Modifiers::ALT) {
                        // Alt-click opens or shuts the whole subtree, as Chrome does.
                        if open.contains(&id) {
                            collapse_below(&snap, id, &mut open);
                            open.remove(&id);
                        } else {
                            expand_all(&snap, id, &mut open);
                        }
                    } else {
                        toggle(&mut open, id);
                    }
                });
        }

        let outline_line = line_of(node);
        let segments = outline(&outline_line);
        let plain: String = segments.iter().map(|(text, _)| text.as_str()).collect();
        let highlight = match self.mark {
            Mark::Current => Paint::SearchCurrent,
            Mark::Hit | Mark::None => Paint::SearchHit,
        };
        let mut line_text = paragraph()
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .highlights(match_ranges(&plain, &self.needle))
            .highlight_color(highlight.color(&tokens));
        for (text, paint) in segments {
            line_text = line_text.span(CODE.span_in_paint(&tokens, text, paint));
        }
        let chips = outline_line
            .chips
            .iter()
            .map(|chip| chip_element(&tokens, chip, &self.needle, highlight));

        let line = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    w_full(),
                    gap(S1),
                    px(S2),
                    clip(),
                ],
            )
            .children(indent)
            .child(toggle_slot)
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[flex_row(), items(Align::Center), gap(S1), w_flex(), clip()],
                    )
                    .child(rect().tw(&tokens, &[clip()]).child(line_text))
                    .children(chips)
                    .children(count_badge(&tokens, node.children.len(), branch)),
            );

        let snap = self.snap.clone();
        let mut selection_state = selection;
        let mut hover = interaction.hovering;
        let mut press = interaction.pressing;
        let selected = self.selected;

        let row = rect()
            .a11y_role(AccessibilityRole::TreeItem)
            .a11y_builder(move |builder| builder.set_selected(selected))
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::CodeRow),
                    surface(
                        Paint::Transparent,
                        interaction.hovering(),
                        interaction.pressing(),
                        self.selected,
                    ),
                ],
            );
        row.child(line)
            .on_pointer_enter(move |_| {
                hover.set(true);
                super::set_hover(Some(id));
            })
            .on_pointer_leave(move |_| {
                hover.set(false);
                press.set_if_modified(false);
                super::set_hover(None);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let right = matches!(
                    event.data(),
                    PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
                );
                if right {
                    event.stop_propagation();
                    selection_state.set(Some(id));
                    if let Some(overlays) = overlays {
                        let at = event.global_location();
                        let items = row_menu(snap.clone(), id, expanded, Some(overlays));
                        overlays.show_menu((at.x as f32, at.y as f32), items);
                    }
                    return;
                }
                if !event.data().is_primary() {
                    return;
                }
                event.stop_propagation();
                press.set(true);
            })
            .on_global_pointer_up(move |_| interaction.pressing.clone().set_if_modified(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                focus_id.request_focus();
                selection_state.set(Some(id));
            })
            .into()
    }
}

/// The name and the quoted text of an element as coloured segments. Its chips
/// are drawn as separate elements beside them.
fn outline(line: &OutlineLine) -> Vec<(String, Paint)> {
    let mut out = vec![(line.name.clone(), Paint::SyntaxTag)];
    if let Some(text) = &line.text {
        out.push((format!("  \"{text}\""), Paint::SyntaxText));
    }
    out
}

/// The chip for one attribute or state value: muted type on a rounded control
/// fill, with the find bar's matches highlighted inside it.
fn chip_element(tokens: &Tokens, chip: &str, needle: &str, highlight: Paint) -> Element {
    rect()
        .tw(
            tokens,
            &[px(S1), rounded(Radius::Mark), center(), bg(Paint::Control)],
        )
        .child(
            paragraph()
                .max_lines(1)
                .highlights(match_ranges(chip, needle))
                .highlight_color(highlight.color(tokens))
                .span(CAPTION.span_in_paint(tokens, chip, Paint::Muted)),
        )
        .into()
}

/// The child count a closed branch shows at the end of its line.
fn count_badge(tokens: &Tokens, count: usize, branch: Branch) -> Vec<Element> {
    if branch != Branch::Closed {
        return Vec::new();
    }
    vec![
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
            .into(),
    ]
}

/// The character ranges of `plain` where the find needle matches, ignoring case.
fn match_ranges(plain: &str, needle: &str) -> Vec<(usize, usize)> {
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

/// The find bar: the query, the match count, previous and next, and close.
#[allow(clippy::too_many_arguments)]
fn find_bar(
    tokens: &Tokens,
    query: State<String>,
    hit: State<usize>,
    selected: State<Option<u64>>,
    find_open: State<bool>,
    snap: &Rc<Snapshot>,
    matches: Vec<u64>,
    hit_index: usize,
    show_wrappers: bool,
) -> Element {
    let count = matches.len();
    let summary = if query.read().trim().is_empty() {
        String::new()
    } else if count == 0 {
        "No matches".to_string()
    } else {
        format!("{} of {}", hit_index + 1, count)
    };
    let typed_snap = snap.clone();
    let mut hit_state = hit;
    let mut selected_state = selected;
    let mut close_query = query;
    let mut close_hit = hit;
    let mut close_open = find_open;
    let submit_matches = matches.clone();
    let prev_matches = matches.clone();
    let next_matches = matches;

    rect()
        .tw(
            tokens,
            &[
                w_full(),
                h(Dim::FlatField),
                flex_row(),
                items(Align::Center),
                gap(S1),
                px(S2),
            ],
        )
        .child(
            text_field(query, move |_: String| {
                step_match(hit, selected, &submit_matches, hit_index, 1);
            })
            .flat()
            .placeholder("Find in tree")
            .leading(Icon::Search)
            .autofocus()
            .on_change(move |text: String| {
                hit_state.set(0);
                if let Some(&first) = find_matches(&typed_snap, &text, show_wrappers).first() {
                    selected_state.set(Some(first));
                }
            }),
        )
        .child(CAPTION.in_ink(Ink::Muted).label(tokens, summary))
        .child(
            icon_button(Icon::GoPrevious, move |_| {
                step_match(hit, selected, &prev_matches, hit_index, -1);
            })
            .size(Dim::IconControl)
            .tooltip("Previous match")
            .enabled(count > 0),
        )
        .child(
            icon_button(Icon::GoNext, move |_| {
                step_match(hit, selected, &next_matches, hit_index, 1);
            })
            .size(Dim::IconControl)
            .tooltip("Next match")
            .enabled(count > 0),
        )
        .child(
            icon_button(Icon::WindowClose, move |_| {
                close_open.set(false);
                close_query.set(String::new());
                close_hit.set(0);
            })
            .size(Dim::IconControl)
            .tooltip("Close find"),
        )
        .into()
}

/// Moves to the match `delta` steps from the current one, wrapping round.
fn step_match(
    mut hit: State<usize>,
    mut selected: State<Option<u64>>,
    matches: &[u64],
    current: usize,
    delta: isize,
) {
    let count = matches.len();
    if count == 0 {
        return;
    }
    let next = (current as isize + delta).rem_euclid(count as isize) as usize;
    hit.set(next);
    selected.set(Some(matches[next]));
}

#[cfg(test)]
mod tests {
    use super::match_ranges;

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
}
