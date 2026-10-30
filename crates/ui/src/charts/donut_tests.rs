//! The donut and the ring gauge at phone widths.
//!
//! Every case puts the chart in a card on a page `width` px wide, as a Settings
//! page or a taxonomy specimen does, renders it in a window of that width (390,
//! 360 and 320 px, so the window is at the compact density) and reads the boxes
//! the layout resolved: the ring is a square as wide as the card allows, up to
//! its maximum, and centred; its centre text is inside its hole; the legend is
//! below it, as wide as the card's content, with rows a finger can hit; and
//! nothing is wider than the card.
//!
//! A tap or a hover on a segment, or on its legend row, shows that segment's
//! value in the centre.
//!
//! `BINGUX_DONUT_PNG=DIR` also writes a PNG of each case to DIR,
//! to look at the frame (`BINGUX_TEXT_SCALE=2` renders it at the largest text
//! scale; `BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-light` in the light scheme).

use std::time::Duration;

use freya::prelude::*;

use super::donut::DonutLayout;
use super::geometry::{DONUT_MAX, DONUT_SIZE, RING_SIZE, RING_STROKE, DONUT_HOLE_MARGIN};
use super::{Segment, SeriesColour, Unit, donut, ring};
use crate::inspector::snapshot::{Kind, Node, Snapshot as Tree};
use crate::layout::boxed;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const HEIGHT: f32 = 900.0;
/// Phone widths: the preview's, a small phone's and the narrowest a popup is.
const PHONES: [f32; 3] = [390.0, 360.0, 320.0];
const EPS: f32 = 0.6;

/// A shape of data to draw.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Data {
    /// Three segments with a caption: the taxonomy specimen.
    Storage,
    /// Long names and large values.
    Long,
    /// Seven segments.
    Many,
    /// One segment, no caption, percent values.
    Single,
    /// No data.
    Empty,
}

const ALL: [Data; 5] = [Data::Storage, Data::Long, Data::Many, Data::Single, Data::Empty];

fn gib(value: f64) -> f64 {
    value * 1024.0 * 1024.0 * 1024.0
}

fn chart(data: Data) -> super::Donut {
    match data {
        Data::Storage => donut([
            Segment::new("Apps", gib(210.0)),
            Segment::new("System", gib(90.0)),
            Segment::new("Cache", gib(45.0)),
        ])
        .unit(Unit::Bytes)
        .caption("Used")
        .label("Storage used by category"),
        Data::Long => donut([
            Segment::new("Applications and games", gib(812.0)),
            Segment::new("Documents, downloads and desktop", gib(1204.0)),
            Segment::new("Operating system", gib(95.0)),
        ])
        .unit(Unit::Bytes)
        .caption("Used of 2 TB")
        .label("Storage used by category"),
        Data::Many => donut(
            ["Apps", "System", "Cache", "Photos", "Music", "Video", "Other"]
                .into_iter()
                .zip([210.0, 90.0, 45.0, 30.0, 22.0, 18.0, 5.0])
                .map(|(name, value)| Segment::new(name, gib(value))),
        )
        .unit(Unit::Bytes)
        .caption("Used")
        .label("Storage used by category"),
        Data::Single => donut([Segment::new("Battery", 82.0)])
            .unit(Unit::Percent)
            .label("Battery"),
        Data::Empty => donut(Vec::<Segment>::new()).label("Storage used by category"),
    }
}

/// A page with one card holding the donut, then one holding a ring gauge.
#[derive(PartialEq, Clone, Copy)]
struct Page {
    data: Data,
}

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let data = self.data;
        WindowHost::new(
            rect()
                .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg), p(S4), gap(S4)])
                .child(boxed(&tokens).child(chart(data)))
                .child(
                    boxed(&tokens).child(
                        ring(0.62)
                            .caption("Charged")
                            .colour(SeriesColour::Success)
                            .label("Battery"),
                    ),
                ),
        )
    }
}

fn mount(data: Data, width: f32) -> Snapshot {
    let mut shot = Snapshot::new(move || Page { data }.into_element(), (width, HEIGHT));
    shot.settle(Duration::from_millis(900));
    shot
}

fn tree(shot: &mut Snapshot) -> Tree {
    Tree::capture_headless(shot.runner(), 1.0)
}

/// The frame of `data` in a window `width` px wide. With `BINGUX_DONUT_PNG`
/// set the frame is saved too (and, with `BINGUX_UI_LAYOUT_LINT=FILE` and
/// `BINGUX_UI_LAYOUT_TREE=1`, its tree beside it).
fn frame(data: Data, width: f32) -> Tree {
    let mut shot = mount(data, width);
    let tree = tree(&mut shot);
    if let Some(dir) = std::env::var_os("BINGUX_DONUT_PNG") {
        let dir = std::path::PathBuf::from(dir);
        let name = format!("{data:?}-{width}").to_lowercase();
        shot.save(dir.join(format!("{name}.png")));
    }
    tree
}

/// A node's border box as `[left, top, right, bottom]`.
fn rect_of(node: &Node) -> [f32; 4] {
    let area = node.used.border.or(node.area).expect("a laid-out node");
    [area.min_x(), area.min_y(), area.max_x(), area.max_y()]
}

fn wide(rect: [f32; 4]) -> f32 {
    rect[2] - rect[0]
}

fn tall(rect: [f32; 4]) -> f32 {
    rect[3] - rect[1]
}

fn child<'a>(tree: &'a Tree, node: &Node, index: usize) -> &'a Node {
    tree.node(node.children[index]).expect("a child in the tree")
}

fn descendants<'a>(tree: &'a Tree, node: &Node, into: &mut Vec<&'a Node>) {
    for &id in &node.children {
        let next = tree.node(id).expect("a child in the tree");
        into.push(next);
        descendants(tree, next, into);
    }
}

/// A chart in the page: the card around it, its ring's box (the column that
/// holds the drawing and the centre text) and the centre text's labels.
struct Found<'a> {
    card: &'a Node,
    ring: &'a Node,
    labels: Vec<&'a Node>,
}

/// Every ring and gauge of the frame, in page order: the donut, then the gauge.
fn rings(tree: &Tree) -> Vec<Found<'_>> {
    tree.nodes
        .iter()
        .filter(|node| {
            // The ring's box holds its drawing (an element), then the text.
            let Some(&first) = node.children.first() else {
                return false;
            };
            let drawing = tree.node(first).expect("a child in the tree");
            let (Some(ring), Some(drawing_area)) = (node.used.border.or(node.area), drawing.used.border.or(drawing.area)) else {
                return false;
            };
            drawing.kind == Kind::Other
                && drawing_area.width() >= 40.0
                && (ring.width() - ring.height()).abs() < EPS
        })
        .map(|ring| {
            let mut below = Vec::new();
            descendants(tree, ring, &mut below);
            let labels = below.into_iter().filter(|node| node.kind == Kind::Label).collect();
            // The donut sits in a card of its own, directly or in its own box.
            let mut card = tree.node(ring.parent.expect("a parent")).expect("a parent");
            while card.used.overflow != Some("clip") {
                card = tree.node(card.parent.expect("a card above")).expect("a parent");
            }
            Found { card, ring, labels }
        })
        .collect()
}

/// Where a label's text is drawn: centred in its slot, `measured` wide.
fn painted(tree: &Tree, label: &Node) -> [f32; 4] {
    let slot = rect_of(tree.node(label.parent.expect("a slot")).expect("a slot"));
    let (width, height) = label.used.text.as_ref().and_then(|text| text.measured).expect("a measured label");
    let middle = (slot[0] + slot[2]) / 2.0;
    let top = rect_of(label)[1];
    [middle - width / 2.0, top, middle + width / 2.0, top + height]
}

fn inside(inner: [f32; 4], outer: [f32; 4]) -> bool {
    inner[0] >= outer[0] - EPS && inner[1] >= outer[1] - EPS && inner[2] <= outer[2] + EPS && inner[3] <= outer[3] + EPS
}

fn compact_pad() -> f32 {
    S4.px_in(SizeClass::Compact)
}

#[test]
fn dump_every_case() {
    // Runs only to write the PNGs when asked to.
    if std::env::var_os("BINGUX_DONUT_PNG").is_none() {
        return;
    }
    for data in ALL {
        for width in [390.0, 360.0, 320.0, 800.0] {
            frame(data, width);
        }
    }
}

/// The donut keeps its aspect ratio at every phone width: a square, as wide as
/// the card's content up to its maximum, and centred.
#[test]
fn the_ring_is_a_centred_square_at_phone_widths() {
    for data in ALL {
        for width in PHONES {
            let tree = frame(data, width);
            let found = &rings(&tree)[0];
            let ring = rect_of(found.ring);
            let card = rect_of(found.card);
            let inner = wide(card) - 2.0 * compact_pad();
            assert!((wide(ring) - tall(ring)).abs() < EPS, "{data:?} at {width}: {ring:?} is not square");
            assert!(
                (wide(ring) - inner.min(DONUT_MAX)).abs() < EPS,
                "{data:?} at {width}: the ring is {} px for {inner} px of room",
                wide(ring)
            );
            let (middle, card_middle) = ((ring[0] + ring[2]) / 2.0, (card[0] + card[2]) / 2.0);
            assert!((middle - card_middle).abs() < EPS, "{data:?} at {width}: centred at {middle}, the card at {card_middle}");
            assert!(inside(ring, card), "{data:?} at {width}: the ring {ring:?} is inside the card {card:?}");
        }
    }
}

/// The centre text is inside the hole, whatever the data and the width: all
/// four corners of what is painted are inside the circle the ring leaves free.
#[test]
fn the_centre_text_is_inside_the_hole() {
    for data in ALL {
        for width in PHONES {
            let tree = frame(data, width);
            let found = rings(&tree);
            let donut = &found[0];
            let ring = rect_of(donut.ring);
            let layout = DonutLayout::new(wide(rect_of(donut.card)) - 2.0 * compact_pad(), true, S4.px_in(SizeClass::Compact), 1.0);
            assert!((wide(ring) - layout.size).abs() < EPS);
            assert!(!donut.labels.is_empty(), "{data:?} at {width}: the centre reads something");
            let radius = layout.hole() / 2.0;
            let centre = ((ring[0] + ring[2]) / 2.0, (ring[1] + ring[3]) / 2.0);
            for label in &donut.labels {
                let text = painted(&tree, label);
                for corner in [(text[0], text[1]), (text[2], text[1]), (text[0], text[3]), (text[2], text[3])] {
                    let distance = (corner.0 - centre.0).hypot(corner.1 - centre.1);
                    assert!(
                        distance <= radius + EPS,
                        "{data:?} at {width}: {:?} reaches {distance} px from the middle of a hole {radius} px across",
                        label.text
                    );
                }
                assert_eq!(label.used.text.as_ref().and_then(|text| text.lines), Some(1), "{:?} is one line", label.text);
            }

            // The gauge beside it: its text is inside its own hole.
            let gauge = &found[1];
            let ring = rect_of(gauge.ring);
            let radius = (RING_SIZE - (RING_STROKE + DONUT_HOLE_MARGIN) * 2.0) / 2.0;
            let centre = ((ring[0] + ring[2]) / 2.0, (ring[1] + ring[3]) / 2.0);
            for label in &gauge.labels {
                let text = painted(&tree, label);
                for corner in [(text[0], text[1]), (text[2], text[3])] {
                    assert!((corner.0 - centre.0).hypot(corner.1 - centre.1) <= radius + EPS, "{:?} is in the gauge's hole", label.text);
                }
            }
        }
    }
}

/// The legend is below the ring, the card's content wide (the same horizontal
/// padding as every card), and nothing in the card is wider than the card or
/// the window.
#[test]
fn the_legend_is_inside_the_card_and_nothing_overflows() {
    for data in ALL {
        for width in PHONES {
            let tree = frame(data, width);
            let found = &rings(&tree)[0];
            let card = rect_of(found.card);
            let ring = rect_of(found.ring);
            let pad = compact_pad();
            let outer = tree.node(found.ring.parent.expect("a parent")).expect("a parent");
            let legend = child(&tree, outer, 1);
            let legend_rect = rect_of(legend);
            assert!(legend_rect[1] >= ring[3] - EPS, "{data:?} at {width}: the legend is below the ring");
            assert!(
                (legend_rect[0] - (card[0] + pad)).abs() < EPS && (legend_rect[2] - (card[2] - pad)).abs() < EPS,
                "{data:?} at {width}: the legend {legend_rect:?} has the card's padding {pad} in {card:?}"
            );

            let mut below = Vec::new();
            descendants(&tree, tree.node(tree.roots[0]).expect("a root"), &mut below);
            for node in below {
                if node.used.hidden || node.area.is_none() {
                    continue;
                }
                let area = rect_of(node);
                assert!(area[2] <= width + EPS && area[0] >= -EPS, "{data:?} at {width}: {:?} {area:?} sticks out of the window", node.text);
            }
            let mut inner = Vec::new();
            descendants(&tree, found.card, &mut inner);
            for node in inner {
                assert!(
                    inside([rect_of(node)[0], card[1], rect_of(node)[2], card[3]], card),
                    "{data:?} at {width}: {:?} {:?} sticks out of the card {card:?}",
                    node.text,
                    rect_of(node)
                );
            }

            // Rows: a finger's height, the numbers on one line each, the name
            // on at most two, and the name never over the numbers.
            for &id in &legend.children {
                let row = tree.node(id).expect("a row");
                assert!(tall(rect_of(row)) >= TOUCH_MIN - EPS, "{data:?} at {width}: a row is {} px", tall(rect_of(row)));
                let name = child(&tree, child(&tree, row, 0), 1);
                let numbers = child(&tree, row, 1);
                assert!(name.used.text.as_ref().and_then(|text| text.lines).is_some_and(|lines| lines <= 2));
                assert!(rect_of(name)[2] <= rect_of(numbers)[0] + EPS, "{data:?} at {width}: {:?} reaches the numbers", name.text);
                for &id in &numbers.children {
                    let label = tree.node(id).expect("a number");
                    assert_eq!(label.used.text.as_ref().and_then(|text| text.lines), Some(1), "{:?} is one line", label.text);
                }
            }
        }
    }
}

/// On a desktop width the legend sits beside the ring at its regular size and
/// the card keeps the same one-token horizontal padding.
#[test]
fn beside_the_legend_on_a_desktop_width() {
    let tree = frame(Data::Long, 800.0);
    let found = &rings(&tree)[0];
    let (card, ring) = (rect_of(found.card), rect_of(found.ring));
    let pad = S4.px_in(SizeClass::Regular);
    assert!((wide(ring) - DONUT_SIZE).abs() < EPS && (tall(ring) - DONUT_SIZE).abs() < EPS);
    assert!((ring[0] - (card[0] + pad)).abs() < EPS, "the ring is {pad} px from the card's edge");
    let outer = tree.node(found.ring.parent.expect("a parent")).expect("a parent");
    let legend = rect_of(child(&tree, outer, 1));
    assert!(legend[0] >= ring[2] + pad - EPS, "the legend is beside the ring, {pad} px away");
    assert!((legend[2] - (card[2] - pad)).abs() < EPS);
}

/// What the centre of the donut reads now: its labels, top to bottom.
fn centre(shot: &mut Snapshot) -> Vec<String> {
    let tree = tree(shot);
    rings(&tree)[0].labels.iter().filter_map(|label| label.text.clone()).collect()
}

/// The point on the ring's band at `turn` of a full turn clockwise from twelve
/// o'clock, in window pixels.
fn on_the_ring(tree: &Tree, turn: f32) -> (f32, f32) {
    let ring = rect_of(rings(tree)[0].ring);
    let size = wide(ring);
    let layout = DonutLayout::new(size, true, S4.px_in(SizeClass::Compact), 1.0);
    let radius = size / 2.0 - layout.stroke / 2.0;
    let angle = turn * std::f32::consts::TAU;
    (ring[0] + size / 2.0 + radius * angle.sin(), ring[1] + size / 2.0 - radius * angle.cos())
}

/// A finger selects a segment by tapping it, or its row, and the centre reads
/// the segment; a second tap, or a tap in the hole, gives it up; a swipe that
/// starts on the ring selects nothing.
#[test]
fn a_tap_selects_a_segment_and_a_swipe_does_not() {
    let mut shot = mount(Data::Storage, 390.0);
    let total = vec!["345 GiB".to_string(), "Used".to_string()];
    assert_eq!(centre(&mut shot), total);

    // The ring: 210 of 345 is the first 61 % of the turn, then 90, then 45.
    let first = tree(&mut shot);
    let (x, y) = on_the_ring(&first, 0.3);
    shot.touch(x, y).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), vec!["210 GiB".to_string(), "60.9%".to_string()], "the first segment");
    let (x, y) = on_the_ring(&first, 0.75);
    shot.touch(x, y).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), vec!["90 GiB".to_string(), "26.1%".to_string()], "the second segment");
    shot.touch(x, y).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), total, "a second tap gives the selection up");

    // A legend row: the whole row, a finger's height, selects its segment.
    let rows = tree(&mut shot);
    let cache = rows
        .nodes
        .iter()
        .find(|node| node.kind == Kind::Paragraph && node.text.as_deref() == Some("Cache"))
        .expect("the Cache row");
    let at = rect_of(cache);
    shot.touch((at[0] + at[2]) / 2.0, (at[1] + at[3]) / 2.0).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), vec!["45 GiB".to_string(), "13.0%".to_string()], "the Cache row");
    let hole = rect_of(rings(&rows)[0].ring);
    shot.touch((hole[0] + hole[2]) / 2.0, (hole[1] + hole[3]) / 2.0).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), total, "a tap in the hole clears it");

    // A swipe over the ring, as a scroll is: nothing is selected.
    let (x, y) = on_the_ring(&first, 0.3);
    shot.swipe((x, y), (x, y + 80.0)).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), total, "a swipe selects nothing");
}

/// A mouse shows a segment by hovering it, and takes it away on leaving.
#[test]
fn hovering_a_segment_or_its_row_shows_it() {
    let mut shot = mount(Data::Storage, 390.0);
    let first = tree(&mut shot);
    let total = vec!["345 GiB".to_string(), "Used".to_string()];
    let (x, y) = on_the_ring(&first, 0.75);
    shot.hover(x, y).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), vec!["90 GiB".to_string(), "26.1%".to_string()]);
    shot.hover(2.0, HEIGHT - 2.0).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), total, "leaving puts the total back");

    let system = first
        .nodes
        .iter()
        .find(|node| node.kind == Kind::Paragraph && node.text.as_deref() == Some("System"))
        .expect("the System row");
    let at = rect_of(system);
    shot.hover((at[0] + at[2]) / 2.0, (at[1] + at[3]) / 2.0).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), vec!["90 GiB".to_string(), "26.1%".to_string()]);
    shot.hover(2.0, HEIGHT - 2.0).settle(Duration::from_millis(100));
    assert_eq!(centre(&mut shot), total);
}
