//! A known-bad and a clean fixture for each rule of `checks`, built as hand
//! made trees with the boxes the layout would give, and the tables' own
//! invariants (every exemption and every pill has its reason, every rule has
//! its IDs).

use freya::torin::prelude::{Area, Point2D, Size2D};

use super::super::{Finding, Rule, check};
use super::{EXEMPT, RULES, meta};
use crate::inspector::snapshot::{Kind, Node, Props, Snapshot, UsedText};

#[derive(Default)]
struct Tree {
    nodes: Vec<Node>,
}

type Rect = (f32, f32, f32, f32);

impl Tree {
    fn rect(&mut self, parent: Option<u64>, (x, y, w, h): Rect, component: &str) -> u64 {
        let id = self.nodes.len() as u64 + 1;
        let area = Area::new(Point2D::new(x, y), Size2D::new(w, h));
        let mut node = Node {
            id,
            parent,
            kind: Kind::Rect,
            text: None,
            area: Some(area),
            used: Default::default(),
            children: Vec::new(),
            props: Some(Props {
                direction: "horizontal",
                ..Props::default()
            }),
            a11y: Default::default(),
            repeats_fill: false,
        };
        node.used.border = node.area;
        node.used.set_padding([0.0; 4]);
        node.used.inner_sizes = Some((w, h));
        if !component.is_empty() {
            node.a11y.component = Some(component.to_owned());
        }
        self.nodes.push(node);
        if let Some(parent) = parent {
            self.nodes[parent as usize - 1].children.push(id);
        }
        id
    }

    fn at(&mut self, id: u64) -> &mut Node {
        &mut self.nodes[id as usize - 1]
    }

    /// A box with a fill and corners of `radius`.
    fn paint(&mut self, id: u64, radius: f32) -> u64 {
        let props = self.at(id).props.as_mut().unwrap();
        props.background = Some(0xFF28_2828_u32.into());
        props.corner_radii = [radius; 4];
        props.corner_radius = radius;
        id
    }

    fn focusable(&mut self, id: u64) -> u64 {
        self.at(id).a11y.focusable = Some(true);
        id
    }

    fn pad(&mut self, id: u64, padding: [f32; 4]) -> u64 {
        self.at(id).used.set_padding(padding);
        id
    }

    fn icon(&mut self, parent: u64, rect: Rect) -> u64 {
        let id = self.rect(Some(parent), rect, "");
        self.at(id).kind = Kind::Other;
        id
    }

    fn text(&mut self, parent: u64, text: &str, rect: Rect) -> u64 {
        let id = self.rect(Some(parent), rect, "");
        let node = self.at(id);
        node.kind = Kind::Label;
        node.text = Some(text.to_owned());
        node.used.text = Some(UsedText {
            families: Vec::new(),
            size: 14.0,
            weight: 400,
            slant: "upright",
            width: 5,
            letter_spacing: 0.0,
            colour: None,
            align: "left",
            lines: Some(1),
            measured: Some((rect.2, rect.3)),
            intrinsic: Some(rect.2),
            wrap: Default::default(),
        });
        id
    }

    fn snapshot(self) -> Snapshot {
        Snapshot::from_nodes(vec![1], self.nodes)
    }
}

fn found(snapshot: &Snapshot, rule: Rule) -> Vec<Finding> {
    check(snapshot).into_iter().filter(|finding| finding.rule == rule).collect()
}

fn window(tree: &mut Tree, width: f32) -> u64 {
    tree.rect(None, (0.0, 0.0, width, 800.0), "Window")
}

// SP-01: inset asymmetry.

fn card(top_gap: f32, bottom_gap: f32) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let group = tree.rect(Some(root), (20.0, 20.0, 400.0, 100.0), "Group");
    tree.at(group).props.as_mut().unwrap().direction = "vertical";
    // The card a group paints round its rows.
    let card = tree.rect(Some(group), (20.0, 20.0, 400.0, 100.0), "");
    tree.paint(card, 16.0);
    tree.at(card).props.as_mut().unwrap().direction = "vertical";
    let content = tree.rect(Some(card), (20.0, 20.0 + top_gap, 400.0, 100.0 - top_gap - bottom_gap), "");
    tree.paint(content, 0.0);
    tree.snapshot()
}

#[test]
fn a_card_with_more_room_below_its_content_than_above_is_found() {
    let findings = found(&card(16.0, 28.0), Rule::InsetAsymmetry);
    let [finding] = findings.as_slice() else {
        panic!("expected one finding, got {findings:?}");
    };
    assert!(finding.what.starts_with("[SP-01]"), "{}", finding.what);
    assert!(finding.numbers.contains(&("inset-top", 16.0)));
    assert!(finding.numbers.contains(&("inset-bottom", 28.0)));
}

#[test]
fn a_card_with_the_same_room_above_and_below_is_clean() {
    assert!(found(&card(16.0, 16.0), Rule::InsetAsymmetry).is_empty());
    assert!(found(&card(0.0, 0.0), Rule::InsetAsymmetry).is_empty());
}

fn value_row(end_gap: f32) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let row = tree.rect(Some(root), (0.0, 0.0, 400.0, 56.0), "Row");
    tree.text(row, "Name", (16.0, 18.0, 80.0, 20.0));
    tree.text(row, "Value", (400.0 - end_gap - 60.0, 18.0, 60.0, 20.0));
    tree.snapshot()
}

#[test]
fn a_row_whose_value_is_nearer_the_end_than_its_label_is_the_start_is_found() {
    let findings = found(&value_row(28.0), Rule::InsetAsymmetry);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].numbers.contains(&("inset-start", 16.0)));
    assert!(findings[0].numbers.contains(&("inset-end", 28.0)));
}

#[test]
fn a_row_with_the_same_room_at_both_ends_is_clean() {
    assert!(found(&value_row(16.0), Rule::InsetAsymmetry).is_empty());
}

#[test]
fn a_row_that_ends_in_a_control_is_left_to_the_trailing_inset() {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let row = tree.rect(Some(root), (0.0, 0.0, 400.0, 56.0), "Row");
    tree.text(row, "Name", (16.0, 18.0, 80.0, 20.0));
    let button = tree.rect(Some(row), (352.0, 8.0, 40.0, 40.0), "RowButton");
    tree.focusable(button);
    let snapshot = tree.snapshot();
    assert!(found(&snapshot, Rule::InsetAsymmetry).is_empty());
    assert!(found(&snapshot, Rule::TrailingInset).is_empty());
}

// SP-02: trailing inset.

fn button_row(x: f32, y: f32, size: f32) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let row = tree.rect(Some(root), (0.0, 0.0, 400.0, 56.0), "Row");
    tree.text(row, "Name", (16.0, 18.0, 80.0, 20.0));
    let button = tree.rect(Some(row), (x, y, size, size), "RowButton");
    tree.focusable(button);
    tree.snapshot()
}

#[test]
fn a_button_nearer_the_end_than_the_top_is_found() {
    // 40 tall in a 56 row: 8 above and below, but 16 from the end.
    let findings = found(&button_row(344.0, 8.0, 40.0), Rule::TrailingInset);
    let [finding] = findings.as_slice() else {
        panic!("expected one finding, got {findings:?}");
    };
    assert!(finding.what.starts_with("[SP-02, AL-09, SP-12, SZ-12]"), "{}", finding.what);
    assert!(finding.numbers.contains(&("inset-end", 16.0)));
    assert!(finding.numbers.contains(&("inset-top", 8.0)));
}

#[test]
fn a_button_held_one_inset_from_three_edges_is_clean() {
    assert!(found(&button_row(352.0, 8.0, 40.0), Rule::TrailingInset).is_empty());
}

#[test]
fn a_flush_full_height_zone_is_clean() {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let row = tree.rect(Some(root), (0.0, 0.0, 400.0, 56.0), "Row");
    let zone = tree.rect(Some(row), (356.0, 0.0, 44.0, 56.0), "RowChevron");
    tree.focusable(zone);
    assert!(found(&tree.snapshot(), Rule::TrailingInset).is_empty());
}

#[test]
fn a_header_is_exempt_with_its_reason() {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let bar = tree.rect(Some(root), (0.0, 0.0, 400.0, 48.0), "HeaderBar");
    let button = tree.rect(Some(bar), (356.0, 6.0, 36.0, 36.0), "IconButton");
    tree.focusable(button);
    assert!(found(&tree.snapshot(), Rule::TrailingInset).is_empty());
}

#[test]
fn a_field_or_slider_in_a_row_keeps_the_gutter_and_is_not_a_button() {
    for control in ["TextField", "Slider"] {
        let mut tree = Tree::default();
        let root = window(&mut tree, 1000.0);
        let row = tree.rect(Some(root), (0.0, 0.0, 400.0, 56.0), "Row");
        let field = tree.rect(Some(row), (200.0, 10.0, 184.0, 36.0), control);
        let inner = tree.rect(Some(field), (200.0, 10.0, 184.0, 36.0), "");
        tree.focusable(inner);
        assert!(found(&tree.snapshot(), Rule::TrailingInset).is_empty(), "{control}");
    }
}

#[test]
fn a_fields_own_trailing_button_is_held_to_one_inset() {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let field = tree.rect(Some(root), (0.0, 0.0, 240.0, 36.0), "TextInput");
    let button = tree.rect(Some(field), (204.0, 4.0, 28.0, 28.0), "IconButton");
    tree.focusable(button);
    // 4 above and below, 8 from the end.
    let findings = found(&tree.snapshot(), Rule::TrailingInset);
    assert_eq!(findings.len(), 1, "{findings:?}");
}

#[test]
fn a_button_centred_in_a_tall_row_is_held_to_its_vertical_centring_only() {
    // A 63 px row (a subtitle) holds a 40 px button 11.5 px from the top and
    // bottom and 8 from the end: the end cannot be 11.5 on the scale.
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let row = tree.rect(Some(root), (0.0, 0.0, 400.0, 63.0), "Row");
    let button = tree.rect(Some(row), (352.0, 11.5, 40.0, 40.0), "RowButton");
    tree.focusable(button);
    assert!(found(&tree.snapshot(), Rule::TrailingInset).is_empty());
    // Off the vertical centre it is found.
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let row = tree.rect(Some(root), (0.0, 0.0, 400.0, 63.0), "Row");
    let button = tree.rect(Some(row), (352.0, 4.0, 40.0, 40.0), "RowButton");
    tree.focusable(button);
    assert_eq!(found(&tree.snapshot(), Rule::TrailingInset).len(), 1);
}

// AL-01, SZ-05: columns.

fn sidebar(icon_x: [f32; 3]) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    for (at, x) in icon_x.into_iter().enumerate() {
        let y = at as f32 * 40.0;
        let row = tree.rect(Some(root), (8.0, y, 264.0, 40.0), "SidebarRow");
        tree.icon(row, (x, y + 11.0, 18.0, 18.0));
        tree.text(row, "Item", (48.0, y + 11.0, 60.0, 18.0));
    }
    tree.snapshot()
}

#[test]
fn a_row_whose_icon_is_off_its_column_is_found() {
    let findings = found(&sidebar([19.0, 19.0, 24.0]), Rule::ColumnAligned);
    let [finding] = findings.as_slice() else {
        panic!("expected one finding, got {findings:?}");
    };
    assert!(finding.what.starts_with("[AL-01, SZ-05]"), "{}", finding.what);
    assert!(finding.numbers.contains(&("icon-x", 24.0)));
}

#[test]
fn rows_with_one_icon_column_are_clean() {
    assert!(found(&sidebar([19.0, 19.0, 19.0]), Rule::ColumnAligned).is_empty());
}

fn dropdowns(widths: [f32; 2]) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let group = tree.rect(Some(root), (0.0, 0.0, 400.0, 120.0), "Group");
    tree.paint(group, 16.0);
    for (at, width) in widths.into_iter().enumerate() {
        tree.rect(Some(group), (400.0 - 8.0 - width, 8.0 + at as f32 * 56.0, width, 40.0), "Dropdown");
    }
    tree.snapshot()
}

#[test]
fn dropdowns_of_one_card_at_different_widths_are_found() {
    let findings = found(&dropdowns([180.0, 120.0]), Rule::ColumnAligned);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].what.contains("one width"), "{}", findings[0].what);
    assert!(found(&dropdowns([180.0, 180.0]), Rule::ColumnAligned).is_empty());
}

#[test]
fn rows_whose_icon_slots_share_a_column_are_clean_whatever_the_glyph_size() {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    for (at, glyph) in [18.0_f32, 24.0, 18.0].into_iter().enumerate() {
        let y = at as f32 * 40.0;
        let row = tree.rect(Some(root), (8.0, y, 264.0, 40.0), "SidebarRow");
        let slot = tree.rect(Some(row), (16.0, y + 8.0, 24.0, 24.0), "");
        tree.icon(slot, (16.0 + (24.0 - glyph) / 2.0, y + 8.0 + (24.0 - glyph) / 2.0, glyph, glyph));
        tree.text(row, "Item", (48.0, y + 11.0, 60.0, 18.0));
    }
    let snapshot = tree.snapshot();
    assert!(found(&snapshot, Rule::ColumnAligned).is_empty());
    assert!(found(&snapshot, Rule::ListRowInset).is_empty());
}

// SP-09: list-row inset.

fn nav_row(icon_x: f32) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let row = tree.rect(Some(root), (8.0, 0.0, 264.0, 40.0), "SidebarRow");
    tree.icon(row, (icon_x, 11.0, 18.0, 18.0));
    tree.text(row, "Item", (48.0, 11.0, 60.0, 18.0));
    tree.snapshot()
}

#[test]
fn a_nav_row_whose_icon_is_nearer_its_top_than_its_start_is_found() {
    let findings = found(&nav_row(14.0), Rule::ListRowInset);
    let [finding] = findings.as_slice() else {
        panic!("expected one finding, got {findings:?}");
    };
    assert!(finding.what.starts_with("[SP-09, SP-10]"), "{}", finding.what);
    assert!(finding.numbers.contains(&("icon-left", 6.0)));
    assert!(finding.numbers.contains(&("icon-top", 11.0)));
}

#[test]
fn a_nav_row_with_x_padding_equal_to_its_y_padding_is_clean() {
    assert!(found(&nav_row(19.0), Rule::ListRowInset).is_empty());
}

#[test]
fn a_two_line_row_centres_its_icon_and_is_not_held_to_the_nav_inset() {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let row = tree.rect(Some(root), (8.0, 0.0, 264.0, 56.0), "PlaceRow");
    tree.icon(row, (16.0, 16.0, 24.0, 24.0));
    tree.text(row, "Drive", (48.0, 8.0, 60.0, 18.0));
    // 56 tall at regular density: a second line, not a nav item.
    assert!(found(&tree.snapshot(), Rule::ListRowInset).is_empty());
}

// AL-03: field text start.

fn field(text_x: f32) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let input = tree.rect(Some(root), (0.0, 0.0, 240.0, 36.0), "TextInput");
    let slot = tree.rect(Some(input), (0.0, 0.0, 36.0, 36.0), "IconSlot");
    tree.icon(slot, (10.0, 10.0, 16.0, 16.0));
    tree.text(input, "Search", (text_x, 8.0, 100.0, 20.0));
    tree.snapshot()
}

#[test]
fn field_text_that_starts_far_after_the_slot_is_found() {
    let findings = found(&field(44.0), Rule::FieldTextStart);
    let [finding] = findings.as_slice() else {
        panic!("expected one finding, got {findings:?}");
    };
    assert!(finding.what.starts_with("[AL-03, SF-01]"), "{}", finding.what);
    assert!(finding.numbers.contains(&("gap", 8.0)));
}

#[test]
fn field_text_one_s1_after_the_slot_is_clean() {
    assert!(found(&field(40.0), Rule::FieldTextStart).is_empty());
}

// AL-04, SZ-02: header.

fn header(button_height: f32, title_x: f32, compact: bool) -> Snapshot {
    let mut tree = Tree::default();
    let width = if compact { 390.0 } else { 1000.0 };
    let root = window(&mut tree, width);
    let bar = tree.rect(Some(root), (0.0, 0.0, width, 48.0), "HeaderBar");
    let button = tree.rect(Some(bar), (8.0, 6.0, 36.0, button_height), "IconButton");
    tree.focusable(button);
    // The centred title is on its own layer over the whole bar.
    let layer = tree.rect(Some(bar), (0.0, 0.0, width, 48.0), "");
    tree.at(layer).used.out_of_flow = true;
    tree.text(layer, "Network", (title_x, 14.0, 80.0, 20.0));
    tree.snapshot()
}

#[test]
fn a_header_button_that_is_not_control_tall_is_found() {
    let findings = found(&header(30.0, 460.0, false), Rule::HeaderMeasure);
    let [finding] = findings.as_slice() else {
        panic!("expected one finding, got {findings:?}");
    };
    assert!(finding.what.starts_with("[AL-04, SZ-02]"), "{}", finding.what);
    assert!(finding.numbers.contains(&("height", 30.0)));
    // At compact density the control is 44 tall.
    assert_eq!(found(&header(36.0, 155.0, true), Rule::HeaderMeasure).len(), 1);
    assert!(found(&header(44.0, 155.0, true), Rule::HeaderMeasure).is_empty());
}

#[test]
fn a_title_off_the_centre_of_the_bar_is_found() {
    let findings = found(&header(36.0, 380.0, false), Rule::HeaderMeasure);
    let [finding] = findings.as_slice() else {
        panic!("expected one finding, got {findings:?}");
    };
    assert!(finding.numbers.contains(&("header-centre", 500.0)));
}

#[test]
fn a_centred_title_and_control_tall_buttons_are_clean() {
    assert!(found(&header(36.0, 460.0, false), Rule::HeaderMeasure).is_empty());
}

// BR-01, BR-02: radii.

fn nested_button(radius: f32, inset: f32) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let card = tree.rect(Some(root), (0.0, 0.0, 400.0, 56.0), "Row");
    tree.paint(card, 16.0);
    let button = tree.rect(Some(card), (302.0, inset, 90.0, 56.0 - 2.0 * inset), "RowButton");
    tree.paint(button, radius);
    tree.snapshot()
}

#[test]
fn a_button_inset_8_in_a_card_of_16_with_a_control_radius_is_found() {
    let findings = found(&nested_button(12.0, 8.0), Rule::RadiusConcentric);
    assert!(!findings.is_empty());
    assert!(findings[0].what.starts_with("[BR-01, BR-02]"), "{}", findings[0].what);
    assert!(findings[0].numbers.contains(&("expected", 8.0)));
}

#[test]
fn a_button_with_the_outer_radius_less_its_inset_is_clean() {
    assert!(found(&nested_button(8.0, 8.0), Rule::RadiusConcentric).is_empty());
    // Flush with the corner, a square child is clipped by the card.
    assert!(found(&nested_button(0.0, 0.0), Rule::RadiusConcentric).is_empty());
}

#[test]
fn a_pill_is_found_unless_it_is_a_switch() {
    let pill = |component: &str| {
        let mut tree = Tree::default();
        let root = window(&mut tree, 1000.0);
        let host = tree.rect(Some(root), (0.0, 0.0, 400.0, 56.0), component);
        let shape = tree.rect(Some(host), (300.0, 16.0, 40.0, 24.0), "");
        tree.paint(shape, 999.0);
        tree.snapshot()
    };
    assert_eq!(found(&pill("IconButton"), Rule::RadiusConcentric).len(), 1);
    assert!(found(&pill("Switch"), Rule::RadiusConcentric).is_empty());
}

// SP-15: off-scale.

fn padded(padding: [f32; 4], gap: f32, width: f32) -> Snapshot {
    let mut tree = Tree::default();
    let root = window(&mut tree, width);
    let body = tree.rect(Some(root), (0.0, 0.0, 300.0, 100.0), "");
    tree.pad(body, padding);
    tree.at(body).props.as_mut().unwrap().gap = gap;
    tree.snapshot()
}

#[test]
fn an_in_between_padding_or_gap_is_found() {
    let findings = found(&padded([6.0, 16.0, 6.0, 16.0], 0.0, 1000.0), Rule::OffScale);
    let [finding] = findings.as_slice() else {
        panic!("expected one finding, got {findings:?}");
    };
    assert!(finding.what.starts_with("[SP-15, SP-21]"), "{}", finding.what);
    assert!(finding.numbers.contains(&("padding-top", 6.0)));
    assert_eq!(found(&padded([0.0; 4], 5.0, 1000.0), Rule::OffScale).len(), 1);
}

#[test]
fn a_scrollbar_thumb_is_placed_by_its_margin_and_is_not_measured() {
    let mut tree = Tree::default();
    let root = window(&mut tree, 1000.0);
    let thumb = tree.rect(Some(root), (990.0, 51.0, 4.0, 80.0), "");
    tree.at(thumb).used.margin = [51.0, 3.0, 3.0, 0.0];
    assert!(found(&tree.snapshot(), Rule::OffScale).is_empty());
}

#[test]
fn space_steps_are_clean_at_their_own_density() {
    assert!(found(&padded([12.0, 16.0, 12.0, 16.0], 8.0, 1000.0), Rule::OffScale).is_empty());
    // S3 is 14 at compact density: clean on a phone, and clean in a wide
    // window too, which may hold a compact specimen.
    assert!(found(&padded([14.0, 18.0, 14.0, 18.0], 14.0, 390.0), Rule::OffScale).is_empty());
    assert!(found(&padded([14.0, 0.0, 14.0, 0.0], 0.0, 1000.0), Rule::OffScale).is_empty());
    // S5 is 20 at regular density and 22 at compact: on a phone, 20 is off.
    assert_eq!(found(&padded([20.0, 0.0, 20.0, 0.0], 0.0, 390.0), Rule::OffScale).len(), 1);
    assert!(found(&padded([20.0, 0.0, 20.0, 0.0], 0.0, 1000.0), Rule::OffScale).is_empty());
}

#[test]
fn a_circle_is_a_knob_unless_it_is_a_buttons_face() {
    let circle = |component: &str| {
        let mut tree = Tree::default();
        let root = window(&mut tree, 1000.0);
        let host = tree.rect(Some(root), (0.0, 0.0, 400.0, 56.0), "Row");
        let shape = tree.rect(Some(host), (300.0, 8.0, 40.0, 40.0), component);
        tree.paint(shape, 999.0);
        tree.snapshot()
    };
    assert_eq!(found(&circle("IconButton"), Rule::RadiusConcentric).len(), 1);
    assert!(found(&circle("LevelRow"), Rule::RadiusConcentric).is_empty());
}

// The tables.

#[test]
fn every_rule_names_its_ids_and_every_exemption_and_pill_its_reason() {
    for rule in RULES {
        let meta = meta(rule);
        assert!(!meta.key.is_empty() && !meta.ids.is_empty() && !meta.why.is_empty() && !meta.fix.is_empty());
        assert_eq!(rule.key(), meta.key);
    }
    for entry in EXEMPT {
        assert!(entry.reason.len() > 20, "{} needs a reason", entry.component);
    }
    for (name, reason) in super::NOT_BUTTONS {
        assert!(reason.len() > 20, "{name} needs a reason");
    }
    for (pill, reason) in super::scale_checks::PILLS {
        assert!(reason.len() > 10, "{pill} needs a reason");
    }
}
