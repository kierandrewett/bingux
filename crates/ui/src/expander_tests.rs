//! The expander row at desktop and phone widths: where its header, chevron
//! and revealed rows sit against a plain row of the same card, that it opens
//! and closes without moving anything but what is below it, and that a
//! keyboard and assistive technology can use it.
//!
//! `BINGUX_EXPANDER_DIR=/some/dir cargo test --lib --features testing
//! expander_matrix -- --ignored` writes one PNG per case to look at
//! (`BINGUX_TEXT_SCALE=2` for the largest text scale,
//! `BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-light` for the light scheme,
//! `BINGUX_EXPANDER_TAG=name-` to prefix the files).

use std::path::PathBuf;
use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use crate::group::Group;
use crate::icons::Icon;
use crate::inspector::snapshot::Snapshot as Tree;
use crate::rows::{expander_row, row, switch_row};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const FRAME: Duration = Duration::from_millis(16);
/// A window wide enough to be a desktop, and the phone widths the apps are
/// held to.
const DESKTOP: f32 = 900.0;
const PHONES: [f32; 3] = [390.0, 360.0, 320.0];
const SHORT: &str = "Advanced";
const SHORT_SUB: &str = "Two rows";
const LONG: &str = "Other wired adapters that are not plugged into anything right now";
const LONG_SUB: &str =
    "Cable unplugged on every one of them, so none can carry traffic until one is connected";

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Kind {
    /// A title and nothing else: one row high.
    Plain,
    Subtitle,
    Leading,
    Value,
    /// An expander among the rows of another.
    Nested,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Case {
    pub kind: Kind,
    pub long: bool,
    pub open: bool,
}

impl Case {
    fn name(self, width: f32) -> String {
        format!(
            "{:?}-{}-{}-{}",
            self.kind,
            if self.long { "long" } else { "short" },
            if self.open { "open" } else { "closed" },
            width as u32
        )
        .to_lowercase()
    }
}

fn class_of(width: f32) -> SizeClass {
    if width < 600.0 { SizeClass::Compact } else { SizeClass::Regular }
}

fn height_of(width: f32) -> f32 {
    if width < 600.0 { 800.0 } else { 700.0 }
}

fn children_for(case: Case) -> Vec<Element> {
    if case.long {
        vec![
            switch_row("Connect automatically when the cable is plugged in", true, |_| {}).into(),
            row("Hardware address").value("02:1A:2B:3C:4D:5E").into(),
            row("Metric of the default route on this adapter").subtitle(LONG_SUB).into(),
        ]
    } else {
        vec![
            switch_row("Option one", true, |_| {}).into(),
            row("Option two").value("On").into(),
        ]
    }
}

/// The expander of `case`.
fn expander(case: Case) -> Element {
    let title = if case.long { LONG } else { SHORT };
    let subtitle = if case.long { LONG_SUB } else { SHORT_SUB };
    let header = expander_row(title, children_for(case)).expanded(case.open);
    match case.kind {
        Kind::Plain => header,
        Kind::Subtitle => header.subtitle(subtitle),
        Kind::Leading => header.subtitle(subtitle).leading(Icon::NetworkWired),
        Kind::Value => header.subtitle(subtitle).value("3 adapters"),
        Kind::Nested => {
            let inner =
                expander_row("More", vec![row("Deeper").value("1").into()]).expanded(case.open);
            expander_row(title, vec![inner.into(), row("Sibling").into()]).expanded(case.open)
        }
    }
    .into()
}

/// A card of "Before", the expander, "After" and a row that navigates, so
/// the rows around the expander show what it moves and what it should match.
pub(crate) fn card(case: Case) -> Element {
    Group::titled("Network")
        .rows(vec![
            row("Before").into(),
            expander(case),
            row("After").into(),
            row("Compare").value("A value").navigates().into(),
        ])
        .into()
}

#[derive(PartialEq)]
struct Scene(Case);

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .expanded()
            .tw(&tokens, &[p(S4)])
            .child(crate::shell::OverlayHost::new(card(self.0)))
    }
}

fn mount(case: Case, width: f32) -> Snapshot {
    let mut shot = Snapshot::new(move || Scene(case).into(), (width, height_of(width)));
    shot.settle(Duration::from_millis(300));
    shot
}

fn tree(shot: &mut Snapshot) -> Tree {
    Tree::capture_headless(shot.runner(), 1.0)
}

/// The areas of every node built by the component `name`, top to bottom.
fn areas(tree: &Tree, name: &str) -> Vec<Area> {
    tree.nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some(name))
        .filter_map(|node| node.used.border.or(node.area))
        .collect()
}

/// The area of the label or paragraph whose text is exactly `text`.
fn text_area(shot: &mut Snapshot, text: &str) -> Option<Area> {
    shot.runner()
        .find_many(|node, element| {
            let plain = match Label::try_downcast(element) {
                Some(label) => label.text.to_string(),
                None => Paragraph::try_downcast(element)?
                    .spans
                    .iter()
                    .map(|span| span.text.to_string())
                    .collect(),
            };
            (plain == text).then(|| node.layout().area)
        })
        .into_iter()
        .next()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.51
}

/// Where the rows of the card are: `Before`, the header, the revealed rows
/// and the rows after it.
struct Rows {
    before: Area,
    header: Area,
    inside: Vec<Area>,
    after: Area,
    compare: Area,
}

fn rows(shot: &mut Snapshot, revealed: usize) -> Rows {
    let all = areas(&tree(shot), "Row");
    assert_eq!(all.len(), 4 + revealed, "the rows of the card: {all:?}");
    Rows {
        before: all[0],
        header: all[1],
        inside: all[2..2 + revealed].to_vec(),
        after: all[2 + revealed],
        compare: all[3 + revealed],
    }
}

// ---------------------------------------------------------------------------
// Structure: the header is a row of the card, the chevron has its place, the
// revealed rows are inside the card.
// ---------------------------------------------------------------------------

/// The header is one of the card's rows: as wide as the row before it, in
/// line with it, no shorter than a row and, on a phone, no shorter than a
/// finger. Its chevron ends one gutter before the row's edge and is centred on
/// the row, in the same place as the chevron of a row that navigates.
#[test]
fn the_header_is_a_row_of_the_card_with_the_chevron_in_its_own_slot() {
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        let class = class_of(width);
        let mut shot = mount(Case { kind: Kind::Plain, long: false, open: false }, width);
        let found = tree(&mut shot);
        let rows = rows(&mut shot, 0);
        let (header, before) = (rows.header, rows.before);
        assert!(near(header.min_x(), before.min_x()), "{width}: header starts at {}, the row before at {}", header.min_x(), before.min_x());
        assert!(near(header.width(), before.width()), "{width}: header is {} wide, the row before {}", header.width(), before.width());
        assert!(near(header.min_y(), before.max_y()), "{width}: the header starts at {}, the row before ends at {}", header.min_y(), before.max_y());
        assert!(near(header.height(), Dim::Row.px_in(class)), "{width}: header is {} high, a row {}", header.height(), Dim::Row.px_in(class));
        if class.is_compact() {
            assert!(header.height() >= TOUCH_MIN, "{width}: header is {} high, under a finger", header.height());
        }
        let chevron = areas(&found, "DisclosureChevron");
        assert_eq!(chevron.len(), 1, "{width}: one chevron");
        let chevron = chevron[0];
        assert!(
            near(chevron.max_x(), header.max_x() - GUTTER.px_in(class)),
            "{width}: the chevron ends at {}, a gutter from {}",
            chevron.max_x(),
            header.max_x()
        );
        assert!(
            near(chevron.min_y() + chevron.height() / 2.0, header.min_y() + header.height() / 2.0),
            "{width}: the chevron is not centred on the header"
        );
        // The chevron of the row that navigates, the last of the card.
        let navigating = found
            .nodes
            .iter()
            .filter_map(|node| node.used.border.or(node.area))
            .filter(|area| {
                area.min_y() >= rows.compare.min_y()
                    && area.max_y() <= rows.compare.max_y()
                    && near(area.width(), area.height())
                    && area.width() < 24.0
                    && area.width() > 6.0
            })
            .max_by(|a, b| a.max_x().total_cmp(&b.max_x()))
            .expect("the chevron of the row that navigates");
        assert!(
            near(navigating.max_x(), chevron.max_x()) && near(navigating.width(), chevron.width()),
            "{width}: the expander's chevron ({}..{}) is not where a navigating row's is ({}..{})",
            chevron.min_x(),
            chevron.max_x(),
            navigating.min_x(),
            navigating.max_x()
        );
    }
}

/// A title and a subtitle too long for the line wrap onto as many lines as
/// they need: nothing is cut with an ellipsis, the header grows to hold them
/// and the chevron stays centred.
#[test]
fn a_long_title_and_subtitle_wrap_inside_the_header() {
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        let class = class_of(width);
        let mut shot = mount(Case { kind: Kind::Subtitle, long: true, open: false }, width);
        let rows = rows(&mut shot, 0);
        let header = rows.header;
        let title = text_area(&mut shot, LONG).expect("the title");
        let subtitle = text_area(&mut shot, LONG_SUB).expect("the subtitle");
        if class.is_compact() {
            // 66 characters of the title and 86 of the smaller subtitle do not
            // fit one line on a phone, and at 320 px the title needs a third,
            // which a title cut after two lines would not show.
            assert!(title.height() > 1.5 * 20.0, "{width}: the title is {} high, on one line", title.height());
            assert!(subtitle.height() > 1.5 * 16.0, "{width}: the subtitle is {} high, on one line", subtitle.height());
        }
        if width == 320.0 {
            assert!(title.height() > 2.5 * 20.0, "{width}: the title is {} high, cut to two lines", title.height());
        }
        assert!(title.min_y() >= header.min_y() - 0.5 && subtitle.max_y() <= header.max_y() + 0.5, "{width}: the text is outside the header");
        assert!(title.max_x() <= header.max_x() - GUTTER.px_in(class) + 0.5, "{width}: the title reaches the chevron's side");
        assert!(subtitle.max_x() <= header.max_x() - GUTTER.px_in(class) + 0.5, "{width}: the subtitle reaches the chevron's side");
        assert!(title.max_y() <= subtitle.min_y() + 0.5, "{width}: the title and the subtitle overlap");
        let chevron = areas(&tree(&mut shot), "DisclosureChevron")[0];
        assert!(
            near(chevron.min_y() + chevron.height() / 2.0, header.min_y() + header.height() / 2.0),
            "{width}: the chevron is not centred on the tall header"
        );
    }
}

#[derive(PartialEq)]
struct NarrowScene(Case, f32);

impl Component for NarrowScene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect().expanded().tw(&tokens, &[p(S4)]).child(crate::shell::OverlayHost::new(
            rect().width(Size::px(self.1)).child(card(self.0)),
        ))
    }
}

/// A narrow column in a desktop window (the taxonomy's desktop stage, a split
/// view's pane) is at regular density, where an ordinary row cuts its title
/// to one line: the header still wraps its title.
#[test]
fn a_long_title_wraps_in_a_narrow_column_at_desktop_density() {
    let case = Case { kind: Kind::Subtitle, long: true, open: false };
    let mut shot = Snapshot::new(move || NarrowScene(case, 360.0).into(), (DESKTOP, 700.0));
    shot.settle(Duration::from_millis(300));
    let title = text_area(&mut shot, LONG).expect("the title");
    assert!(title.height() > 1.5 * 18.0, "the title is {} high, on one line", title.height());
}

/// The revealed rows are in the card, set in by one gutter from the header,
/// ending where the header ends, with nothing wider than the window.
#[test]
fn the_revealed_rows_are_inside_the_card_inset_by_one_gutter() {
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        let class = class_of(width);
        for (kind, long) in [(Kind::Subtitle, false), (Kind::Subtitle, true), (Kind::Value, true)] {
            let mut shot = mount(Case { kind, long, open: true }, width);
            let found = tree(&mut shot);
            let revealed = if long { 3 } else { 2 };
            let rows = rows(&mut shot, revealed);
            let gutter = GUTTER.px_in(class);
            let mut top = rows.header.max_y();
            for (index, inside) in rows.inside.iter().enumerate() {
                assert!(near(inside.min_x(), rows.header.min_x() + gutter), "{width} {kind:?} {long}: row {index} starts at {}, a gutter from {}", inside.min_x(), rows.header.min_x());
                assert!(near(inside.max_x(), rows.header.max_x()), "{width} {kind:?} {long}: row {index} ends at {}, the header at {}", inside.max_x(), rows.header.max_x());
                assert!(inside.min_y() >= top - 0.5, "{width} {kind:?} {long}: row {index} overlaps the one above");
                top = inside.max_y();
            }
            assert!(rows.after.min_y() >= top - 0.5, "{width}: After overlaps the revealed rows");
            for node in found.nodes.iter().filter(|node| node.text.is_some() || node.a11y.component.is_some()) {
                if let Some(area) = node.used.border.or(node.area) {
                    assert!(area.max_x() <= width + 0.5, "{width} {kind:?} {long}: {} reaches {} past the window", node.title(), area.max_x());
                }
            }
            // Nothing the rows hold sticks out of the card at its sides.
            for area in areas(&found, "Row").iter().skip(1) {
                assert!(area.min_x() >= rows.before.min_x() - 0.5 && area.max_x() <= rows.before.max_x() + 0.5, "{width}: a row is outside the card");
            }
        }
    }
}

/// An expander among the rows of another is set in by one more gutter, and
/// its rows by one more again.
#[test]
fn an_expander_inside_an_expander_is_set_in_one_gutter_more() {
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        let gutter = GUTTER.px_in(class_of(width));
        let mut shot = mount(Case { kind: Kind::Nested, long: false, open: true }, width);
        // Advanced, More, Deeper, Sibling.
        let found = areas(&tree(&mut shot), "Row");
        assert_eq!(found.len(), 7, "{found:?}");
        let (outer, more, deeper, sibling) = (found[1], found[2], found[3], found[4]);
        assert!(near(more.min_x(), outer.min_x() + gutter), "{width}: More");
        assert!(near(deeper.min_x(), outer.min_x() + 2.0 * gutter), "{width}: Deeper");
        assert!(near(sibling.min_x(), outer.min_x() + gutter), "{width}: Sibling");
        for area in [more, deeper, sibling] {
            assert!(near(area.max_x(), outer.max_x()), "{width}: a nested row ends at {}", area.max_x());
        }
    }
}

// ---------------------------------------------------------------------------
// Motion: the rows below move by the height the content gains, in the pass
// that starts the opening, and nothing above or beside moves.
// ---------------------------------------------------------------------------

/// What one frame of a motion looks like.
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    before: Area,
    header: Area,
    chevron: Area,
    after: Area,
    compare: Area,
}

fn frame(shot: &mut Snapshot) -> Frame {
    let found = tree(shot);
    let all = areas(&found, "Row");
    let chevron = areas(&found, "DisclosureChevron")[0];
    Frame {
        before: all[0],
        header: all[1],
        chevron,
        after: all[all.len() - 2],
        compare: all[all.len() - 1],
    }
}

fn frames(shot: &mut Snapshot, count: usize) -> Vec<Frame> {
    (0..count)
        .map(|_| {
            shot.settle(FRAME);
            frame(shot)
        })
        .collect()
}

fn check_motion(width: f32, case: Case) {
    let closed = mount(Case { open: false, ..case }, width).pipe_frame();
    let open = mount(Case { open: true, ..case }, width).pipe_frame();
    let grows = open.after.min_y() - closed.after.min_y();
    assert!(grows > 40.0, "{width}: the content is {grows} high");
    let mut shot = mount(case, width);
    let start = frame(&mut shot);
    let (from, to) = if case.open { (&open, &closed) } else { (&closed, &open) };
    assert_eq!(&start, from, "{width}: it mounts where it settles");
    shot.click(width / 2.0, start.header.min_y() + 12.0);
    let run = frames(&mut shot, 24);
    let delta = |frame: &Frame| frame.after.min_y() - start.after.min_y();
    let wanted = to.after.min_y() - from.after.min_y();
    let mut last = 0.0_f32;
    let mut moved = false;
    for (index, frame) in run.iter().enumerate() {
        // Nothing above the content moves, and the header keeps its size
        // and its chevron its place.
        assert_eq!(frame.before, start.before, "{width} frame {index}: the row above moved");
        assert_eq!(frame.header, start.header, "{width} frame {index}: the header changed");
        assert_eq!(frame.chevron, start.chevron, "{width} frame {index}: the chevron left its slot");
        // The rows below move together, by the height the content has now.
        assert!(near(delta(frame), frame.compare.min_y() - start.compare.min_y()), "{width} frame {index}: the rows below drifted apart");
        assert!(near(frame.after.height(), start.after.height()), "{width} frame {index}: After changed height");
        // The motion goes one way and never passes where it settles.
        let step = delta(frame);
        if wanted > 0.0 {
            assert!(step >= last - 0.01 && step <= wanted + 0.51, "{width} frame {index}: {step} of {wanted} after {last}");
        } else {
            assert!(step <= last + 0.01 && step >= wanted - 0.51, "{width} frame {index}: {step} of {wanted} after {last}");
        }
        moved |= step.abs() > 1.0 && step.abs() < wanted.abs() - 1.0;
        last = step;
    }
    assert!(moved, "{width}: the rows never passed between the two heights: {:?}", run.iter().map(delta).collect::<Vec<_>>());
    assert_eq!(run.last().unwrap(), to, "{width}: it settles where a mount of the other state does");
}

trait PipeFrame {
    fn pipe_frame(self) -> Frame;
}

impl PipeFrame for Snapshot {
    fn pipe_frame(mut self) -> Frame {
        frame(&mut self)
    }
}

#[test]
fn opening_moves_the_rows_below_by_the_height_of_the_content_and_nothing_else() {
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        for (kind, long) in [(Kind::Subtitle, false), (Kind::Subtitle, true), (Kind::Value, true)] {
            check_motion(width, Case { kind, long, open: false });
        }
    }
}

#[test]
fn closing_moves_the_rows_below_back_by_the_same_height() {
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        for (kind, long) in [(Kind::Subtitle, false), (Kind::Subtitle, true), (Kind::Value, true)] {
            check_motion(width, Case { kind, long, open: true });
        }
    }
}

/// An expander that is open when it mounts is laid out at its final height in
/// the first frame: the rows below do not move afterwards.
#[test]
fn an_expander_that_mounts_open_does_not_move_after_its_first_frame() {
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        for long in [false, true] {
            let case = Case { kind: Kind::Subtitle, long, open: true };
            let mut shot = Snapshot::new(move || Scene(case).into(), (width, height_of(width)));
            shot.settle(FRAME);
            let first = frame(&mut shot);
            shot.settle(Duration::from_millis(400));
            let settled = frame(&mut shot);
            assert_eq!(first, settled, "{width} long={long}: the layout of the first frame is not the settled one");
        }
    }
}

/// The inspector's layout rules (text cut off, clipped ink, overlaps, boxes
/// outside their parent) find nothing in any case at any width.
///
/// With a leading icon beside a subtitle the lint reports the icon's column
/// (`fill_minimum` tall, 9.5 px below the text) as uneven padding and off
/// centre. A plain `row(..).subtitle(..).leading(..)` outside an expander
/// gets the same two findings (the glyph itself is centred), so they are not
/// the expander's and are left out here. The text of the rows inside (the
/// ordinary rows with a value and a long subtitle) is the rows' own, held by
/// their tests: a `value-ellipsised` finding under `NestedRows` is left out.
#[test]
fn the_layout_rules_find_nothing_in_any_case() {
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        for kind in [Kind::Plain, Kind::Subtitle, Kind::Leading, Kind::Value, Kind::Nested] {
            for long in [false, true] {
                for open in [false, true] {
                    let case = Case { kind, long, open };
                    let mut shot = mount(case, width);
                    let findings: Vec<String> = shot
                        .layout_findings()
                        .iter()
                        .filter(|finding| {
                            let leading = kind == Kind::Leading
                                && matches!(finding.rule.key(), "uneven-padding" | "off-centre" | "inset-asymmetry");
                            let inside = finding.rule.key() == "value-ellipsised"
                                && finding.path.contains("NestedRows");
                            !leading && !inside
                        })
                        .map(|finding| format!("{}: {} ({})", finding.rule.key(), finding.target, finding.what))
                        .collect();
                    assert!(findings.is_empty(), "{}: {findings:#?}", case.name(width));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Closed content is not there; keys and assistive technology.
// ---------------------------------------------------------------------------

fn shows(shot: &mut Snapshot, text: &str) -> bool {
    text_area(shot, text).is_some()
}

/// A closed expander has none of its rows in the tree, so the Tab key and a
/// screen reader cannot reach what is not shown.
#[test]
fn a_closed_expander_holds_no_rows_and_an_open_one_does() {
    for width in [DESKTOP, 390.0] {
        let mut closed = mount(Case { kind: Kind::Subtitle, long: false, open: false }, width);
        assert!(!shows(&mut closed, "Option one"), "{width}: a closed expander holds its rows");
        // Tab from the top reaches the header, then nothing hidden.
        closed.key(Key::Named(NamedKey::Tab));
        closed.key(Key::Named(NamedKey::Tab));
        let focused = tree(&mut closed)
            .nodes
            .iter()
            .filter(|node| node.a11y.focused)
            .filter_map(|node| node.a11y.name.clone().or_else(|| node.a11y.component.clone()))
            .collect::<Vec<_>>();
        assert!(!focused.iter().any(|name| name.contains("Option")), "{width}: Tab reached a hidden row: {focused:?}");
        let mut open = mount(Case { kind: Kind::Subtitle, long: false, open: true }, width);
        assert!(shows(&mut open, "Option one"));
    }
}

fn settle_state(shot: &mut Snapshot) {
    shot.settle(Duration::from_millis(400));
}

/// Enter and Space press the focused header, Right opens it and Left closes
/// it, and Right on an open one or Left on a closed one does nothing.
#[test]
fn the_keyboard_opens_and_closes_the_expander() {
    for width in [DESKTOP, 390.0] {
        let mut shot = mount(Case { kind: Kind::Subtitle, long: false, open: false }, width);
        shot.key(Key::Named(NamedKey::Tab));
        assert!(!shows(&mut shot, "Option one"));
        shot.key(Key::Named(NamedKey::Enter));
        settle_state(&mut shot);
        assert!(shows(&mut shot, "Option one"), "{width}: Enter did not open it");
        shot.key(Key::Character(" ".into()));
        settle_state(&mut shot);
        assert!(!shows(&mut shot, "Option one"), "{width}: Space did not close it");
        shot.key(Key::Named(NamedKey::ArrowRight));
        settle_state(&mut shot);
        assert!(shows(&mut shot, "Option one"), "{width}: Right did not open it");
        shot.key(Key::Named(NamedKey::ArrowRight));
        settle_state(&mut shot);
        assert!(shows(&mut shot, "Option one"), "{width}: Right closed an open one");
        shot.key(Key::Named(NamedKey::ArrowLeft));
        settle_state(&mut shot);
        assert!(!shows(&mut shot, "Option one"), "{width}: Left did not close it");
        shot.key(Key::Named(NamedKey::ArrowLeft));
        settle_state(&mut shot);
        assert!(!shows(&mut shot, "Option one"), "{width}: Left opened a closed one");
    }
}

/// What the focused node says about being expanded.
fn focused_expanded(shot: &mut Snapshot) -> Option<bool> {
    shot.runner()
        .run_in(|| Platform::get().current_window().focused_accessibility_node.read().is_expanded())
}

/// The header says whether it is expanded: false while closed, true while
/// open, in the accessibility tree.
#[test]
fn the_header_reports_whether_it_is_expanded() {
    for open in [false, true] {
        let mut shot = mount(Case { kind: Kind::Subtitle, long: false, open }, 390.0);
        shot.key(Key::Named(NamedKey::Tab));
        assert_eq!(focused_expanded(&mut shot), Some(open), "mounted open={open}");
        shot.key(Key::Named(NamedKey::Enter));
        shot.settle(Duration::from_millis(200));
        assert_eq!(focused_expanded(&mut shot), Some(!open), "after Enter, from open={open}");
    }
}

/// The header is lit while the pointer is on it, as any row that opens
/// something is, and hovering a revealed row does not light the header.
#[test]
fn the_header_lights_under_the_pointer_and_the_revealed_rows_do_not_light_it() {
    let width = 390.0;
    let fill = |shot: &mut Snapshot, index: usize| {
        let found = tree(shot);
        found
            .nodes
            .iter()
            .filter(|node| node.a11y.component.as_deref() == Some("Row"))
            .nth(index)
            .and_then(|node| node.props.as_ref())
            .and_then(|props| props.background)
            .is_some_and(|colour| colour.a() != 0)
    };
    let mut shot = mount(Case { kind: Kind::Subtitle, long: false, open: true }, width);
    let rows = rows(&mut shot, 2);
    assert!(!fill(&mut shot, 1), "the header is lit at rest");
    shot.hover(width / 2.0, rows.header.min_y() + 12.0);
    shot.settle(Duration::from_millis(200));
    assert!(fill(&mut shot, 1), "the header is not lit under the pointer");
    let inside = rows.inside[1];
    shot.hover(width / 2.0, inside.min_y() + inside.height() / 2.0);
    shot.settle(Duration::from_millis(200));
    assert!(!fill(&mut shot, 1), "the header is lit while a revealed row is hovered");
}

// ---------------------------------------------------------------------------
// A page that scrolls: opening near the bottom keeps the header where it was.
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
struct ScrollScene;

impl Component for ScrollScene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut rows: Vec<Element> = (0..8).map(|index| row(format!("Filler {index}")).into()).collect();
        rows.push(
            expander_row("Advanced", vec![row("Option one").into(), row("Option two").into(), row("Option three").into()])
                .subtitle("Near the bottom")
                .into(),
        );
        rect().expanded().tw(&tokens, &[p(S4)]).child(crate::shell::OverlayHost::new(
            ScrollView::new().child(Group::titled("Page").rows(rows)),
        ))
    }
}

/// Opening an expander at the bottom of a scrolled page does not scroll the
/// page: the header stays where it was and the rows grow below it.
#[test]
fn opening_near_the_bottom_of_a_scrolled_page_keeps_the_header_where_it_was() {
    for width in [DESKTOP, 390.0] {
        let mut shot = Snapshot::new(|| ScrollScene.into(), (width, 420.0));
        shot.settle(Duration::from_millis(300));
        shot.scroll(width / 2.0, 200.0, -2000.0);
        shot.settle(Duration::from_millis(300));
        let header = *areas(&tree(&mut shot), "Row").last().expect("the expander's header");
        assert!(header.max_y() <= 420.0, "{width}: the header is off the window at {header:?}");
        shot.click(width / 2.0, header.min_y() + 12.0);
        let mut ys = Vec::new();
        for _ in 0..24 {
            shot.settle(FRAME);
            let rows = areas(&tree(&mut shot), "Row");
            ys.push(rows[8].min_y());
        }
        assert!(ys.iter().all(|y| near(*y, header.min_y())), "{width}: the header moved while it opened: {ys:?} from {}", header.min_y());
        assert!(shows(&mut shot, "Option three"), "{width}: the rows did not open");
    }
}

// ---------------------------------------------------------------------------
// The record: one PNG per case.
// ---------------------------------------------------------------------------

fn dir() -> Option<PathBuf> {
    std::env::var_os("BINGUX_EXPANDER_DIR").map(PathBuf::from)
}

#[test]
#[ignore = "writes PNGs when BINGUX_EXPANDER_DIR is set"]
fn expander_matrix() {
    let Some(dir) = dir() else { return };
    let tag = std::env::var("BINGUX_EXPANDER_TAG").unwrap_or_default();
    for width in std::iter::once(DESKTOP).chain(PHONES) {
        for kind in [Kind::Plain, Kind::Subtitle, Kind::Leading, Kind::Value, Kind::Nested] {
            for long in [false, true] {
                for open in [false, true] {
                    let case = Case { kind, long, open };
                    let mut shot = mount(case, width);
                    shot.save(dir.join(format!("{tag}{}.png", case.name(width))));
                }
            }
        }
    }
}

/// The frames of an opening and a closing at 390 px, to look at.
#[test]
#[ignore = "writes PNGs when BINGUX_EXPANDER_DIR is set"]
fn expander_frame_strip() {
    let Some(dir) = dir() else { return };
    let tag = std::env::var("BINGUX_EXPANDER_TAG").unwrap_or_default();
    for open in [false, true] {
        let case = Case { kind: Kind::Subtitle, long: true, open };
        let mut shot = mount(case, 390.0);
        let header = frame(&mut shot).header;
        shot.save(dir.join(format!("{tag}strip-{}-00.png", if open { "close" } else { "open" })));
        shot.click(195.0, header.min_y() + 12.0);
        for index in 1..=12 {
            shot.settle(FRAME);
            shot.save(dir.join(format!("{tag}strip-{}-{index:02}.png", if open { "close" } else { "open" })));
        }
    }
}
