//! Fixtures for the state checks: windows made of controls that break one rule
//! each (the check must flag exactly that), and a window of the shared
//! controls (the check must find nothing).

use std::time::Duration;

use freya::prelude::*;

use super::{Report, Rule, Scope};
use crate::component::ComponentName;
use crate::controls::{action_button, checkbox, dropdown, icon_button, switch, text_field};
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const DESKTOP: (f32, f32) = (900.0, 600.0);
const PHONE: (f32, f32) = (390.0, 800.0);

/// One control that breaks a rule on purpose, or keeps it.
#[derive(Clone, PartialEq)]
struct Probe {
    name: &'static str,
    /// Paints a hover and a pressed state.
    states: bool,
    /// Ends its render with `.focusable(..)`.
    ring: bool,
    size: (f32, f32),
    cursor: Option<CursorIcon>,
    /// Where it sits in the window, when it is placed by hand.
    at: Option<(f32, f32)>,
    /// Clipped by a box exactly its size: the ring has no room.
    clipped: bool,
    /// A dropdown: a combo box with a chevron of this size at its end, and,
    /// when `under`, a label that runs under it.
    chevron: Option<IconSize>,
    under: bool,
}

impl Probe {
    fn new(name: &'static str) -> Self {
        Self { name, states: true, ring: true, size: (120.0, 44.0), cursor: None, at: None, clipped: false, chevron: None, under: false }
    }
}

impl Component for Probe {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let states = self.states;
        let face = rect()
            .component(self.name)
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(if self.chevron.is_some() { AccessibilityRole::ComboBox } else { AccessibilityRole::Button })
            .a11y_alt(self.name)
            .tw(
                &tokens,
                &[
                    rounded(Radius::Control),
                    surface(Paint::Control, states && hovering(), states && pressing(), false),
                ],
            )
            .width(Size::px(self.size.0))
            .height(Size::px(self.size.1))
            .maybe(self.cursor.is_some(), |face| face.cursor(self.cursor.unwrap_or_default()))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .child(label().text(self.name.to_string()))
            .maybe_child(self.chevron.map(|size| {
                let glyph = icons::icon(Icon::PanDown, size, Paint::Muted.color(&tokens));
                // Under the label, absolute, or after it in the row.
                if self.under {
                    rect().position(Position::new_absolute().left(4.0).top(10.0)).child(glyph).into_element()
                } else {
                    rect().position(Position::new_absolute().right(8.0).top(14.0)).child(glyph).into_element()
                }
            }));
        let face = if self.ring { face.focusable(focus()) } else { face };
        let held = if self.clipped {
            rect().width(Size::px(self.size.0)).height(Size::px(self.size.1)).overflow(Overflow::Clip).child(face)
        } else {
            rect().child(face)
        };
        match self.at {
            Some((x, y)) => held.position(Position::new_absolute().left(x).top(y)),
            None => held,
        }
    }
}

/// A window of `probes` in a column, or placed by hand.
#[derive(Clone, PartialEq)]
struct Page(Vec<Probe>);

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut column = rect().expanded().tw(&tokens, &[flex_col(), gap(S5), p(S5)]);
        for probe in &self.0 {
            column = column.child(probe.clone());
        }
        WindowHost::new(OverlayHost::new(column))
    }
}

fn report(probes: Vec<Probe>, size: (f32, f32), scope: Scope) -> Report {
    let mut shot = Snapshot::new(move || Page(probes.clone()).into_element(), size);
    shot.settle(Duration::from_millis(300));
    shot.state_report(scope)
}

fn only(scope: Scope) -> Scope {
    Scope { limit: 40, ..scope }
}

fn rules(report: &Report, rule: Rule) -> Vec<String> {
    report.findings.iter().filter(|finding| finding.rule == rule).map(|finding| finding.target.clone()).collect()
}

fn named(report: &Report, rule: Rule, name: &str) -> bool {
    rules(report, rule).iter().any(|target| target.contains(name))
}

#[test]
fn a_control_with_no_hover_or_pressed_look_is_found() {
    let mut dead = Probe::new("Dead");
    dead.states = false;
    let report = report(vec![dead, Probe::new("Alive")], DESKTOP, only(Scope::ALL));
    assert!(named(&report, Rule::Hover, "Dead"), "{:#?}", report.findings);
    assert!(named(&report, Rule::Press, "Dead"), "{:#?}", report.findings);
    assert!(!named(&report, Rule::Hover, "Alive"), "{:#?}", report.findings);
    assert!(!named(&report, Rule::Press, "Alive"), "{:#?}", report.findings);
}

#[test]
fn a_focusable_control_with_no_ring_is_found() {
    let mut bare = Probe::new("Bare");
    bare.ring = false;
    let report = report(vec![bare, Probe::new("Ringed")], DESKTOP, only(Scope::ALL));
    assert!(named(&report, Rule::Focus, "Bare") || named(&report, Rule::TabRing, "Bare"), "{:#?}", report.findings);
    assert!(!named(&report, Rule::Focus, "Ringed"), "{:#?}", report.findings);
    assert!(!named(&report, Rule::TabRing, "Ringed"), "{:#?}", report.findings);
    assert!(!named(&report, Rule::RingClipped, "Ringed"), "{:#?}", report.findings);
}

#[test]
fn a_ring_cut_off_by_its_clipping_parent_is_found() {
    let mut tight = Probe::new("Tight");
    tight.clipped = true;
    let report = report(vec![tight, Probe::new("Free")], DESKTOP, only(Scope::ALL));
    assert!(
        named(&report, Rule::RingClipped, "Tight") || named(&report, Rule::Focus, "Tight") || named(&report, Rule::TabRing, "Tight"),
        "{:#?}",
        report.findings
    );
    assert!(!named(&report, Rule::RingClipped, "Free"), "{:#?}", report.findings);
}

#[test]
fn a_small_control_is_a_finding_at_compact_density_only() {
    let mut small = Probe::new("Small");
    small.size = (30.0, 30.0);
    let touch = Scope { sweep: false, tab: false, menu: false, cursor: false, ..Scope::ALL };
    let phone = report(vec![small.clone(), Probe::new("Big")], PHONE, touch);
    assert!(named(&phone, Rule::Touch, "Small"), "{:#?}", phone.findings);
    assert!(!named(&phone, Rule::Touch, "Big"), "{:#?}", phone.findings);
    let desktop = report(vec![small], DESKTOP, touch);
    assert!(rules(&desktop, Rule::Touch).is_empty(), "{:#?}", desktop.findings);
}

#[test]
fn a_pointer_hand_over_a_control_is_a_finding() {
    let mut hand = Probe::new("Hand");
    hand.cursor = Some(CursorIcon::Pointer);
    let mut beam = Probe::new("Beam");
    beam.cursor = Some(CursorIcon::Text);
    let cursor = Scope { sweep: false, tab: false, menu: false, statics: false, ..Scope::ALL };
    let report = report(vec![hand, beam, Probe::new("Arrow")], DESKTOP, cursor);
    assert!(named(&report, Rule::Cursor, "Hand"), "{:#?}", report.findings);
    assert!(named(&report, Rule::Cursor, "Beam"), "{:#?}", report.findings);
    assert!(!named(&report, Rule::Cursor, "Arrow"), "{:#?}", report.findings);
}

#[test]
fn a_label_under_its_chevron_and_a_big_chevron_are_found() {
    let mut under = Probe::new("Under");
    under.chevron = Some(IconSize::Mark);
    under.under = true;
    under.size = (120.0, 36.0);
    let mut big = Probe::new("Big");
    big.chevron = Some(IconSize::Row);
    big.size = (120.0, 36.0);
    let mut right = Probe::new("Right");
    right.chevron = Some(IconSize::Mark);
    right.size = (120.0, 36.0);
    let statics = Scope { sweep: false, tab: false, menu: false, cursor: false, ..Scope::ALL };
    let report = report(vec![under, big, right], DESKTOP, statics);
    assert!(named(&report, Rule::TextUnderGlyph, "Under"), "{:#?}", report.findings);
    assert!(!named(&report, Rule::TextUnderGlyph, "Right"), "{:#?}", report.findings);
    assert!(named(&report, Rule::ChevronSize, "Big"), "{:#?}", report.findings);
    assert!(!named(&report, Rule::ChevronSize, "Right"), "{:#?}", report.findings);
}

#[test]
fn tab_that_goes_back_up_the_page_is_found() {
    // Placed by hand: the first in tree order is the lower one.
    let mut low = Probe::new("Low");
    low.at = Some((40.0, 300.0));
    let mut high = Probe::new("High");
    high.at = Some((40.0, 60.0));
    let tab = Scope { sweep: false, statics: false, menu: false, cursor: false, ..Scope::ALL };
    let report = report(vec![low, high], DESKTOP, tab);
    assert_eq!(report.stops, 2, "{:#?}", report);
    assert!(named(&report, Rule::TabOrder, "High"), "{:#?}", report.findings);
}

#[test]
fn tab_in_reading_order_has_one_stop_a_control_and_a_ring_at_each() {
    let tab = Scope { sweep: false, statics: false, menu: false, cursor: false, ..Scope::ALL };
    let report = report(vec![Probe::new("One"), Probe::new("Two"), Probe::new("Three")], DESKTOP, tab);
    assert_eq!(report.stops, 3, "{:#?}", report);
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
}

/// A scroller whose view ends exactly at the bottom of its third control, with
/// a fourth below the fold: focusing the third scrolls it a margin into view, so
/// the page moves under the ring.
#[derive(Clone, PartialEq)]
struct EdgePage;

impl Component for EdgePage {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let probe = |name: &'static str| Probe { size: (120.0, 44.0), ..Probe::new(name) };
        let list = rect()
            .tw(&tokens, &[flex_col(), px(S5)])
            .child(rect().height(Size::px(8.0)))
            .child(probe("First"))
            .child(rect().height(Size::px(8.0)))
            .child(probe("Second"))
            .child(rect().height(Size::px(8.0)))
            .child(probe("Edge"))
            .child(rect().height(Size::px(60.0)))
            .child(probe("Below"));
        // 8 + 44 + 8 + 44 + 8 + 44 = 156: the third control ends at the view's last pixel.
        WindowHost::new(OverlayHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg)])
                .child(ScrollView::new().width(Size::px(400.0)).height(Size::px(156.0)).child(list)),
        ))
    }
}

#[test]
fn a_ring_on_a_control_the_focus_scrolls_into_view_is_read_on_the_page_it_is_on() {
    let mut shot = Snapshot::new(|| EdgePage.into_element(), DESKTOP);
    shot.settle(Duration::from_millis(300));
    let report = shot.state_report(Scope::ALL);
    // The fourth control is below the fold and not on screen; the three in view are swept.
    assert!(report.swept >= 3, "{report:#?}");
    for rule in [Rule::Focus, Rule::TabRing, Rule::RingClipped] {
        assert!(rules(&report, rule).is_empty(), "{rule:?}: {:#?}", report.findings);
    }
}

/// Three controls, the first under a panel that is open over it.
#[derive(Clone, PartialEq)]
struct Covered(bool);

impl Component for Covered {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut under = Probe::new("Under");
        under.states = false;
        under.ring = false;
        under.at = Some((20.0, 20.0));
        let mut beside = Probe::new("Beside");
        beside.at = Some((20.0, 300.0));
        WindowHost::new(OverlayHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg)])
                .child(under)
                .child(beside)
                .maybe_child(self.0.then(|| {
                    rect()
                        .component("Panel")
                        .position(Position::new_absolute().left(0.0).top(0.0))
                        .width(Size::px(400.0))
                        .height(Size::px(120.0))
                        .tw(&tokens, &[bg(Paint::Surface)])
                })),
        ))
    }
}

#[test]
fn a_control_under_an_open_panel_is_not_measured_behind_it() {
    let measure = |covered: bool| {
        let mut shot = Snapshot::new(move || Covered(covered).into_element(), DESKTOP);
        shot.settle(Duration::from_millis(300));
        shot.state_report(only(Scope::ALL))
    };
    // A control with no ring and no states is found in the open...
    let open = measure(false);
    assert!(named(&open, Rule::Hover, "Under"), "{:#?}", open.findings);
    assert!(named(&open, Rule::Focus, "Under") || named(&open, Rule::TabRing, "Under"), "{:#?}", open.findings);
    // ...and left alone while a panel is over it: the pointer and the ring land on the panel.
    let under = measure(true);
    for rule in [Rule::Hover, Rule::Press, Rule::Focus, Rule::TabRing] {
        assert!(!named(&under, rule, "Under"), "{rule:?}: {:#?}", under.findings);
    }
}

/// A group of switch rows on a phone, where a finger needs a wider target than the painted switch.
#[derive(Clone, PartialEq)]
struct SwitchRows;

impl Component for SwitchRows {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let first = use_state(|| true);
        let second = use_state(|| true);
        let third = use_state(|| true);
        let group = crate::group::Group::new(None)
            .row(crate::rows::switch_row("Applications", first(), { let mut s = first; move |v| s.set(v) }))
            .row(crate::rows::switch_row("Files", second(), { let mut s = second; move |v| s.set(v) }))
            .row(crate::rows::switch_row("Calculation", third(), { let mut s = third; move |v| s.set(v) }));
        WindowHost::new(OverlayHost::new(
            rect().expanded().tw(&tokens, &[flex_col(), p(S4), bg(Paint::Bg)]).child(group),
        ))
    }
}

// A switch on a phone has a hit area wider than its face (`hit_area` in controls/press.rs), and
// its thumb is a solid box under the pointer in the middle of the face. The area is in front of
// the thumb, so every switch of a group lights under the pointer, and again when pressed, in
// whatever order the engine lists the nodes of one layer.
#[test]
fn every_switch_of_a_phone_group_lights_under_the_pointer_and_again_when_pressed() {
    let mut shot = Snapshot::new(|| SwitchRows.into_element(), PHONE);
    shot.settle(Duration::from_millis(300));
    let report = shot.state_report(only(Scope { menu: false, tab: false, cursor: false, statics: false, ..Scope::ALL }));
    assert!(report.swept >= 3, "{report:#?}");
    for rule in [Rule::Hover, Rule::Press] {
        assert!(rules(&report, rule).is_empty(), "{rule:?}: {:#?}", report.findings);
    }
}

/// The shared controls, which must pass every check.
#[derive(Clone, PartialEq)]
struct Shared;

impl Component for Shared {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let options: Vec<(String, String)> = ["alpha", "bravo", "charlie", "delta"]
            .iter()
            .map(|key| (key.to_string(), key.to_uppercase()))
            .collect();
        let mut selected = use_state(|| "alpha".to_string());
        let mut on = use_state(|| false);
        let mut ticked = use_state(|| false);
        let text = use_state(|| String::new());
        WindowHost::new(OverlayHost::new(
            rect()
                .expanded()
                // A page surface: a button with no fill of its own shows its hover and press as
                // a translucent layer over what is behind it, and over the window's black that
                // layer is invisible.
                .tw(&tokens, &[flex_col(), gap(S5), p(S5), bg(Paint::Bg)])
                .child(action_button("Apply", |_| {}))
                .child(icon_button(Icon::Search, |_| {}).tooltip("Search"))
                .child(switch(on(), move |value| on.set(value)))
                // Its hit area must not hide the pointer from it: hover and press still show.
                .child(crate::shell::WindowControls)
                .child(checkbox(ticked(), move |value| ticked.set(value)).text("Remember"))
                .child(text_field(text, |_: String| {}).placeholder("Name"))
                .child(dropdown(options, selected.read().clone(), move |key: String| selected.set(key))),
        ))
    }
}

#[test]
fn the_shared_controls_pass_every_check() {
    let mut shot = Snapshot::new(|| Shared.into_element(), DESKTOP);
    shot.settle(Duration::from_millis(300));
    let report = shot.state_report(Scope::ALL);
    assert!(report.interactive >= 5, "{report:#?}");
    assert_eq!(report.menus, 1, "{report:#?}");
    let failures: Vec<_> = report.failures().collect();
    assert!(failures.is_empty(), "{failures:#?}\nnotes: {:#?}", report.notes);
}

#[test]
fn the_resize_handles_are_a_band_and_a_corner_a_hand_can_grab() {
    use crate::window::popup::{RESIZE_CORNER, RESIZE_INSIDE, RESIZE_OUTSIDE};
    // SZ-06: an edge band of 4 to 8 px, a corner square of 12 to 16 px, and
    // most of the band outside the panel so a control or scrollbar at the
    // edge is not under it.
    let band = RESIZE_OUTSIDE + RESIZE_INSIDE;
    assert!((4.0..=8.0).contains(&band), "edge band {band}");
    assert!((12.0..=16.0).contains(&RESIZE_CORNER), "corner {RESIZE_CORNER}");
    assert!(RESIZE_INSIDE <= RESIZE_OUTSIDE, "the band reaches further outside than in");
    assert!(RESIZE_CORNER / 2.0 <= RESIZE_OUTSIDE + RESIZE_INSIDE + 2.0, "the corner square stays near the edge band");
}
