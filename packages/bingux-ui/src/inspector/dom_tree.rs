//! The Elements tree: every element of the application's snapshot as one
//! outline line, indented by depth, in the `tree_view` control. A line gives
//! the element's name (the component that built it, else its accessibility
//! role, else its layout role), its text in quotes, and muted chips for its
//! size, layout and state. A closed branch shows its child count. The arrow
//! keys move the selection, Ctrl+F opens a find bar, a breadcrumb trail shows
//! the selected element's ancestors, and a right click opens a menu.
//!
//! The selection and the open branches live in the window's state, so a
//! reveal from the picker opens the element's ancestors and scrolls to it.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use freya::prelude::*;

use super::snapshot::{Node, Props, Snapshot};
use crate::controls::{TreeNode, breadcrumb, icon_button, search_field, tree_view};
use crate::icons::Icon;
use crate::shell::menu::{MenuItem, menu_item, menu_separator};
use crate::shell::overlays::Overlays;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, Ink};

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

impl Component for DomTree {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut query = use_state(String::new);
        let mut find_open = use_state(|| false);
        let mut hit = use_state(|| 0usize);
        let selected = self.selected;
        let expanded = self.expanded;
        let overlays = try_consume_context::<Overlays>();

        let snap = self.snap.0.clone();
        let show_wrappers = *self.wrappers.read();
        let selected_id = *selected.read();

        // The tree's nodes are built once for a snapshot and a wrapper
        // setting, not for every selection or open branch.
        let cache = use_hook(|| Rc::new(RefCell::new(None::<(Shot, bool, Rc<[TreeNode]>)>)));
        let roots = {
            let mut cached = cache.borrow_mut();
            match cached.as_ref() {
                Some((shot, wrappers, nodes)) if *shot == self.snap && *wrappers == show_wrappers => {
                    nodes.clone()
                }
                _ => {
                    let nodes: Rc<[TreeNode]> = tree_nodes(&snap, &snap.roots, show_wrappers).into();
                    *cached = Some((self.snap.clone(), show_wrappers, nodes.clone()));
                    nodes
                }
            }
        };

        let needle = if find_open() {
            query.read().clone()
        } else {
            String::new()
        };
        let matches = find_matches(&snap, &needle, show_wrappers);
        let hit_index = (*hit.read()).min(matches.len().saturating_sub(1));
        let current = matches.get(hit_index).copied();

        let tree = tree_view(roots, expanded, selected)
            .code()
            .child_counts()
            .find(needle, current)
            .on_hover(EventHandler::new(|id: Option<u64>| super::set_hover(id)))
            .on_context({
                let snap = snap.clone();
                EventHandler::new(move |(id, at): (u64, (f32, f32))| {
                    if let Some(overlays) = overlays {
                        overlays.show_menu(at, row_menu(snap.clone(), id, expanded, Some(overlays)));
                    }
                })
            });

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

        let body = rect()
            .tw(&tokens, &[w_full(), h_flex(), flex_col(), grow_children()])
            .min_height(Size::px(0.))
            .child(tree);

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

/// The tree control's nodes for the elements `ids`. Wrapper boxes are left
/// out, and their children take their place, unless `wrappers` asks for them.
fn tree_nodes(snap: &Snapshot, ids: &[u64], wrappers: bool) -> Vec<TreeNode> {
    let mut out = Vec::new();
    for &id in ids {
        let Some(node) = snap.node(id) else {
            continue;
        };
        if !wrappers && snap.is_wrapper(id) {
            out.extend(tree_nodes(snap, &node.children, wrappers));
            continue;
        }
        let line = line_of(node);
        let mut tree = TreeNode::new(id, line.name)
            .paints(Paint::SyntaxTag, Paint::SyntaxText)
            .chips(line.chips)
            .children(tree_nodes(snap, &node.children, wrappers));
        if let Some(text) = line.text {
            tree = tree.detail(format!("\"{text}\""));
        }
        out.push(tree);
    }
    out
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
    node.title().to_lowercase().contains(needle)
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
    // The border box the layout measured: `area` also holds the margin.
    match node.used.border.or(node.area) {
        Some(area) => chips.push(format!(
            "{}×{}",
            area.width().round(),
            area.height().round()
        )),
        None => chips.push("hidden".to_string()),
    }
    // An error card is the place a component failed and was replaced: say so in
    // red at the front of the line, where the label chip carries the message.
    if node.a11y.component.as_deref() == Some("ErrorCard") {
        chips.insert(0, "! failed".to_string());
    }
    if node.a11y.selected == Some(true) {
        chips.push("selected".to_string());
    }
    if node.a11y.focused {
        chips.push("focused".to_string());
    }
    if node.a11y.modal {
        chips.push("modal".to_string());
    }
    if node.a11y.inert && node.a11y.focusable == Some(true) {
        // Only the stops: every box behind a modal is inert, and a chip on
        // each would bury the tree.
        chips.push("inert".to_string());
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
            search_field(query, move |text: String| {
                hit_state.set(0);
                if let Some(&first) = find_matches(&typed_snap, &text, show_wrappers).first() {
                    selected_state.set(Some(first));
                }
            })
            .flat()
            .placeholder("Find in tree")
            .autofocus()
            .on_submit(move |_: String| {
                step_match(hit, selected, &submit_matches, hit_index, 1);
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


/// Rows are laid out for real: a row with dozens of chips stays one line.
#[cfg(all(test, feature = "testing"))]
mod layout_tests {
    use std::collections::HashSet;
    use std::rc::Rc;

    use freya::prelude::*;

    use super::super::snapshot::{A11y, Kind, Node, Props, Snapshot as Tree, Used};
    use super::{DomTree, Shot};
    use crate::style::Dim;
    use crate::theme::provide_tokens;

    fn props(many: bool) -> Props {
        Props {
            width: "fill".into(),
            height: if many { "fill".into() } else { "auto".into() },
            min_width: if many { "100".into() } else { "auto".into() },
            min_height: if many { "20".into() } else { "auto".into() },
            max_width: if many { "900".into() } else { "auto".into() },
            max_height: if many { "900".into() } else { "auto".into() },
            gap: if many { 2.0 } else { 0.0 },
            padding: if many { [8.0, 8.0, 16.0, 8.0] } else { [0.0; 4] },
            margin: if many { [1.0, 2.0, 3.0, 4.0] } else { [0.0; 4] },
            main_align: if many { "space-between" } else { "start" },
            cross_align: if many { "center" } else { "start" },
            corner_radius: if many { 6.0 } else { 0.0 },
            font_size: many.then_some(13.0),
            direction: "vertical",
            ..Props::default()
        }
    }

    fn node(id: u64, parent: Option<u64>, children: Vec<u64>, many: bool) -> Node {
        Node {
            id,
            parent,
            kind: Kind::Rect,
            text: None,
            area: Some(torin::prelude::Area::new(
                (0.0, 0.0).into(),
                (280.0, 774.0).into(),
            )),
            used: Used::default(),
            children,
            props: Some(props(many)),
            a11y: A11y {
                name: many.then(|| "a long accessible label that is not the text".to_string()),
                role: String::new(),
                ..A11y::default()
            },
            repeats_fill: false,
        }
    }

    #[derive(PartialEq)]
    struct Host {
        snap: Shot,
    }

    impl Component for Host {
        fn render(&self) -> impl IntoElement {
            let _tokens = provide_tokens();
            let expanded = use_state(|| HashSet::from([1, 2, 3]));
            let selected = use_state(|| Some(4));
            let wrappers = use_state(|| true);
            rect()
                .expanded()
                .child(DomTree {
                    snap: self.snap.clone(),
                    selected,
                    expanded,
                    wrappers,
                })
        }
    }

    /// A chain of four boxes; the fourth, a closed branch at depth 3, has more
    /// chips than any width shows.
    fn chain() -> Shot {
        let nodes = vec![
            node(1, None, vec![2], false),
            node(2, Some(1), vec![3], false),
            node(3, Some(2), vec![4], false),
            node(4, Some(3), (5..23).collect(), true),
        ]
        .into_iter()
        .chain((5..23).map(|id| node(id, Some(4), vec![], false)))
        .collect();
        Shot(Rc::new(Tree::from_nodes(vec![1], nodes)))
    }

    /// Every row is one line: no box of a row leaves the row's vertical
    /// extent, and the chevron slot is centred on it.
    #[test]
    fn a_row_with_many_chips_stays_one_centred_line() {
        // The host has no WindowHost, so the tree renders at the regular class.
        let row = Dim::CodeRow.px_in(crate::style::SizeClass::Regular);
        let slot = Dim::IconControl.px_in(crate::style::SizeClass::Regular);
        for width in [260.0_f32, 420.0, 900.0] {
            let snap = chain();
            let mut shot = crate::snapshot::Snapshot::new(
                move || Host { snap: snap.clone() }.into(),
                (width, 300.0),
            );
            shot.settle(std::time::Duration::from_millis(300));
            if let Ok(dir) = std::env::var("BINGUX_TREE_SHOTS") {
                std::fs::create_dir_all(&dir).unwrap();
                shot.save(format!("{dir}/rows-{width}.png"));
            }
            // The chevron slots: the only boxes exactly an icon control wide
            // and a row tall.
            let slots = shot.runner().find_many(|node, _| {
                let area = node.layout().area;
                ((area.width() - slot).abs() < 0.01 && (area.height() - row).abs() < 0.01)
                    .then_some((area.min_y(), area.height()))
            });
            assert!(
                slots.len() >= 4,
                "width {width}: expected a slot per row, found {}",
                slots.len()
            );
            for (top, height) in &slots {
                // Every slot starts on a row boundary and is one row tall.
                let into_row = (top / row).fract();
                assert!(
                    into_row < 0.01 || into_row > 0.99,
                    "width {width}: a chevron slot at y {top} is not on a row boundary"
                );
                assert!((height - row).abs() < 0.01, "slot is {height} tall");
            }
        }
    }
}
