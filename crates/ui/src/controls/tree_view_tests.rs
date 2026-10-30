//! The tree view's indent guides, read from the laid-out frame.
//!
//! Each row draws a one-pixel guide per ancestor level, hung from the middle of
//! that ancestor's chevron slot. These tests take the rows, chevron slots,
//! guides, labels and badges from the frame and ask what the design asks: a
//! guide's line is on its ancestor's chevron, the guides of consecutive rows
//! join with no gap and stop with the last descendant, a leaf's label is where
//! a folder's is, the badge ends inside the row's padding, and every line is on
//! a whole device pixel. They run at desktop and phone density and at scales 1
//! and 2.

use std::collections::HashSet;
use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use super::tree_view::{TreeNode, indent_for, tree_view};
use crate::icons::Icon;
use crate::inspector::snapshot::{Kind, Snapshot as Tree};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const DESKTOP: (f32, f32) = (480.0, 640.0);
const PHONE: (f32, f32) = (390.0, 800.0);

/// What a scene shows: its nodes, the open branches and the selection.
#[derive(Clone, Copy, PartialEq)]
struct Scene {
    nodes: fn() -> Vec<TreeNode>,
    open: &'static [u64],
    selected: Option<u64>,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let open = self.open;
        let selected = self.selected;
        let expanded = use_state(move || open.iter().copied().collect::<HashSet<u64>>());
        let selected = use_state(move || selected);
        rect()
            .expanded()
            .tw(&tokens, &[p(S4)])
            .child(tree_view((self.nodes)(), expanded, selected))
    }
}

/// Five levels with a closed branch, a leaf under it and a badge on the root.
fn five_levels() -> Vec<TreeNode> {
    vec![
        TreeNode::new(1, "Window")
            .detail("1100 x 820")
            .icon(Icon::Computer)
            .badge("root", Paint::Accent)
            .children(vec![
                TreeNode::new(2, "Column").children(vec![
                    TreeNode::new(3, "Label").detail("Wi-Fi"),
                    TreeNode::new(4, "Label").detail("Bluetooth"),
                ]),
                TreeNode::new(5, "ScrollView").children(vec![TreeNode::new(6, "Group").children(
                    vec![
                        TreeNode::new(7, "Row").children(vec![
                            TreeNode::new(8, "Label").detail("Network"),
                            TreeNode::new(9, "Label").detail("Details"),
                        ]),
                        TreeNode::new(10, "Row").children(vec![TreeNode::new(11, "Label")]),
                    ],
                )]),
                TreeNode::new(12, "Footer")
                    .badge("end", Paint::Accent)
                    .children(vec![TreeNode::new(13, "Label")]),
            ]),
    ]
}

const FIVE_OPEN: &[u64] = &[1, 2, 5, 6, 7];

/// A chain of twenty boxes with a badge on the last and a long text.
fn chain() -> Vec<TreeNode> {
    let mut node = TreeNode::new(20, "Leaf")
        .detail("a label that is far too long for the room it has left")
        .badge("end", Paint::Accent);
    for id in (0..20).rev() {
        node = TreeNode::new(id, "Box").children(vec![node]);
    }
    vec![node]
}

const CHAIN_OPEN: &[u64] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19];

/// One visible row with what the tests ask about it.
#[derive(Debug)]
struct Line {
    depth: usize,
    area: Area,
    /// The whole chevron slot, present on every row.
    slot: Area,
    /// The guide boxes, one per ancestor level, outermost first.
    guides: Vec<Area>,
    /// The label's text box.
    text: Area,
    /// The badge's box, when the row has one.
    badge: Option<Area>,
    filled: bool,
}

struct Frame {
    tree: Tree,
}

impl Frame {
    fn of(shot: &mut Snapshot, scale: f32) -> Self {
        Self { tree: Tree::capture_headless(shot.runner(), scale) }
    }

    fn below(&self, id: u64, out: &mut Vec<u64>) {
        for &child in &self.tree.node(id).expect("a node").children {
            out.push(child);
            self.below(child, out);
        }
    }

    fn lines(&self) -> Vec<Line> {
        let mut rows: Vec<&crate::inspector::snapshot::Node> = self
            .tree
            .nodes
            .iter()
            .filter(|node| node.a11y.role == "TreeItem")
            .collect();
        rows.sort_by(|a, b| a.area.unwrap().min_y().total_cmp(&b.area.unwrap().min_y()));
        rows.into_iter()
            .map(|row| {
                let mut under = Vec::new();
                self.below(row.id, &mut under);
                let nodes: Vec<_> = under.iter().map(|id| self.tree.node(*id).unwrap()).collect();
                let by = |name: &str| -> Vec<Area> {
                    nodes
                        .iter()
                        .filter(|node| node.a11y.component.as_deref() == Some(name))
                        .map(|node| node.area.unwrap())
                        .collect()
                };
                let guides = by("TreeGuide");
                let slot = by("TreeChevron").pop().expect("every row has a chevron slot");
                let text = nodes
                    .iter()
                    .find(|node| node.kind == Kind::Paragraph)
                    .and_then(|node| node.area)
                    .expect("a label");
                let badge = nodes
                    .iter()
                    .find(|node| node.kind == Kind::Label)
                    .and_then(|node| node.parent)
                    .and_then(|parent| self.tree.node(parent))
                    .and_then(|pill| pill.area);
                Line {
                    depth: guides.len(),
                    area: row.area.unwrap(),
                    slot,
                    guides,
                    text,
                    badge,
                    filled: row
                        .props
                        .as_ref()
                        .and_then(|props| props.background)
                        .is_some_and(|colour| colour.a() != 0),
                }
            })
            .collect()
    }
}

fn mount(scene: Scene, size: (f32, f32), scale: f64) -> (Snapshot, Vec<Line>) {
    let mut shot = Snapshot::with_scale(move || scene.into(), size, scale);
    shot.settle(Duration::from_millis(500));
    let lines = Frame::of(&mut shot, scale as f32).lines();
    (shot, lines)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

/// Each case: a scene, the size of its window and the scale of its display.
fn cases() -> Vec<(Scene, (f32, f32), f64)> {
    let five = Scene { nodes: five_levels, open: FIVE_OPEN, selected: Some(8) };
    let chain = Scene { nodes: chain, open: CHAIN_OPEN, selected: Some(20) };
    vec![
        (five, DESKTOP, 1.0),
        (five, DESKTOP, 2.0),
        (five, PHONE, 1.0),
        (five, PHONE, 2.0),
        (chain, DESKTOP, 1.0),
        (chain, PHONE, 1.0),
    ]
}

#[test]
fn the_rows_are_the_visible_ones_and_the_closed_branches_hide_their_children() {
    let (_, lines) = mount(Scene { nodes: five_levels, open: FIVE_OPEN, selected: Some(8) }, DESKTOP, 1.0);
    // Window, Column, two labels, ScrollView, Group, Row, two labels, a closed
    // Row, and a closed Footer.
    let depths: Vec<usize> = lines.iter().map(|line| line.depth).collect();
    assert_eq!(depths, vec![0, 1, 2, 2, 1, 2, 3, 4, 4, 3, 1]);
}

#[test]
fn a_guide_runs_on_the_middle_of_its_ancestors_chevron() {
    for (scene, size, scale) in cases() {
        let (_, lines) = mount(scene, size, scale);
        // The chevron slot of the nearest row above at each depth.
        let mut ancestors: Vec<Area> = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            ancestors.truncate(line.depth);
            assert_eq!(line.guides.len(), line.depth, "row {index} has a guide per ancestor");
            for (level, guide) in line.guides.iter().enumerate() {
                let slot = ancestors[level];
                let mark = *guide;
                // The line is the pixel at or right of the slot's middle: the
                // slot's middle is a whole number, so the line covers whole
                // pixels, half a pixel off the centre of an even slot.
                let middle = slot.min_x() + slot.width() / 2.0;
                assert!(
                    mark.min_x() >= middle - 0.51 && mark.max_x() <= middle + 1.01,
                    "{size:?}@{scale} row {index} level {level}: line {:?} is not on the chevron at {middle}",
                    mark.min_x()
                );
                assert!(
                    close(mark.width(), 1.0),
                    "{size:?}@{scale} row {index} level {level}: the line is {} wide",
                    mark.width()
                );
            }
            ancestors.push(line.slot);
        }
    }
}

#[test]
fn guides_cover_the_whole_row_and_join_from_row_to_row() {
    for (scene, size, scale) in cases() {
        let (_, lines) = mount(scene, size, scale);
        for (index, pair) in lines.windows(2).enumerate() {
            let (above, below) = (&pair[0], &pair[1]);
            assert!(
                close(above.area.max_y(), below.area.min_y()),
                "{size:?}@{scale}: rows {index} and {} leave a gap",
                index + 1
            );
            // A level both rows draw continues without a break or an overlap.
            for level in 0..above.depth.min(below.depth) {
                let (a, b) = (&above.guides[level], &below.guides[level]);
                assert!(close(a.min_x(), b.min_x()), "{size:?}@{scale}: level {level} moves between rows");
                assert!(
                    close(a.max_y(), b.min_y()),
                    "{size:?}@{scale}: level {level} breaks between rows {index} and {}",
                    index + 1
                );
            }
        }
        for line in &lines {
            for guide in &line.guides {
                assert!(
                    close(guide.min_y(), line.area.min_y()) && close(guide.max_y(), line.area.max_y()),
                    "{size:?}@{scale}: a guide does not cover its row"
                );
            }
        }
    }
}

#[test]
fn a_guide_ends_at_the_bottom_of_the_last_descendant_and_no_lower() {
    for (scene, size, scale) in cases() {
        let (_, lines) = mount(scene, size, scale);
        for level in 0..21 {
            // Every row that draws this level's guide is a descendant of a row
            // at this depth: the row above the first of a run is that
            // ancestor, and the row after the run is not below it.
            let mut index = 0;
            while index < lines.len() {
                if lines[index].depth > level {
                    let first = index;
                    while index < lines.len() && lines[index].depth > level {
                        index += 1;
                    }
                    assert!(
                        first > 0 && lines[first - 1].depth == level,
                        "{size:?}@{scale}: the run starts under its ancestor"
                    );
                    let last = &lines[index - 1];
                    assert!(
                        close(last.guides[level].max_y(), last.area.max_y()),
                        "{size:?}@{scale}: level {level} ends at the last descendant's bottom"
                    );
                    if let Some(next) = lines.get(index) {
                        assert!(next.depth <= level);
                        assert!(
                            next.guides.len() <= level,
                            "{size:?}@{scale}: level {level} runs on below its last descendant"
                        );
                    }
                } else {
                    index += 1;
                }
            }
        }
    }
}

#[test]
fn rows_are_one_height_with_and_without_guides() {
    for (scene, size, scale) in cases() {
        let (_, lines) = mount(scene, size, scale);
        let class_height = lines[0].area.height();
        for line in &lines {
            assert!(
                close(line.area.height(), class_height),
                "{size:?}@{scale}: a row at depth {} is {} high, not {class_height}",
                line.depth,
                line.area.height()
            );
        }
    }
}

#[test]
fn a_leaf_label_is_where_a_folders_label_is() {
    for (scene, size, scale) in cases() {
        let (_, lines) = mount(scene, size, scale);
        // Rows at one depth, folder or leaf, start their text at one x, and the
        // text is the same distance from the chevron slot it follows.
        for depth in 1..5 {
            let at: Vec<f32> = lines
                .iter()
                .filter(|line| line.depth == depth)
                .map(|line| line.text.min_x())
                .collect();
            for x in &at {
                assert!(close(*x, at[0]), "{size:?}@{scale}: depth {depth} labels start at {at:?}");
            }
        }
        // The text follows the chevron slot at the row's own gap.
        let class = if size.0 < 400.0 { SizeClass::Compact } else { SizeClass::Regular };
        for line in lines.iter().filter(|line| line.depth > 0) {
            assert!(
                close(line.text.min_x() - line.slot.max_x(), S1.px_in(class)),
                "{size:?}@{scale}: text is {} from the chevron slot",
                line.text.min_x() - line.slot.max_x()
            );
        }
        // A deeper row's chevron slot is one level to the right of its parent's.
        let first = &lines[0];
        let second = &lines[1];
        assert!(second.slot.min_x() > first.slot.min_x());
    }
}

#[test]
fn the_badge_ends_inside_the_rows_padding_and_the_text_gives_way() {
    for (scene, size, scale) in cases() {
        let (_, lines) = mount(scene, size, scale);
        let pad = S2.px_in(if size.0 < 400.0 { SizeClass::Compact } else { SizeClass::Regular });
        for (index, line) in lines.iter().enumerate() {
            let Some(badge) = line.badge else { continue };
            assert!(
                close(badge.max_x(), line.area.max_x() - pad),
                "{size:?}@{scale}: row {index}'s badge ends at {} and the row's padding box at {}",
                badge.max_x(),
                line.area.max_x() - pad
            );
            assert!(
                line.text.max_x() <= badge.min_x() + 0.01,
                "{size:?}@{scale}: row {index}'s text runs under its badge"
            );
            assert!(badge.min_x() > line.area.min_x());
        }
        assert!(lines.iter().any(|line| line.badge.is_some()), "a row has a badge");
    }
}

#[test]
fn a_deep_tree_narrows_its_levels_and_keeps_its_label() {
    let size = PHONE;
    let (_, lines) = mount(Scene { nodes: chain, open: CHAIN_OPEN, selected: Some(20) }, size, 1.0);
    let deepest = lines.last().unwrap();
    assert_eq!(deepest.depth, 20);
    let slot = Dim::IconControl.px_in(SizeClass::Compact);
    // Narrower than a slot, but never less than the least level.
    let step = lines[2].slot.min_x() - lines[1].slot.min_x();
    assert!(step < slot && step >= S2.px_in(SizeClass::Compact), "the level is {step} wide");
    // The label still has room to read, and nothing is cut at the row's end.
    // Six slots are kept for the label run and the badge; the badge and the
    // gaps take about two and a half of them.
    assert!(deepest.text.width() >= slot * 3.0, "the label has {} px", deepest.text.width());
    assert!(deepest.slot.max_x() <= deepest.area.max_x());
    assert!(deepest.badge.unwrap().max_x() <= deepest.area.max_x());
}

#[test]
fn the_level_is_the_slot_until_a_tree_is_too_deep_for_its_view() {
    // Unmeasured, shallow and roomy views keep the full slot.
    assert_eq!(indent_for(5, 0.0, 16.0, 8.0), 16.0);
    assert_eq!(indent_for(0, 300.0, 16.0, 8.0), 16.0);
    assert_eq!(indent_for(3, 480.0, 16.0, 8.0), 16.0);
    // Too deep: whole pixels, narrowing with the depth, never under the least.
    let narrow = indent_for(20, 390.0, 18.0, 8.0);
    assert_eq!(narrow, narrow.floor());
    assert!(narrow < 18.0 && narrow >= 8.0);
    assert!(indent_for(40, 390.0, 18.0, 8.0) <= narrow);
    assert_eq!(indent_for(200, 390.0, 16.0, 8.0), 8.0);
}

#[test]
fn the_guides_are_on_whole_device_pixels_at_every_scale() {
    for (scene, size, scale) in cases() {
        let (_, lines) = mount(scene, size, scale);
        let device = |x: f32| (x * scale as f32 * 1000.0).round() / 1000.0;
        for line in &lines {
            for guide in &line.guides {
                let mark = *guide;
                assert_eq!(
                    device(mark.min_x()).fract(),
                    0.0,
                    "{size:?}@{scale}: a guide at x {} is between device pixels",
                    mark.min_x()
                );
                assert_eq!(device(mark.min_y()).fract(), 0.0);
                assert_eq!(device(mark.max_y()).fract(), 0.0);
            }
        }
    }
}

#[test]
fn the_selected_rows_fill_is_under_its_guides() {
    let (mut shot, lines) =
        mount(Scene { nodes: five_levels, open: FIVE_OPEN, selected: Some(8) }, DESKTOP, 1.0);
    // The selected row, four levels deep, has a fill and all four guides, and
    // its guides are inside the row's own box: the row paints its fill first
    // and the guides over it.
    let selected = lines.iter().find(|line| line.depth == 4 && line.filled).expect("a selected row");
    assert_eq!(selected.guides.len(), 4);
    for guide in &selected.guides {
        assert!(selected.area.contains(guide.origin) && selected.area.contains((guide.max_x() - 0.1, guide.max_y() - 0.1).into()));
    }
    // Hovering a row fills it the same way and moves nothing.
    let before: Vec<Area> = lines.iter().flat_map(|line| line.guides.clone()).collect();
    let target = lines.iter().find(|line| line.depth == 2).unwrap();
    let (x, y) = (target.area.min_x() + 60.0, target.area.min_y() + target.area.height() / 2.0);
    shot.hover(x, y).settle(Duration::from_millis(300));
    let after = Frame::of(&mut shot, 1.0).lines();
    let moved: Vec<Area> = after.iter().flat_map(|line| line.guides.clone()).collect();
    assert_eq!(before, moved);
    assert!(after.iter().filter(|line| line.filled).count() >= 2, "the hovered row is filled too");
}
