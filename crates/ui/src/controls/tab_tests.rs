//! The tab strip, rendered: the geometry of the tabs at every width and
//! density, the pixels of a tab and of the page under it in both schemes and
//! both reading directions, and the keyboard, pointer and closing behaviour of
//! the strip.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use freya_core::elements::label::Label;
use freya_testing::prelude::{MouseEventName, PlatformEvent, WheelEventName};
use torin::prelude::Area;

use super::direction::{Direction, directed, window_directed};
use super::fade_text::fade_text;
use super::tab_close::selection_after_close;
use super::tab_geometry::Geometry;
use super::tab_tear::{Detach, StripProbe};
use super::tabs::{BAR, TabItem, tab_strip};
use crate::component::ComponentName;
use crate::contrast::composite;
use crate::group::caption;
use crate::shell::overlays::OverlayHost;
use crate::shell::{HeaderBar, InertControls, WindowControls};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};
use crate::window::WindowHost;

const LONG: &str = "A name far too long for the room it is given, which must be cut";
const REGULAR: f32 = 900.0;
const COMPACT: f32 = 390.0;

/// A strip over a page, and what it is built from.
#[derive(PartialEq, Clone, Copy)]
struct Case {
    count: usize,
    /// The width the strip may take.
    container: f32,
    add: bool,
    long: bool,
    dark: bool,
    selected: u64,
    /// The strip is the tab slot of a header bar with the window controls, as
    /// in Files, and fills the room between them.
    header: bool,
    /// The interface reads from the right.
    rtl: bool,
}

impl Case {
    fn new(count: usize, container: f32) -> Self {
        Self { count, container, add: true, long: false, dark: true, selected: 1, header: false, rtl: false }
    }

    /// The Files window: a header bar with the strip.
    fn files(count: usize, selected: u64) -> Self {
        Self { header: true, selected, ..Self::new(count, 0.0) }
    }
}

/// The window root: it provides the tokens of the case's scheme.
#[derive(PartialEq)]
struct Root(Case);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let scheme = if self.0.dark { Scheme::PreferDark } else { Scheme::PreferLight };
        let tokens = Tokens::new(DEFAULT_ACCENT, scheme, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(Host(self.0))
    }
}

#[derive(PartialEq)]
struct Host(Case);

impl Component for Host {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let case = self.0;
        // The window controls draw and do nothing: a test must not close its window.
        use_provide_context(|| InertControls);
        let tabs = (0..case.count)
            .map(|index| {
                let id = index as u64 + 1;
                if case.long && index == 0 {
                    TabItem::new(id, LONG)
                } else {
                    TabItem::new(id, format!("Folder {id}"))
                }
            })
            .collect();
        let mut strip = tab_strip(tabs, case.selected);
        if case.add {
            strip = strip.on_new(|_| {});
        }
        let bar: Element = if case.header {
            HeaderBar::new("Files").trailing(WindowControls).tab_slot(strip).into()
        } else {
            rect()
                .component("TabBar")
                .tw(&tokens, &[w_full(), h(Dim::TabBar), bg(BAR), pl(S2)])
                .child(rect().component("Container").width(Size::px(case.container)).child(strip))
                .into()
        };
        let page = rect()
            .component("Page")
            .tw(&tokens, &[w_full(), h_flex(), flex_col(), bg(Paint::Bg), p(S4), gap(S1)])
            .child(caption("The page the selected tab opens."))
            .child(caption("The tabs are separate from it."));
        let root = rect().tw(&tokens, &[w_full(), h_full(), flex_col()]).child(bar).child(page);
        if case.rtl { window_directed(Direction::Rtl, root).into_element() } else { root.into_element() }
    }
}

/// The areas of the elements of component class `name`, left to right as the
/// person sees them. The tree order is the paint order, not the order on
/// screen: a held tab is painted over its neighbours.
fn named(shot: &mut Snapshot, name: &'static str) -> Vec<Area> {
    let mut found: Vec<Area> = shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area)
    });
    found.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()).then(a.min_y().total_cmp(&b.min_y())));
    found
}

fn one(shot: &mut Snapshot, name: &'static str) -> Area {
    named(shot, name).into_iter().next().unwrap_or_else(|| panic!("no {name} on screen"))
}

/// The bar the strip is in: a plain one, or the header bar of a Files window.
fn bar_of(shot: &mut Snapshot) -> Area {
    named(shot, "TabBar").into_iter().next().unwrap_or_else(|| one(shot, "HeaderBar"))
}

fn contains(outer: &Area, inner: &Area, slack: f32) -> bool {
    inner.min_x() >= outer.min_x() - slack
        && inner.max_x() <= outer.max_x() + slack
        && inner.min_y() >= outer.min_y() - slack
        && inner.max_y() <= outer.max_y() + slack
}

fn start(window: f32, case: Case) -> Snapshot {
    start_in(window, 300.0, case)
}

fn start_in(window: f32, height: f32, case: Case) -> Snapshot {
    let mut shot = Snapshot::new(move || Root(case).into_element(), (window, height));
    shot.settle(Duration::from_millis(400));
    shot
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.6
}

/// Whether `window` is a compact-density window.
fn compact(window: f32) -> bool {
    window < 400.0
}

fn geometry_for(window: f32) -> Geometry {
    Geometry::in_class(if compact(window) { SizeClass::Compact } else { SizeClass::Regular })
}

// ---- The tabs' geometry ----------------------------------------------------

/// Check one strip in one window.
fn check(window: f32, case: Case) {
    let what = format!(
        "{window}px window, {} tabs in {}px, add {}, long {}, selected {}, rtl {}",
        case.count, case.container, case.add, case.long, case.selected, case.rtl
    );
    let mut shot = start(window, case);
    let g = geometry_for(window);
    let (bar, page) = (bar_of(&mut shot), one(&mut shot, "Page"));
    let strip = one(&mut shot, "TabStrip");
    let container = one(&mut shot, "Container");
    let tabs = named(&mut shot, "Tab");
    let closes = named(&mut shot, "TabClose");
    assert_eq!(tabs.len(), case.count, "{what}: a tab is drawn for each");
    // The strip reads from the other end in a right-to-left window: its first
    // tab is the rightmost.
    let first = |areas: &[Area]| if case.rtl { areas[areas.len() - 1] } else { areas[0] };

    // The page begins where the bar ends: no gap, no overlap, and no margin.
    assert!(near(bar.height(), g.strip), "{what}: the bar is {} tall, not {}", bar.height(), g.strip);
    assert!(near(bar.max_y(), page.min_y()), "{what}: the bar ends at {} and the page begins at {}", bar.max_y(), page.min_y());

    // Every tab stands the same distance from the bar's top and bottom, is as
    // tall as the token, and is as wide as the others.
    let width = tabs[0].width();
    for tab in &tabs {
        assert!(near(tab.min_y() - bar.min_y(), g.top), "{what}: a tab is {} below the bar's top, not {}", tab.min_y() - bar.min_y(), g.top);
        assert!(near(bar.max_y() - tab.max_y(), g.top), "{what}: a tab is {} above the bar's bottom, not {}", bar.max_y() - tab.max_y(), g.top);
        assert!(near(tab.height(), g.tab), "{what}: a tab is {} tall, not {}", tab.height(), g.tab);
        assert!(near(tab.width(), width), "{what}: tab widths differ: {} and {width}", tab.width());
    }
    assert!(width <= g.max + 0.6, "{what}: tabs are {width} wide, past the token");

    // Children inside the strip, the strip inside its container and the bar.
    assert!(contains(&bar, &strip, 0.5), "{what}: the strip is outside the bar");
    if !case.header {
        assert!(contains(&container, &strip, 0.5), "{what}: the strip {strip:?} spills out of {container:?}");
    }
    assert!(near(strip.height(), g.strip), "{what}: the strip is {} tall", strip.height());

    // The one cluster after the tabs that the tab list and New tab buttons are
    // in: as tall as the strip, with the inset before the first button and
    // after the last, which is where it ends.
    let container_width = if case.header { strip.width() } else { case.container };
    let overflowing = g.content(case.count as f32, g.min) > container_width - g.cluster(usize::from(case.add)) + 0.01;
    let wanted = usize::from(case.add) + usize::from(overflowing);
    let cluster = named(&mut shot, "TabEnd");
    assert_eq!(cluster.len(), usize::from(wanted > 0), "{what}: one cluster of buttons, when there are any");
    // Where the tabs end and the cluster begins, along the reading direction.
    let (region_start, region_end) = match cluster.first() {
        Some(cluster) if case.rtl => (strip.max_x(), cluster.max_x()),
        Some(cluster) => (strip.min_x(), cluster.min_x()),
        None if case.rtl => (strip.max_x(), strip.min_x()),
        None => (strip.min_x(), strip.max_x()),
    };
    if let Some(cluster) = cluster.first() {
        assert!(contains(&strip, cluster, 0.5), "{what}: the buttons {cluster:?} are outside the strip {strip:?}");
        assert!(near(cluster.width(), g.cluster(wanted)) && near(cluster.height(), g.strip), "{what}: the cluster is {}x{}", cluster.width(), cluster.height());
        assert!(near(cluster.min_y(), strip.min_y()) && near(cluster.max_y(), strip.max_y()), "{what}: the buttons are level with the tabs");
        if case.add {
            let end = if case.rtl { (strip.min_x(), cluster.min_x()) } else { (strip.max_x(), cluster.max_x()) };
            assert!(near(end.0, end.1), "{what}: the New tab button ends the strip: strip {strip:?}, cluster {cluster:?}");
        }
    }

    // The close slot: every tab has one, a lone tab too. It is inside its
    // tab, the inset from its end, centred in it.
    assert_eq!(closes.len(), case.count, "{what}: every tab has a close slot");
    for (tab, close) in tabs.iter().zip(&closes) {
        assert!(contains(tab, close, 0.5), "{what}: the close slot is outside its tab");
        let from_end = if case.rtl { close.min_x() - tab.min_x() } else { tab.max_x() - close.max_x() };
        assert!(near(from_end, g.inset), "{what}: the close slot is {from_end} from the tab's end");
        assert!(near(close.min_y() - tab.min_y(), (g.tab - close.height()) / 2.0), "{what}: the close slot is centred in the tab: {} from its top", close.min_y() - tab.min_y());
    }

    // Titles are cut before the close slot: a title's box ends where the room for it does.
    let titles = named(&mut shot, "FadeText");
    assert_eq!(titles.len(), case.count, "{what}: every tab has a fading title");
    for (title, close) in titles.iter().zip(&closes) {
        if case.rtl {
            assert!(title.min_x() >= close.max_x() - 0.5, "{what}: a title runs under the close slot");
        } else {
            assert!(title.max_x() <= close.min_x() + 0.5, "{what}: a title runs under the close slot");
        }
    }

    if overflowing {
        assert!(near(width, g.min), "{what}: scrolling tabs are {width} wide, not the minimum {}", g.min);
        assert!(near(strip.width(), container_width), "{what}: an overflowing strip fills its container");
        assert!(wanted >= 1 + usize::from(case.add), "{what}: the tab list button is drawn when tabs overflow");
        // The selected tab is scrolled into the region, when the region can hold a tab at all.
        let selected = tabs[if case.rtl { case.count - case.selected as usize } else { case.selected as usize - 1 }];
        if (region_end - region_start).abs() >= width {
            let (low, high) = (region_start.min(region_end), region_start.max(region_end));
            assert!(selected.min_x() >= low - 0.6 && selected.max_x() <= high + 0.6, "{what}: the selected tab {selected:?} is not in view between {low} and {high}: strip {strip:?}, cluster {cluster:?}, tabs {tabs:?}");
        }
    } else {
        // The first tab is at the start of the strip, the tabs are a gap apart,
        // and the last ends before the buttons.
        let start_edge = if case.rtl { first(&tabs).max_x() - strip.max_x() } else { first(&tabs).min_x() - strip.min_x() };
        assert!(near(start_edge, 0.0), "{what}: the first tab is {start_edge} from the strip's start");
        for pair in tabs.windows(2) {
            assert!(near(pair[1].min_x() - pair[0].max_x(), g.gap()), "{what}: tabs are a gap apart");
        }
        let last = if case.rtl { tabs[0] } else { *tabs.last().unwrap() };
        let room = if case.rtl { last.min_x() - region_end } else { region_end - last.max_x() };
        assert!(room >= -0.6, "{what}: the last tab runs into what follows");
        assert!(width >= g.min - 0.6, "{what}: tabs are {width} wide, under the minimum");
    }
}

#[test]
fn the_tabs_stand_the_same_distance_from_the_bars_top_and_bottom_at_mouse_density() {
    for rtl in [false, true] {
        for container in [260.0, 340.0, 440.0, 600.0, 760.0] {
            for (count, add, long, selected) in [
                (1, true, false, 1),
                (2, true, false, 2),
                (3, true, false, 1),
                (3, false, false, 3),
                (6, true, false, 4),
                (6, true, false, 6),
                (9, true, false, 9),
                (9, true, false, 5),
                (12, true, false, 1),
                (12, true, false, 7),
                (2, true, true, 1),
            ] {
                check(REGULAR, Case { add, long, selected, rtl, ..Case::new(count, container) });
            }
        }
    }
}

#[test]
fn the_tabs_stand_the_same_distance_from_the_bars_top_and_bottom_at_compact_density() {
    for rtl in [false, true] {
        for container in [200.0, 280.0, 360.0] {
            for (count, add, long, selected) in [
                (1, true, false, 1),
                (3, true, false, 2),
                (6, true, false, 6),
                (6, true, false, 4),
                (9, true, false, 5),
                (2, true, true, 1),
            ] {
                check(COMPACT, Case { add, long, selected, rtl, ..Case::new(count, container) });
            }
        }
    }
}

#[test]
fn in_a_header_bar_the_selected_tab_is_in_view_at_either_density_and_direction() {
    // The Files window: the strip fills the room between the window's buttons, and a
    // selection far along it is scrolled into the region, from the start edge in
    // left to right and from the other one in right to left.
    for rtl in [false, true] {
        for (window, count, selected) in [(REGULAR, 12, 7), (REGULAR, 12, 12), (COMPACT, 9, 5), (COMPACT, 9, 9), (COMPACT, 9, 1)] {
            let mut shot = start(window, Case { rtl, ..Case::files(count, selected) });
            let (strip, cluster) = (one(&mut shot, "TabStrip"), one(&mut shot, "TabEnd"));
            let tabs = named(&mut shot, "Tab");
            let tab = tabs[if rtl { count - selected as usize } else { selected as usize - 1 }];
            let (low, high) = if rtl { (cluster.max_x(), strip.max_x()) } else { (strip.min_x(), cluster.min_x()) };
            assert!(
                tab.min_x() >= low - 0.6 && tab.max_x() <= high + 0.6,
                "{window}px, rtl {rtl}, tab {selected} of {count} is at {tab:?}, not in {low}..{high}"
            );
        }
    }
}

#[test]
fn at_compact_density_every_target_is_the_touch_minimum() {
    let mut shot = start(COMPACT, Case::new(4, 360.0));
    let g = geometry_for(COMPACT);
    assert_eq!(g.inset, 0.0, "no inset at compact density");
    for tab in named(&mut shot, "Tab") {
        assert!(near(tab.height(), 44.0), "a tab is {} tall: not a touch target", tab.height());
    }
    let closes = named(&mut shot, "TabClose");
    assert_eq!(closes.len(), 4, "every tab is closable, and the button is always there at compact density");
    for close in closes {
        assert!(near(close.width(), 44.0) && near(close.height(), 44.0), "the close target is {}x{}", close.width(), close.height());
    }
    let buttons = named(&mut shot, "IconButton");
    assert!(buttons.len() >= 5, "four close buttons and the New tab button are drawn: {}", buttons.len());
    for button in buttons {
        assert!(near(button.width(), 44.0) && near(button.height(), 44.0), "a button is {}x{}", button.width(), button.height());
    }
}

// ---- The bar ---------------------------------------------------------------

#[test]
fn the_bar_is_a_tab_bar_tall_and_the_tabs_stand_four_pixels_from_its_top() {
    for (window, strip, tab, top) in [(REGULAR, 40.0, 32.0, 4.0), (COMPACT, 44.0, 44.0, 0.0)] {
        let mut shot = start(window, Case::new(3, 600.0));
        let (bar, page) = (one(&mut shot, "TabBar"), one(&mut shot, "Page"));
        let tabs = named(&mut shot, "Tab");
        assert!(near(bar.height(), strip), "{window}px: the bar is {} tall, not {strip}", bar.height());
        assert!(near(page.min_y(), strip), "{window}px: the page begins {} down, not {strip}", page.min_y());
        for t in &tabs {
            assert!(near(t.height(), tab), "{window}px: a tab is {} tall, not {tab}", t.height());
            assert!(near(t.min_y(), top), "{window}px: a tab is {} from the top, not {top}", t.min_y());
        }
    }
}

#[test]
fn the_window_gets_the_difference_back_from_the_bar() {
    // Against the 48 px (56 px at compact density) header bar the strip replaced.
    for (class, saved) in [(SizeClass::Regular, 8.0), (SizeClass::Compact, 12.0)] {
        assert_eq!(Dim::Header.px_in(class) - Geometry::in_class(class).strip, saved);
    }
}

#[test]
fn the_header_bar_is_the_strip_so_the_window_has_no_second_row() {
    for (window, count) in [(REGULAR, 3), (COMPACT, 2)] {
        let g = geometry_for(window);
        let mut shot = start(window, Case::files(count, 1));
        let (header, page) = (one(&mut shot, "HeaderBar"), one(&mut shot, "Page"));
        assert!(near(header.height(), g.strip), "{window}px: the header bar is {} tall, the strip's height is {}", header.height(), g.strip);
        assert!(near(page.min_y(), header.max_y()), "{window}px: the page starts under the title bar, with nothing between");
        assert!(near(page.min_x(), header.min_x()) && near(page.max_x(), header.max_x()), "{window}px: and is as wide as it");
    }
}

#[test]
fn the_buttons_in_a_tab_bar_are_the_tab_button_centred_in_it() {
    for (window, side) in [(REGULAR, 32.0), (COMPACT, 44.0)] {
        let g = geometry_for(window);
        let mut shot = start(window, Case::files(3, 1));
        let header = one(&mut shot, "HeaderBar");
        let new_tab = named(&mut shot, "TabEnd");
        assert!(!new_tab.is_empty());
        // The window controls are drawn too: their targets are 32 px (44 px
        // at compact density), inside the bar, and centred in it.
        for control in named(&mut shot, "WindowControl") {
            assert!(control.height() <= header.height() + 0.5, "{window}px: a window button {} tall in a bar {} tall", control.height(), header.height());
            assert!(near(control.min_y() - header.min_y(), header.max_y() - control.max_y()), "{window}px: a window button is centred in the bar");
        }
        for button in named(&mut shot, "IconButton") {
            assert!(button.height() <= header.height() + 0.5, "{window}px: an icon button is {} tall in a bar {} tall", button.height(), header.height());
            assert!(near(button.width(), side) || button.width() <= side, "{window}px: an icon button is {} wide", button.width());
            assert!(near(button.min_y() - header.min_y(), header.max_y() - button.max_y()), "{window}px: an icon button is centred in the bar");
        }
        assert!(g.button() <= g.strip);
    }
}

/// The buttons in the cluster at the end of the strip, left to right.
fn end_buttons(shot: &mut Snapshot) -> (Area, Vec<Area>) {
    let cluster = one(shot, "TabEnd");
    let buttons: Vec<Area> = named(shot, "IconButton").into_iter().filter(|button| contains(&cluster, button, 0.5)).collect();
    (cluster, buttons)
}

#[test]
fn the_new_tab_button_is_the_same_distance_from_the_top_the_bottom_the_last_tab_and_the_trailing_edge() {
    // Mouse density: the field inset (4 px) all round, a 32 px square, the
    // height of a tab. Compact density has no inset: the 44 px button is flush
    // on all four sides.
    for (window, container, count) in [
        (REGULAR, 600.0, 3),
        (REGULAR, 1000.0, 1),
        (REGULAR, 300.0, 9),
        (COMPACT, 360.0, 3),
        (COMPACT, 200.0, 6),
    ] {
        for header in [false, true] {
            let case = if header { Case::files(count, 1) } else { Case::new(count, container) };
            let g = geometry_for(window);
            let what = format!("{window}px, {count} tabs in {container}, header {header}");
            let mut shot = start(window, case);
            let strip = one(&mut shot, "TabStrip");
            let (cluster, buttons) = end_buttons(&mut shot);
            let new = *buttons.last().expect("the New tab button");
            let (top, bottom) = (new.min_y() - strip.min_y(), strip.max_y() - new.max_y());
            let (before, after) = (buttons[0].min_x() - cluster.min_x(), cluster.max_x() - new.max_x());
            assert!(near(top, g.inset), "{what}: {top} from the strip's top, not {}", g.inset);
            assert!(near(bottom, g.inset), "{what}: {bottom} from the strip's bottom, not {}", g.inset);
            assert!(near(before, g.inset), "{what}: {before} from what comes before it");
            assert!(near(after, g.inset), "{what}: {after} from the strip's trailing edge");
            assert!(near(top, bottom) && near(bottom, before) && near(before, after), "{what}: the four insets are one: {top} {bottom} {before} {after}");
            assert!(near(new.width(), new.height()) && near(new.width(), g.button()), "{what}: the hit area is a {} square, not {}x{}", g.button(), new.width(), new.height());
            assert!(near(new.height(), g.tab), "{what}: and as tall as a tab");
            assert!(near(cluster.max_x(), strip.max_x()), "{what}: the trailing edge is the strip's");
            // After the last tab: an inset before the first button, when the tabs fit.
            let tabs = named(&mut shot, "Tab");
            if buttons.len() == 1 {
                let from_tab = new.min_x() - tabs.last().unwrap().max_x();
                assert!(near(from_tab, g.inset), "{what}: {from_tab} from the last tab's end");
            }
        }
    }
}

// ---- Pixels ----------------------------------------------------------------

/// One frame as the renderer drew it.
struct Frame {
    bytes: Vec<u8>,
    row: usize,
    width: usize,
    height: usize,
}

impl Frame {
    fn of(shot: &mut Snapshot) -> Self {
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface = raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        Self { bytes, row, width: image.width() as usize, height: image.height() as usize }
    }

    /// The pixel at the window pixel `(x, y)`.
    fn pixel(&self, x: f32, y: f32) -> Color {
        let x = (x.floor().max(0.0) as usize).min(self.width - 1);
        let y = (y.floor().max(0.0) as usize).min(self.height - 1);
        let at = y * self.row + x * 4;
        // N32 on a little-endian machine is blue, green, red, alpha.
        colour_from_argb(255, self.bytes[at + 2], self.bytes[at + 1], self.bytes[at])
    }
}

fn same(a: Color, b: Color, slack: f32) -> bool {
    (0..3).all(|channel| {
        let (a, b) = (
            [a.r(), a.g(), a.b()][channel] as f32,
            [b.r(), b.g(), b.b()][channel] as f32,
        );
        (a - b).abs() <= slack
    })
}

fn tokens_of(dark: bool) -> Tokens {
    Tokens::new(DEFAULT_ACCENT, if dark { Scheme::PreferDark } else { Scheme::PreferLight }, false)
}

fn scheme_name(dark: bool) -> &'static str {
    if dark { "dark" } else { "light" }
}

#[test]
fn the_new_tab_buttons_hover_fill_is_its_square() {
    // A pointer hovers at mouse density; a finger has no hover.
    let tokens = tokens_of(true);
    let bar_colour = BAR.color(&tokens);
    let mut shot = start(REGULAR, Case::new(3, 600.0));
    let (_, buttons) = end_buttons(&mut shot);
    let new = *buttons.last().unwrap();
    shot.hover(centre(&new).0, centre(&new).1);
    shot.settle(Duration::from_millis(300));
    let frame = Frame::of(&mut shot);
    let hover = composite(Paint::Hover.color(&tokens), bar_colour);
    let middle = centre(&new);
    assert!(same(frame.pixel(middle.0, middle.1 - 8.0), hover, 3.0), "the fill is under the pointer");
    assert!(same(frame.pixel(new.min_x() - 1.5, middle.1), bar_colour, 1.0), "the bar is bare outside it on the left");
    assert!(same(frame.pixel(middle.0, new.min_y() - 1.5), bar_colour, 1.0), "and above");
    assert!(same(frame.pixel(middle.0, new.max_y() + 1.5), bar_colour, 1.0), "and below");
    // Its corners are a tab's: round at Radius::Control, not square.
    assert!(same(frame.pixel(new.min_x() + 0.5, new.min_y() + 0.5), bar_colour, 2.0), "the corner is round");
}

/// A tab's fill at its corners and in its middle, in a frame, in the colours
/// the tab should have. The corner pixel is the bar's: the radius is
/// `Radius::Control`, which is 12 px on a 32 px tab.
fn assert_pill(frame: &Frame, tab: &Area, fill: Color, ground: Color, what: &str) {
    let r = Radius::Control.px();
    // The middle, away from the title and the icon.
    let middle = frame.pixel(tab.min_x() + tab.width() * 0.5, tab.min_y() + 2.0);
    assert!(same(middle, fill, 2.0), "{what}: the top edge of the tab is {middle:?}, not {fill:?}");
    for (x, y, label) in [
        (tab.min_x() + 0.5, tab.min_y() + 0.5, "top start"),
        (tab.max_x() - 0.5, tab.min_y() + 0.5, "top end"),
        (tab.min_x() + 0.5, tab.max_y() - 0.5, "bottom start"),
        (tab.max_x() - 0.5, tab.max_y() - 0.5, "bottom end"),
    ] {
        let got = frame.pixel(x, y);
        assert!(same(got, ground, 2.0), "{what}: the {label} corner is {got:?}, not round to {ground:?}");
    }
    // A radius in along the edge, the fill is back: all four corners have the same radius.
    for (x, y, label) in [
        (tab.min_x() + r + 1.0, tab.min_y() + 1.0, "top start"),
        (tab.max_x() - r - 1.0, tab.min_y() + 1.0, "top end"),
        (tab.min_x() + r + 1.0, tab.max_y() - 1.0, "bottom start"),
        (tab.max_x() - r - 1.0, tab.max_y() - 1.0, "bottom end"),
    ] {
        let got = frame.pixel(x, y);
        assert!(same(got, fill, 2.0), "{what}: a radius in from the {label} corner is {got:?}, not {fill:?}");
    }
}

#[test]
fn a_tab_is_a_rounded_rectangle_of_its_own_in_both_schemes_and_directions() {
    for dark in [true, false] {
        for rtl in [false, true] {
            let tokens = tokens_of(dark);
            let bar = BAR.color(&tokens);
            let selected = composite(Paint::Selection.color(&tokens), bar);
            for window in [REGULAR] {
                let mut shot = start(window, Case { dark, rtl, selected: 2, ..Case::new(3, 600.0) });
                let tabs = named(&mut shot, "Tab");
                let tab = tabs[1];
                let frame = Frame::of(&mut shot);
                let what = format!("{} {} {window}px", scheme_name(dark), if rtl { "rtl" } else { "ltr" });
                assert_pill(&frame, &tab, selected, bar, &format!("{what}, selected tab"));
                // An unselected tab at rest has no fill: the bar shows where it stands.
                let other = tabs[if rtl { 2 } else { 0 }];
                let rest = frame.pixel(other.min_x() + other.width() * 0.5, other.min_y() + 2.0);
                assert!(same(rest, bar, 1.0), "{what}: an unselected tab at rest is {rest:?}, not the bar {bar:?}");
            }
        }
    }
}

#[test]
fn the_page_runs_square_to_the_window_under_a_bar_of_its_own_colour() {
    // Nothing joins the tabs to the page and the page has no corner of its own:
    // every corner and edge of it is the page's colour, at every selection, width
    // and reading direction, and so is the bar around the tabs (they are the same
    // colour on purpose).
    for dark in [true, false] {
        for rtl in [false, true] {
            for (window, count, selected) in [(REGULAR, 1_usize, 1_u64), (REGULAR, 3, 1), (REGULAR, 3, 3), (REGULAR, 9, 5), (COMPACT, 3, 2), (COMPACT, 6, 6)] {
                let tokens = tokens_of(dark);
                let (page_colour, bar_colour) = (Paint::Bg.color(&tokens), BAR.color(&tokens));
                assert_eq!(page_colour, bar_colour, "the bar and the page are one colour");
                let container = window - 24.0;
                let mut shot = start_in(window, 300.0, Case { dark, rtl, selected, ..Case::new(count, container) });
                let (bar, page) = (one(&mut shot, "TabBar"), one(&mut shot, "Page"));
                let frame = Frame::of(&mut shot);
                let what = format!("{} {} {window}px {count} tabs, {selected} selected", scheme_name(dark), if rtl { "rtl" } else { "ltr" });
                let inset = 0.5;
                for (x, y, label) in [
                    (page.min_x() + inset, page.min_y() + inset, "page top start"),
                    (page.max_x() - inset, page.min_y() + inset, "page top end"),
                    (page.min_x() + inset, page.max_y() - inset, "page bottom start"),
                    (page.max_x() - inset, page.max_y() - inset, "page bottom end"),
                    (bar.min_x() + inset, bar.min_y() + inset, "bar top start"),
                    (bar.max_x() - inset, bar.min_y() + inset, "bar top end"),
                    (bar.min_x() + inset, bar.max_y() - inset, "bar bottom start"),
                    (bar.max_x() - inset, bar.max_y() - inset, "bar bottom end"),
                ] {
                    let got = frame.pixel(x, y);
                    assert!(same(got, page_colour, 1.0), "{what}: {label} is {got:?}, not {page_colour:?}");
                }
                // Along the join of bar and page: nothing is drawn but the one colour, except where a
                // tab's own fill comes within its inset of the line (it never touches it).
                let mut x = bar.min_x() + 2.0;
                while x < bar.max_x() - 2.0 {
                    for dy in [-2.0, -1.0, 0.0, 1.0] {
                        let got = frame.pixel(x, page.min_y() + dy);
                        assert!(same(got, page_colour, 1.0) || window == COMPACT, "{what}: at x {x}, {dy} from the join, {got:?} is not {page_colour:?}");
                    }
                    x += 5.0;
                }
            }
        }
    }
}

#[test]
fn the_tab_is_not_joined_to_the_page_under_it() {
    // At mouse density the tab ends an inset above the page. At compact density
    // the tab is the whole bar and its own rounded corners keep it apart.
    for dark in [true, false] {
        let tokens = tokens_of(dark);
        let (bar_colour, selected) = (BAR.color(&tokens), composite(Paint::Selection.color(&tokens), BAR.color(&tokens)));
        for window in [REGULAR, COMPACT] {
            let mut shot = start(window, Case { dark, selected: 2, ..Case::new(3, window - 24.0) });
            let (page, tab) = (one(&mut shot, "Page"), named(&mut shot, "Tab")[1]);
            let frame = Frame::of(&mut shot);
            let what = format!("{} {window}px", scheme_name(dark));
            // Directly under the middle of the selected tab's lower edge: the page's colour, not the selection's.
            let under = frame.pixel(tab.min_x() + tab.width() / 2.0, page.min_y() + 1.0);
            assert!(same(under, bar_colour, 1.0), "{what}: under the tab is {under:?}, not {bar_colour:?}");
            let inside = frame.pixel(tab.min_x() + tab.width() / 2.0, tab.max_y() - 1.5);
            assert!(same(inside, selected, 2.0), "{what}: the tab's lower edge is {inside:?}, not the selection {selected:?}");
            if !compact(window) {
                assert!(page.min_y() - tab.max_y() >= geometry_for(window).inset - 0.5, "{what}: a gap of the inset between tab and page");
            }
        }
    }
}

#[test]
fn a_hovered_tab_has_the_hover_fill_and_a_pressed_one_the_pressed_fill() {
    for dark in [true, false] {
        let tokens = tokens_of(dark);
        let bar = BAR.color(&tokens);
        let (hover, pressed) = (composite(Paint::Hover.color(&tokens), bar), composite(Paint::Pressed.color(&tokens), bar));
        let mut shot = start(REGULAR, Case { dark, selected: 1, ..Case::new(3, 600.0) });
        let tab = named(&mut shot, "Tab")[2];
        shot.hover(tab.min_x() + 20.0, tab.min_y() + 10.0);
        shot.settle(Duration::from_millis(400));
        let frame = Frame::of(&mut shot);
        let what = scheme_name(dark);
        assert_pill(&frame, &tab, hover, bar, &format!("{what}, hovered"));
        let _ = pressed;
    }
}


// ---- Interaction -----------------------------------------------------------

/// How often the host's drag region heard a press.
#[derive(Clone)]
struct Presses(Rc<Cell<u32>>);

impl PartialEq for Presses {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A strip that keeps its own tabs, like a host does: it closes by the
/// browser's rule, keeps the closed tabs, and offers everything in the menu.
#[derive(PartialEq)]
struct Live {
    width: f32,
    names: Vec<&'static str>,
    /// Stand-in for a header bar: counts the presses that reach the bar.
    drag: Option<Presses>,
    /// The ids of the tabs that were closed, in the order they were.
    closed_log: Option<Rc<RefCell<Vec<u64>>>>,
}

impl Live {
    fn new(width: f32) -> Self {
        Self { width, names: vec!["Home", "Documents", "Downloads"], drag: None, closed_log: None }
    }
}

/// Close the tabs `gone` as a host does: the selection by the browser's rule,
/// the closed tabs kept rightmost first so Reopen restores them in order.
fn close_tabs(
    mut tabs: State<Vec<TabItem>>,
    mut closed: State<Vec<(usize, TabItem)>>,
    mut selected: State<u64>,
    gone: &[u64],
    log: &Option<Rc<RefCell<Vec<u64>>>>,
) {
    let ids: Vec<u64> = tabs.read().iter().map(|tab| tab.id).collect();
    let after = selection_after_close(&ids, gone, selected());
    let mut list = tabs.write();
    for id in ids.iter().rev().filter(|id| gone.contains(id)) {
        let Some(index) = list.iter().position(|tab| tab.id == *id) else {
            continue;
        };
        let tab = list.remove(index);
        let place = ids.iter().position(|other| other == id).unwrap_or(index);
        if let Some(log) = log {
            log.borrow_mut().push(tab.id);
        }
        closed.write().push((place, tab));
    }
    if let Some(after) = after {
        selected.set(after);
    }
}

impl Component for Live {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let names = self.names.clone();
        let mut tabs = use_state(move || {
            names.iter().enumerate().map(|(index, name)| TabItem::new(index as u64 + 1, *name)).collect::<Vec<_>>()
        });
        let mut closed = use_state(Vec::<(usize, TabItem)>::new);
        let mut selected = use_state(|| 1_u64);
        let mut next = use_state({
            let count = self.names.len();
            move || count as u64 + 1
        });
        let log = self.closed_log.clone();
        let log_many = log.clone();
        let strip = tab_strip(tabs.read().clone(), selected())
            .on_select(move |id| selected.set(id))
            .on_close(move |id| close_tabs(tabs, closed, selected, &[id], &log))
            .on_close_many(move |ids: Vec<u64>| close_tabs(tabs, closed, selected, &ids, &log_many))
            .on_duplicate(move |id| {
                let fresh = next();
                next.set(fresh + 1);
                let mut list = tabs.write();
                if let Some(index) = list.iter().position(|tab| tab.id == id) {
                    let title = list[index].title.clone();
                    list.insert(index + 1, TabItem::new(fresh, title));
                    selected.set(fresh);
                }
            })
            .on_reopen(!closed.read().is_empty(), move |_| {
                let Some((place, tab)) = closed.write().pop() else {
                    return;
                };
                let id = tab.id;
                let mut list = tabs.write();
                let at = place.min(list.len());
                list.insert(at, tab);
                selected.set(id);
            })
            .on_new(move |_| {
                let id = next();
                next.set(id + 1);
                tabs.write().push(TabItem::new(id, format!("Tab {id}")));
                selected.set(id);
            })
            .on_reorder(move |(id, to): (u64, usize)| {
                let mut list = tabs.write();
                if let Some(from) = list.iter().position(|tab| tab.id == id) {
                    let tab = list.remove(from);
                    let to = to.min(list.len());
                    list.insert(to, tab);
                }
            });
        let presses = self.drag.clone();
        let bar = rect()
            .component("TabBar")
            .tw(&tokens, &[w_full(), h(Dim::TabBar), bg(BAR), pl(S2)])
            // What a header bar does with a press that reaches it: drags the window.
            .on_pointer_down(move |_: Event<PointerEventData>| {
                if let Some(presses) = &presses {
                    presses.0.set(presses.0.get() + 1);
                }
            })
            .child(rect().component("Container").width(Size::px(self.width)).child(strip));
        let page = rect()
            .component("Page")
            .tw(&tokens, &[w_full(), h_flex(), flex_col(), bg(Paint::Bg), p(S4), gap(S1)])
            .child(caption("The page the selected tab opens."));
        rect().tw(&tokens, &[w_full(), h_full(), flex_col()]).child(bar).child(page)
    }
}

#[derive(PartialEq)]
struct LiveRoot(Live, bool);

impl Component for LiveRoot {
    fn render(&self) -> impl IntoElement {
        let scheme = if self.1 { Scheme::PreferDark } else { Scheme::PreferLight };
        let tokens = Tokens::new(DEFAULT_ACCENT, scheme, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(OverlayHost::new(Live {
            width: self.0.width,
            names: self.0.names.clone(),
            drag: self.0.drag.clone(),
            closed_log: self.0.closed_log.clone(),
        }))
    }
}

fn live(width: f32) -> Snapshot {
    live_in(REGULAR, 300.0, Live::new(width), true)
}

/// A live strip in a window of `window` by `height`.
fn live_in(window: f32, height: f32, host: Live, dark: bool) -> Snapshot {
    let host = Rc::new(host);
    let mut shot = Snapshot::new(
        move || {
            let host = host.clone();
            LiveRoot(
                Live { width: host.width, names: host.names.clone(), drag: host.drag.clone(), closed_log: host.closed_log.clone() },
                dark,
            )
            .into_element()
        },
        (window, height),
    );
    shot.settle(Duration::from_millis(400));
    shot
}

/// The tabs left to right: name and whether it is the selected one.
fn tabs_of(shot: &mut Snapshot) -> Vec<(String, bool)> {
    let mut found: Vec<(f32, String, bool)> = shot.runner().find_many(|node, element| {
        let data = element.accessibility();
        (data.builder.class_name() == Some("Tab")).then(|| {
            (
                node.layout().area.min_x(),
                data.builder.label().unwrap_or_default().to_string(),
                data.builder.is_selected() == Some(true),
            )
        })
    });
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, name, selected)| (name, selected)).collect()
}

fn names(shot: &mut Snapshot) -> Vec<String> {
    tabs_of(shot).into_iter().map(|(name, _)| name).collect()
}

fn selected(shot: &mut Snapshot) -> String {
    tabs_of(shot).into_iter().find(|(_, selected)| *selected).map(|(name, _)| name).unwrap_or_default()
}

fn rest(shot: &mut Snapshot) {
    shot.settle(Duration::from_millis(500));
}

fn centre(area: &Area) -> (f32, f32) {
    (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0)
}

/// A press and release of the middle button at a point.
fn middle_click(shot: &mut Snapshot, at: (f32, f32)) {
    let cursor = CursorPoint::new(at.0 as f64, at.1 as f64);
    for name in [MouseEventName::MouseDown, MouseEventName::MouseUp] {
        shot.runner().send_event(PlatformEvent::Mouse { name, cursor, button: Some(MouseButton::Middle) });
        shot.runner().sync_and_update();
    }
}

/// The close buttons that are drawn, left to right: the icon buttons in a tab.
fn close_buttons(shot: &mut Snapshot) -> Vec<Area> {
    let tabs = named(shot, "Tab");
    let mut found: Vec<Area> = named(shot, "TabClose")
        .into_iter()
        .filter(|slot| tabs.iter().any(|tab| contains(tab, slot, 0.5)))
        .filter(|slot| {
            named(shot, "IconButton").iter().any(|button| contains(slot, button, 0.5))
        })
        .collect();
    found.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()));
    found
}

#[test]
fn the_keyboard_selects_closes_and_adds() {
    let mut shot = live(600.0);
    // Tab reaches the selected tab, the only one in the tab order.
    shot.key(Key::Named(NamedKey::Tab));
    rest(&mut shot);
    assert_eq!(selected(&mut shot), "Home");
    shot.key(Key::Named(NamedKey::ArrowRight));
    rest(&mut shot);
    assert_eq!(selected(&mut shot), "Documents");
    shot.key(Key::Named(NamedKey::End));
    rest(&mut shot);
    assert_eq!(selected(&mut shot), "Downloads");
    shot.key(Key::Named(NamedKey::ArrowRight));
    rest(&mut shot);
    assert_eq!(selected(&mut shot), "Downloads", "Right stops at the last tab");
    shot.key(Key::Named(NamedKey::Home));
    rest(&mut shot);
    assert_eq!(selected(&mut shot), "Home");
    // The focus follows the selection, so the keys keep working.
    shot.key(Key::Named(NamedKey::ArrowRight));
    shot.key(Key::Named(NamedKey::ArrowRight));
    rest(&mut shot);
    assert_eq!(selected(&mut shot), "Downloads");

    shot.key(Key::Named(NamedKey::Delete));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents"], "Delete closes the selected tab");
    assert_eq!(selected(&mut shot), "Documents", "the neighbour takes the selection");
    shot.key(Key::Named(NamedKey::ArrowLeft));
    rest(&mut shot);
    assert_eq!(selected(&mut shot), "Home", "the focus followed the close");

    shot.key_with(Key::Character("t".into()), Modifiers::CONTROL);
    rest(&mut shot);
    assert_eq!(tabs_of(&mut shot).len(), 3, "Ctrl+T adds a tab");
    assert_eq!(selected(&mut shot), "Tab 4");
    shot.key_with(Key::Character("w".into()), Modifiers::CONTROL);
    rest(&mut shot);
    assert_eq!(tabs_of(&mut shot).len(), 2, "Ctrl+W closes the selected tab");
}

#[test]
fn ctrl_shift_arrows_move_the_selected_tab() {
    let mut shot = live(600.0);
    shot.key(Key::Named(NamedKey::Tab));
    shot.key_with(Key::Named(NamedKey::ArrowRight), Modifiers::CONTROL | Modifiers::SHIFT);
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Home", "Downloads"]);
    assert_eq!(selected(&mut shot), "Home", "the moved tab stays selected");
    shot.key_with(Key::Named(NamedKey::ArrowLeft), Modifiers::CONTROL | Modifiers::SHIFT);
    shot.key_with(Key::Named(NamedKey::ArrowLeft), Modifiers::CONTROL | Modifiers::SHIFT);
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents", "Downloads"], "the first tab does not move left");
}

// ---- Closing ---------------------------------------------------------------

#[test]
fn a_lone_tab_has_a_close_button_and_every_way_to_close_it_asks() {
    let one_tab = || live_in(REGULAR, 300.0, Live { names: vec!["Home"], ..Live::new(600.0) }, true);
    let mut shot = one_tab();
    assert_eq!(close_buttons(&mut shot).len(), 1, "the lone tab shows its close button");
    let close = close_buttons(&mut shot)[0];
    shot.click(centre(&close).0, centre(&close).1);
    rest(&mut shot);
    assert!(names(&mut shot).is_empty(), "its button asked the host, which let it go");

    let mut shot = one_tab();
    shot.key(Key::Named(NamedKey::Tab));
    shot.key_with(Key::Character("w".into()), Modifiers::CONTROL);
    rest(&mut shot);
    assert!(names(&mut shot).is_empty(), "Ctrl+W on the lone tab asks too");

    let mut shot = one_tab();
    let tab = named(&mut shot, "Tab")[0];
    middle_click(&mut shot, (tab.min_x() + 30.0, centre(&tab).1));
    rest(&mut shot);
    assert!(names(&mut shot).is_empty(), "a middle click on the lone tab asks too");

    let mut shot = one_tab();
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::Delete));
    rest(&mut shot);
    assert!(names(&mut shot).is_empty(), "and so does Delete");
}

#[test]
fn every_tab_has_a_24px_close_button_inset_in_it_and_an_unselected_one_shows_it_on_hover() {
    let mut shot = live(600.0);
    let before = close_buttons(&mut shot);
    assert_eq!(before.len(), 1, "only the selected tab shows its button at rest");
    let tabs = named(&mut shot, "Tab");
    assert!(contains(&tabs[0], &before[0], 0.5), "the button is the selected tab's");
    assert!(near(before[0].width(), 24.0) && near(before[0].height(), 24.0), "the button is {}x{}, not 24", before[0].width(), before[0].height());
    let third = tabs[2];
    shot.hover(third.min_x() + 40.0, centre(&third).1);
    shot.settle(Duration::from_millis(200));
    let after = close_buttons(&mut shot);
    assert_eq!(after.len(), 2, "the tab under the pointer shows its button too");
    assert!(contains(&third, &after[1], 0.5));
    shot.hover(1.0, 250.0);
    shot.settle(Duration::from_millis(200));
    assert_eq!(close_buttons(&mut shot).len(), 1, "and gives it up when the pointer leaves");
}

#[test]
fn the_close_button_takes_its_press_and_the_bar_hears_none() {
    // A header bar drags the window when a press reaches it. A press on a
    // close button, a tab or the New tab button must not; a press on bare bar must.
    let presses = Presses(Rc::new(Cell::new(0)));
    let mut shot = live_in(REGULAR, 300.0, Live { drag: Some(presses.clone()), ..Live::new(600.0) }, true);
    let close = close_buttons(&mut shot)[0];
    shot.click(centre(&close).0, centre(&close).1);
    rest(&mut shot);
    assert_eq!(presses.0.get(), 0, "pressing a close button does not drag the window");
    assert_eq!(names(&mut shot), ["Documents", "Downloads"], "and it closed the tab");
    let tab = named(&mut shot, "Tab")[0];
    shot.click(tab.min_x() + 40.0, centre(&tab).1);
    assert_eq!(presses.0.get(), 0, "nor does pressing a tab");
    // Bare bar, right of the strip: the window is dragged from there.
    let bar = one(&mut shot, "TabBar");
    shot.click(bar.max_x() - 6.0, bar.min_y() + 8.0);
    assert_eq!(presses.0.get(), 1, "a press on the bar itself still drags the window");
    // The close button lies inside its tab, inside the strip's row.
    let strip = one(&mut shot, "TabStrip");
    for close in close_buttons(&mut shot) {
        assert!(contains(&strip, &close, 0.5), "the close button is inside the strip");
        assert!(close.min_y() >= bar.min_y() + geometry_for(REGULAR).top - 0.5, "and not in the band above the tabs");
    }
}

#[test]
fn closing_the_selected_tab_selects_its_right_neighbour_and_then_its_left() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    // Select the middle tab and close it with a middle click.
    shot.click(tabs[1].min_x() + 30.0, centre(&tabs[1]).1);
    assert_eq!(selected(&mut shot), "Documents");
    middle_click(&mut shot, (tabs[1].min_x() + 30.0, centre(&tabs[1]).1));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Downloads"]);
    assert_eq!(selected(&mut shot), "Downloads", "the neighbour to the right");
    // The last tab has only a left neighbour.
    shot.key(Key::Named(NamedKey::Tab));
    shot.key_with(Key::Character("w".into()), Modifiers::CONTROL);
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home"]);
    assert_eq!(selected(&mut shot), "Home", "else the one to the left");
}

#[test]
fn closing_a_tab_that_is_not_selected_leaves_the_selection_alone() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    middle_click(&mut shot, (tabs[2].min_x() + 30.0, centre(&tabs[2]).1));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents"]);
    assert_eq!(selected(&mut shot), "Home");
}

#[test]
fn a_middle_click_closes_the_tab_it_is_on() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut shot = live_in(REGULAR, 300.0, Live { closed_log: Some(log.clone()), ..Live::new(600.0) }, true);
    let tabs = named(&mut shot, "Tab");
    middle_click(&mut shot, (tabs[1].min_x() + 30.0, centre(&tabs[1]).1));
    rest(&mut shot);
    assert_eq!(*log.borrow(), [2], "tab 2 was closed");
    assert_eq!(names(&mut shot), ["Home", "Downloads"]);
}

#[test]
fn ctrl_shift_t_brings_back_the_last_closed_tab_at_its_place() {
    let mut shot = live(600.0);
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::ArrowRight));
    rest(&mut shot);
    shot.key(Key::Named(NamedKey::Delete));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Downloads"]);
    shot.key_with(Key::Character("T".into()), Modifiers::CONTROL | Modifiers::SHIFT);
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents", "Downloads"], "Documents is back between its neighbours");
    assert_eq!(selected(&mut shot), "Documents", "and selected");
    shot.key_with(Key::Character("T".into()), Modifiers::CONTROL | Modifiers::SHIFT);
    rest(&mut shot);
    assert_eq!(tabs_of(&mut shot).len(), 3, "with nothing more to bring back it does nothing, and adds no tab");
}

// ---- The context menu ------------------------------------------------------

fn menu_labels(shot: &mut Snapshot) -> Vec<String> {
    let wanted = ["Close tab", "Close other tabs", "Close tabs to the left", "Close tabs to the right", "Duplicate tab", "Reopen closed tab", "Move left", "Move right"];
    let found: Vec<String> = shot.runner().find_many(|_, element| {
        let label = Label::try_downcast(element)?;
        wanted.contains(&label.text.as_ref()).then(|| label.text.to_string())
    });
    wanted.iter().filter(|name| found.iter().any(|f| f == *name)).map(|name| name.to_string()).collect()
}

#[test]
fn a_right_click_opens_the_menu_with_every_action() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[1].min_x() + 30.0, centre(&tabs[1]).1);
    rest(&mut shot);
    assert_eq!(
        menu_labels(&mut shot),
        ["Close tab", "Close other tabs", "Close tabs to the left", "Close tabs to the right", "Duplicate tab", "Reopen closed tab", "Move left", "Move right"],
        "the middle tab's menu offers all eight, in the order of a browser's"
    );
}

#[test]
fn the_menus_close_actions_close_the_right_tabs() {
    // Close other tabs from the middle tab.
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[1].min_x() + 30.0, centre(&tabs[1]).1);
    rest(&mut shot);
    assert!(shot.click_text("Close other tabs"));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents"]);
    assert_eq!(selected(&mut shot), "Documents");

    // Close tabs to the right of the first.
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[0].min_x() + 30.0, centre(&tabs[0]).1);
    rest(&mut shot);
    assert!(shot.click_text("Close tabs to the right"));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home"]);

    // Close tab, Duplicate tab and Reopen closed tab.
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[2].min_x() + 30.0, centre(&tabs[2]).1);
    rest(&mut shot);
    assert!(shot.click_text("Duplicate tab"));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents", "Downloads", "Downloads"], "the copy is beside its tab");
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[0].min_x() + 30.0, centre(&tabs[0]).1);
    rest(&mut shot);
    assert!(shot.click_text("Close tab"));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Downloads", "Downloads"]);
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[0].min_x() + 30.0, centre(&tabs[0]).1);
    rest(&mut shot);
    assert!(shot.click_text("Reopen closed tab"));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents", "Downloads", "Downloads"], "Home is back at the start");
}

#[test]
fn the_ends_of_the_strip_leave_out_the_moves_that_cannot_happen() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[0].min_x() + 30.0, centre(&tabs[0]).1);
    rest(&mut shot);
    let labels = menu_labels(&mut shot);
    assert!(!labels.contains(&"Move left".to_string()), "the first tab cannot move left: {labels:?}");
    assert!(labels.contains(&"Move right".to_string()));
    assert!(labels.contains(&"Close tabs to the left".to_string()), "the item is there, greyed out: {labels:?}");
}

#[test]
fn close_tabs_to_the_left_is_greyed_out_on_the_first_tab_and_closes_the_tabs_before_the_others() {
    // On the first tab there is nothing to close: the item is disabled and
    // pressing it does nothing.
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[0].min_x() + 30.0, centre(&tabs[0]).1);
    rest(&mut shot);
    shot.click_text("Close tabs to the left");
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents", "Downloads"], "a disabled item closes nothing");

    // On the last tab it closes the two before it, and the tab stays selected.
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut shot = live_in(REGULAR, 300.0, Live { closed_log: Some(log.clone()), ..Live::new(600.0) }, true);
    let tabs = named(&mut shot, "Tab");
    shot.click(tabs[2].min_x() + 30.0, centre(&tabs[2]).1);
    rest(&mut shot);
    shot.right_click(tabs[2].min_x() + 30.0, centre(&tabs[2]).1);
    rest(&mut shot);
    assert!(shot.click_text("Close tabs to the left"));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Downloads"]);
    assert_eq!(selected(&mut shot), "Downloads");
    assert_eq!(*log.borrow(), [2, 1], "rightmost first, so Reopen brings them back left to right");

    // From the middle tab, with the first selected: the first goes, the
    // selection moves to the tab the menu was opened on, and nothing right of
    // it is touched.
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[1].min_x() + 30.0, centre(&tabs[1]).1);
    rest(&mut shot);
    assert!(shot.click_text("Close tabs to the left"));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Downloads"]);
    assert_eq!(selected(&mut shot), "Documents");

    // Reopen brings them back, in place.
    let tabs = named(&mut shot, "Tab");
    shot.right_click(tabs[0].min_x() + 30.0, centre(&tabs[0]).1);
    rest(&mut shot);
    assert!(shot.click_text("Reopen closed tab"));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents", "Downloads"]);
}

// ---- Rapid closure ---------------------------------------------------------

#[test]
fn the_tabs_keep_their_width_while_the_pointer_closes_tab_after_tab_and_grow_when_it_leaves() {
    // 600 px for three tabs, less the New tab cluster (40) and two 4 px gaps: each is 184 wide; for two they would be 240.
    let mut shot = live(600.0);
    let before = named(&mut shot, "Tab");
    let held = before[0].width();
    let g = geometry_for(REGULAR);
    let room = 600.0 - g.cluster(1);
    assert!(near(held, g.tab_in(room, 3.0)) && near(held, 184.0), "three tabs share the room: {held}");
    let first = before[0];
    // Move onto the first tab and close it with its button.
    let spot = (first.max_x() - 16.0, centre(&first).1);
    shot.hover(spot.0, spot.1);
    shot.settle(Duration::from_millis(150));
    shot.click(spot.0, spot.1);
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Downloads"]);
    let tabs = named(&mut shot, "Tab");
    assert!(near(tabs[0].width(), held), "the pointer is still in the strip: the tabs keep their width ({} not {held})", tabs[0].width());
    assert!(near(tabs[0].min_x(), first.min_x()), "and the next tab has come under the pointer");
    // The next close button is where the pointer is: pressing again closes the next tab.
    shot.click(spot.0, spot.1);
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Downloads"], "the second press, on the same spot, closed the next tab");
    let lone = named(&mut shot, "Tab")[0];
    assert!(near(lone.width(), held), "still held: {}", lone.width());
    // Leave the strip: the tab grows to the room.
    shot.hover(10.0, 250.0);
    rest(&mut shot);
    let grown = named(&mut shot, "Tab")[0];
    assert!(grown.width() > held + 20.0, "the tab grew when the pointer left: {} from {held}", grown.width());
    assert!(near(grown.width(), geometry_for(REGULAR).max), "to the widest a tab is: {}", grown.width());
}

#[test]
fn closing_with_the_keyboard_does_not_hold_the_widths() {
    let mut shot = live(600.0);
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::Delete));
    rest(&mut shot);
    let width = named(&mut shot, "Tab")[0].width();
    assert!(near(width, 240.0), "two tabs fill the room: {width}");
}

// ---- Dragging --------------------------------------------------------------

#[test]
fn the_new_tab_button_adds_and_the_tabs_never_leave_the_strip_while_they_move() {
    let mut shot = live(600.0);
    let new = *named(&mut shot, "IconButton").last().expect("the New tab button");
    let before = named(&mut shot, "Tab");
    assert_eq!(before.len(), 3);
    shot.click(new.min_x() + new.width() / 2.0, new.min_y() + new.height() / 2.0);
    let mut seen = BTreeSet::new();
    for _ in 0..30 {
        shot.settle(Duration::from_millis(16));
        let strip = one(&mut shot, "TabStrip");
        let tabs = named(&mut shot, "Tab");
        seen.insert(tabs.len());
        for tab in &tabs {
            assert!(tab.max_x() <= strip.max_x() + 0.6, "a tab at {tab:?} is outside the strip {strip:?} while a tab arrives");
            let bar = one(&mut shot, "TabBar");
            assert!(near(tab.min_y() - bar.min_y(), geometry_for(REGULAR).top), "a tab leaves its place in the bar while one arrives");
        }
        let first = tabs[0].width();
        assert!(
            tabs.iter().take(3).all(|tab| (tab.width() - first).abs() < 0.6),
            "the tabs that were already there keep one width while the new one grows"
        );
    }
    assert!(seen.contains(&4));
    rest(&mut shot);
    let after = named(&mut shot, "Tab");
    assert_eq!(after.len(), 4);
    assert!(after[0].width() < before[0].width(), "the tabs made room");
    assert_eq!(selected(&mut shot), "Tab 4", "the new tab is selected");
}

#[test]
fn closing_a_tab_with_its_button_shrinks_it_away_and_the_others_widen() {
    let mut shot = live(600.0);
    // The selected tab shows its close button.
    let close = close_buttons(&mut shot)[0];
    let before = named(&mut shot, "Tab")[1].width();
    shot.click(close.min_x() + close.width() / 2.0, close.min_y() + close.height() / 2.0);
    let mut widest_gap = 0.0_f32;
    let mut last_end: Option<f32> = None;
    for _ in 0..30 {
        shot.settle(Duration::from_millis(16));
        let strip = one(&mut shot, "TabStrip");
        let tabs = named(&mut shot, "Tab");
        for tab in &tabs {
            assert!(tab.max_x() <= strip.max_x() + 0.6, "a tab leaves the strip while one closes");
        }
        let end = tabs.last().unwrap().max_x();
        if let Some(last) = last_end {
            widest_gap = widest_gap.max((end - last).abs());
        }
        last_end = Some(end);
    }
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Downloads"]);
    assert!(named(&mut shot, "Tab")[0].width() >= before - 0.6, "the remaining tabs are no narrower");
    assert!(widest_gap < 60.0, "the strip closed ranks in steps, not in one jump ({widest_gap})");
}

#[test]
fn dragging_a_tab_past_its_neighbour_reorders_and_a_short_drag_does_not() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    let (a, c) = (tabs[0], tabs[2]);
    let y = a.min_y() + a.height() / 2.0;
    // A drag from the first tab to the end of the last.
    shot.drag((a.min_x() + 30.0, y), (c.max_x() - 10.0, y));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Downloads", "Home"]);
    assert_eq!(selected(&mut shot), "Home", "the dragged tab is the selected one");
    // A drag that stays on the tab only selects it.
    let b = named(&mut shot, "Tab")[1];
    shot.drag((b.min_x() + 30.0, y), (b.min_x() + 33.0, y));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Downloads", "Home"]);
}

#[test]
fn a_dragged_tab_follows_the_pointer_while_the_others_slide_aside() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    let (a, b) = (tabs[0], tabs[1]);
    let step = b.min_x() - a.min_x();
    let y = a.min_y() + a.height() / 2.0;
    let from = (a.min_x() + 30.0, y);
    shot.press(from);
    shot.move_to((from.0 + 20.0, y));
    shot.settle(Duration::from_millis(100));
    let held = named(&mut shot, "Tab")[0];
    assert!(near(held.min_x(), a.min_x() + 20.0), "the held tab follows the pointer: {} not {}", held.min_x(), a.min_x() + 20.0);
    // Past the middle of the next tab: it slides into the place the held tab left.
    shot.move_to((from.0 + step * 0.75, y));
    shot.settle(Duration::from_millis(400));
    let now = named(&mut shot, "Tab");
    assert!(near(now[1].min_x(), a.min_x() + step * 0.75), "still following");
    assert!(near(now[0].min_x(), a.min_x()), "the neighbour slid into the held tab's place: {} not {}", now[0].min_x(), a.min_x());
    assert_eq!(names(&mut shot).len(), 3);
    assert_eq!(names(&mut shot), ["Documents", "Home", "Downloads"], "the neighbour slid aside");
    // Nothing is reordered in the host's list until it is let go.
    shot.release((from.0 + step * 0.75, y));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Home", "Downloads"]);
    let settled = named(&mut shot, "Tab");
    assert!(near(settled[1].min_x(), b.min_x()), "it settled into its new place");
}

/// Paint order in Freya is the layer, the depth in the tree, so a tab's
/// contents would draw over the fill of a tab that overlaps it. A dragged tab
/// lies over its neighbour, so all of it must be in front.
#[test]
fn a_dragged_tab_is_painted_over_the_tab_it_overlaps() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    let (a, b) = (tabs[0], tabs[1]);
    let y = a.min_y() + a.height() / 2.0;
    let from = (a.min_x() + 30.0, y);
    shot.press(from);
    // Less than half a step: the neighbour has not slid aside yet, and the held
    // tab covers the start of its title.
    shot.move_to((from.0 + 70.0, y));
    shot.settle(Duration::from_millis(400));
    let title: Area = shot
        .runner()
        .find_many(|node, element| {
            let label = Label::try_downcast(element)?;
            (label.text == "Documents").then(|| node.layout().area)
        })
        .into_iter()
        .next()
        .expect("the neighbour's title");
    let held = named(&mut shot, "Tab")[0];
    assert!(held.max_x() > title.min_x() + 30.0 && held.max_x() < title.max_x() + 40.0, "the held tab {held:?} covers the start of the title {title:?}");
    // The held tab is the selected one: its opaque fill is the selection layer over the bar.
    let tokens = tokens_of(true);
    let surface = composite(Paint::Selection.color(&tokens), BAR.color(&tokens));
    let frame = Frame::of(&mut shot);
    let mut x = title.min_x();
    while x < title.min_x() + 28.0 {
        let mut row = title.min_y() + 1.0;
        while row < title.max_y() - 1.0 {
            let got = frame.pixel(x, row);
            assert!(same(got, surface, 1.0), "the neighbour's title shows through the held tab at ({x}, {row}): {got:?}, not {surface:?}");
            row += 1.0;
        }
        x += 1.0;
    }
    shot.release((from.0 + 70.0, y));
}

/// A drag takes its release, so the tab it began on never hears the pointer
/// go up: it must not stay drawn as pressed once another tab is selected and
/// the pointer is away.
#[test]
fn a_tab_that_was_dragged_is_not_left_looking_pressed() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    let (a, c) = (tabs[0], tabs[2]);
    let y = a.min_y() + a.height() / 2.0;
    let from = (a.min_x() + 30.0, y);
    shot.press(from);
    shot.move_to((from.0 + 40.0, y));
    shot.settle(Duration::from_millis(200));
    shot.release((from.0 + 40.0, y));
    rest(&mut shot);
    // Another tab takes the selection, and the pointer goes away.
    shot.hover(c.min_x() + 30.0, y);
    shot.click(c.min_x() + 30.0, y);
    shot.hover(1.0, 250.0);
    rest(&mut shot);
    assert_eq!(selected(&mut shot), "Downloads");
    let strip = BAR.color(&tokens_of(true));
    let frame = Frame::of(&mut shot);
    // Inside where its pill would be, above the title's glyphs.
    let got = frame.pixel(a.min_x() + 20.0, a.min_y() + 5.0);
    assert!(same(got, strip, 1.0), "the first tab is {got:?}, not the strip {strip:?}");
}

/// Moving from one tab to the next, in a header bar, leaves the first one not
/// hovered. The tab's pieces are keyed, so Freya never pairs one piece with
/// another and destroys the body the pointer had entered (no area, hence no
/// `PointerLeave`, and the tab stayed lit).
#[test]
fn a_tab_the_pointer_has_moved_off_is_not_left_hovered() {
    let mut shot = start(REGULAR, Case::files(3, 3));
    let tabs = named(&mut shot, "Tab");
    let (a, b) = (tabs[0], tabs[1]);
    let y = b.min_y() + b.height() / 2.0;
    shot.hover(a.min_x() + 30.0, y);
    shot.settle(Duration::from_millis(300));
    shot.hover(b.min_x() + 30.0, y);
    shot.settle(Duration::from_millis(300));
    let strip = BAR.color(&tokens_of(true));
    let frame = Frame::of(&mut shot);
    // Inside where its pill would be, above the title's glyphs.
    let got = frame.pixel(a.min_x() + 20.0, a.min_y() + 5.0);
    assert!(same(got, strip, 1.0), "the first tab is {got:?}, not the strip {strip:?}");
}

#[test]
fn escape_puts_a_dragged_tab_back() {
    let mut shot = live(600.0);
    let tabs = named(&mut shot, "Tab");
    let a = tabs[0];
    let y = a.min_y() + a.height() / 2.0;
    shot.press((a.min_x() + 30.0, y));
    shot.move_to((tabs[2].min_x() + 30.0, y));
    shot.settle(Duration::from_millis(200));
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    shot.release((tabs[2].min_x() + 30.0, y));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Home", "Documents", "Downloads"], "Escape cancels the move");
    assert!(near(named(&mut shot, "Tab")[0].min_x(), a.min_x()), "and the tab is back where it was");
}

#[test]
fn many_tabs_scroll_with_the_new_tab_button_pinned_and_the_selection_in_view() {
    let mut shot = live(300.0);
    let new_x = |shot: &mut Snapshot| named(shot, "IconButton").last().copied().expect("the New tab button").min_x();
    let pinned = new_x(&mut shot);
    shot.key(Key::Named(NamedKey::Tab));
    for _ in 0..8 {
        shot.key_with(Key::Character("t".into()), Modifiers::CONTROL);
        rest(&mut shot);
    }
    assert_eq!(tabs_of(&mut shot).len(), 11);
    assert_eq!(new_x(&mut shot), pinned, "the New tab button does not move while the tabs scroll");
    // The newest tab, selected, is scrolled into the region, which ends before the buttons.
    let strip = one(&mut shot, "TabStrip");
    let tab = named(&mut shot, "Tab").into_iter().last().unwrap();
    let tab_list = named(&mut shot, "IconButton").into_iter().rev().nth(1).expect("the tab list button");
    assert!(
        tab.max_x() <= tab_list.min_x() + 0.6 && tab.min_x() >= strip.min_x(),
        "the selected tab {tab:?} is in view, before the buttons ({tab_list:?}) in the strip {strip:?}"
    );
    shot.key(Key::Named(NamedKey::Home));
    rest(&mut shot);
    let first = named(&mut shot, "Tab")[0];
    assert!(first.min_x() >= strip.min_x() && first.max_x() <= tab_list.min_x() + 0.6, "Home scrolls back to the first tab");
}

// ---- The pictures ----------------------------------------------------------

/// With `TAB_PNG_DIR` set, writes the pictures that `design/tabs.md` and the
/// review of the tabs are made from: one, three and nine tabs, long titles,
/// hover, pressed, focus and a drag in progress, at mouse and at phone size,
/// in both schemes and both reading directions, and the Files window's title
/// bar. Without it, does nothing.
#[test]
fn tab_pictures() {
    let Some(dir) = std::env::var_os("TAB_PNG_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    for (window, height, size) in [(REGULAR, 220.0, "desktop"), (COMPACT, 800.0, "phone")] {
        for (dark, scheme) in [(true, "dark"), (false, "light")] {
            let container = window - 24.0;
            let save = |shot: &mut Snapshot, name: &str| {
                shot.save(dir.join(format!("{size}-{scheme}-{name}.png")));
            };
            let scene = |count: usize, selected: u64, long: bool| {
                let case = Case { dark, selected, long, ..Case::new(count, container) };
                let mut shot = Snapshot::new(move || Root(case).into_element(), (window, height));
                shot.settle(Duration::from_millis(400));
                shot
            };
            save(&mut scene(1, 1, false), "1-tab");
            save(&mut scene(3, 2, false), "3-tabs");
            save(&mut scene(3, 1, false), "3-tabs-first");
            save(&mut scene(9, 5, false), "9-tabs");
            save(&mut scene(3, 1, true), "long-titles");
            for (count, selected, name) in [(3_usize, 2_u64, "rtl-3-tabs"), (9, 5, "rtl-9-tabs")] {
                let case = Case { dark, selected, rtl: true, ..Case::new(count, container) };
                let mut shot = Snapshot::new(move || Root(case).into_element(), (window, height));
                shot.settle(Duration::from_millis(400));
                save(&mut shot, name);
            }
            let files = Case { dark, ..Case::files(3, 1) };
            let mut shot = Snapshot::new(move || Root(files).into_element(), (window, height));
            shot.settle(Duration::from_millis(400));
            save(&mut shot, "files-title-bar");

            // The pointer over an unselected tab, a finger down on one, and
            // the keyboard focus on the selected one.
            let mut shot = scene(3, 1, false);
            let third = named(&mut shot, "Tab")[2];
            shot.hover(third.min_x() + third.width() / 2.0, third.min_y() + third.height() / 2.0);
            shot.settle(Duration::from_millis(200));
            save(&mut shot, "hover");
            let mut shot = scene(3, 2, false);
            shot.key(Key::Named(NamedKey::Tab));
            shot.settle(Duration::from_millis(300));
            save(&mut shot, "focus");
            // A finger puts the window at touch density; the next mouse
            // movement puts it back.
            let mut shot = scene(3, 1, false);
            let third = named(&mut shot, "Tab")[2];
            let at = (third.min_x() + third.width() / 2.0, third.min_y() + third.height() / 2.0);
            let path = dir.join(format!("{size}-{scheme}-pressed.png"));
            shot.run_script(&format!("touch_down {},{}; wait 150; save {}", at.0, at.1, path.display()), "")
                .expect("the pressed script runs");
            shot.hover(1.0, 1.0);

            // A drag in progress: the first tab is held half way over the
            // second and the second has slid into its place.
            let mut shot = live_in(window, height, Live::new(container), dark);
            let tabs = named(&mut shot, "Tab");
            let (a, b) = (tabs[0], tabs[1]);
            let y = a.min_y() + a.height() / 2.0;
            let from = (a.min_x() + 24.0, y);
            shot.press(from);
            shot.move_to((from.0 + (b.min_x() - a.min_x()) * 0.8, y));
            shot.settle(Duration::from_millis(400));
            save(&mut shot, "drag");
            shot.release((from.0 + (b.min_x() - a.min_x()) * 0.8, y));
        }
    }
}

// ---- Tearing a tab off ------------------------------------------------------

/// A host that takes torn-off tabs, as Files does: it removes the tab from its
/// list, and writes down what it was given. `insertion` is the marker a window
/// shows for a tab brought in from another one.
#[derive(PartialEq)]
struct Tear {
    log: Rc<RefCell<Vec<Detach>>>,
    probe: StripProbe,
    insertion: Rc<Cell<Option<usize>>>,
    /// Whether the strip is given a detach handler at all.
    tearable: bool,
    count: usize,
}

impl Tear {
    fn new(count: usize) -> Self {
        Self {
            log: Rc::default(),
            probe: StripProbe::new(),
            insertion: Rc::default(),
            tearable: true,
            count,
        }
    }
    fn share(&self) -> Self {
        Self {
            log: self.log.clone(),
            probe: self.probe.clone(),
            insertion: self.insertion.clone(),
            tearable: self.tearable,
            count: self.count,
        }
    }
}

impl Component for Tear {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let count = self.count;
        let mut tabs = use_state(move || {
            (0..count).map(|index| TabItem::new(index as u64 + 1, format!("Folder {}", index + 1))).collect::<Vec<_>>()
        });
        let mut selected = use_state(|| 1_u64);
        let log = self.log.clone();
        let mut strip = tab_strip(tabs.read().clone(), selected())
            .on_select(move |id| selected.set(id))
            .on_reorder(move |(id, to): (u64, usize)| {
                let mut list = tabs.write();
                if let Some(from) = list.iter().position(|tab| tab.id == id) {
                    let tab = list.remove(from);
                    let to = to.min(list.len());
                    list.insert(to, tab);
                }
            })
            .insertion(self.insertion.get())
            .probe(&self.probe);
        if self.tearable {
            strip = strip.on_detach(move |detach: Detach| {
                log.borrow_mut().push(detach);
                // The host takes the tab: out of the list, the next one selected.
                let ids: Vec<u64> = tabs.read().iter().map(|tab| tab.id).collect();
                if let Some(next) = selection_after_close(&ids, &[detach.id], selected()) {
                    selected.set(next);
                }
                tabs.write().retain(|tab| tab.id != detach.id);
            });
        }
        let bar = rect()
            .component("TabBar")
            .tw(&tokens, &[w_full(), h(Dim::TabBar), bg(BAR), pl(S2)])
            .child(rect().component("Container").tw(&tokens, &[w(Dim::Page)]).child(strip));
        let page = rect().component("Page").tw(&tokens, &[w_full(), h_flex(), bg(Paint::Bg)]).child(caption("The page."));
        rect().tw(&tokens, &[w_full(), h_full(), flex_col()]).child(bar).child(page)
    }
}

#[derive(PartialEq)]
struct TearRoot(Tear);

impl Component for TearRoot {
    fn render(&self) -> impl IntoElement {
        let tokens = Tokens::new(DEFAULT_ACCENT, Scheme::PreferDark, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(OverlayHost::new(self.0.share()))
    }
}

fn tear_in(host: &Tear) -> Snapshot {
    let host = Rc::new(host.share());
    let mut shot = Snapshot::new(move || TearRoot(host.share()).into_element(), (REGULAR, 300.0));
    shot.settle(Duration::from_millis(400));
    shot
}

/// The strip's row, which is what the tear thresholds are measured from.
fn strip_row(shot: &mut Snapshot) -> Area {
    one(shot, "TabStrip")
}

/// Press the middle of tab `index` (0-based, left to right) and return the
/// point and the strip's height.
fn grab(shot: &mut Snapshot, index: usize) -> ((f32, f32), f32) {
    let tab = named(shot, "Tab")[index];
    let at = centre(&tab);
    shot.press(at);
    (at, strip_row(shot).height())
}

#[test]
fn a_tab_dragged_a_strip_height_below_the_strip_is_torn_off_once() {
    let host = Tear::new(3);
    let mut shot = tear_in(&host);
    let (at, strip) = grab(&mut shot, 1);
    let bottom = strip_row(&mut shot).max_y();
    // Sideways first, as a hand does, then down past the strip.
    shot.move_to((at.0 + 8.0, at.1));
    shot.settle(Duration::from_millis(100));
    assert!(host.log.borrow().is_empty(), "inside the strip nothing leaves");
    shot.move_to((at.0 + 8.0, bottom + strip + 2.0));
    shot.settle(Duration::from_millis(200));
    // Going on, and back, and on again, is still one tear: the tab has left.
    shot.move_to((at.0 + 40.0, bottom + 3.0 * strip));
    shot.settle(Duration::from_millis(200));
    shot.move_to((at.0, at.1));
    shot.settle(Duration::from_millis(200));
    let log = host.log.borrow().clone();
    assert_eq!(log.len(), 1, "one tab, torn off once: {log:?}");
    assert_eq!(log[0].id, 2, "the tab that was held");
    assert!(!log[0].released, "the button is still down");
    assert!(log[0].pointer.1 >= bottom + strip, "it was torn where the pointer had passed the threshold: {:?}", log[0]);
    // The grab is where the hand held it: inside the tab's body, from its left edge.
    let body = named(&mut shot, "Tab");
    assert!(log[0].grab.0 > 0.0 && log[0].grab.0 < 240.0, "the grab is on the body: {:?} of {body:?}", log[0].grab);
    // The press is where the button went down, not where the tab was torn: a
    // compositor that anchors a window move at the press needs the first.
    assert_eq!(log[0].press, at, "the press point: {:?}", log[0]);
    shot.release((at.0, at.1));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Folder 1", "Folder 3"], "the host took it: none lost, none left behind");
}

#[test]
fn drift_inside_the_threshold_neither_tears_nor_reorders_a_tab_far_from_its_place() {
    let host = Tear::new(3);
    let mut shot = tear_in(&host);
    let (at, strip) = grab(&mut shot, 0);
    let bottom = strip_row(&mut shot).max_y();
    // Out to 0.9 of a strip height, and wobbling there, but never to one.
    for dy in [0.2, 0.7, 0.9, 0.65, 0.9, 0.3] {
        shot.move_to((at.0 + 10.0, bottom + dy * strip));
        shot.settle(Duration::from_millis(80));
    }
    assert!(host.log.borrow().is_empty(), "drift is not a tear");
    shot.release((at.0 + 10.0, bottom + 0.3 * strip));
    rest(&mut shot);
    assert!(host.log.borrow().is_empty());
    assert_eq!(names(&mut shot), ["Folder 1", "Folder 2", "Folder 3"], "a tab let go near the strip stays");
}

#[test]
fn a_tab_taken_out_and_brought_back_before_the_threshold_is_not_torn() {
    let host = Tear::new(3);
    let mut shot = tear_in(&host);
    let (at, strip) = grab(&mut shot, 2);
    let bottom = strip_row(&mut shot).max_y();
    shot.move_to((at.0 - 20.0, bottom + 0.8 * strip));
    shot.settle(Duration::from_millis(150));
    shot.move_to((at.0 - 20.0, at.1));
    shot.settle(Duration::from_millis(150));
    shot.release((at.0 - 20.0, at.1));
    rest(&mut shot);
    assert!(host.log.borrow().is_empty());
    assert_eq!(names(&mut shot).len(), 3);
}

#[test]
fn a_tab_let_go_beyond_the_threshold_in_one_move_is_torn_as_it_is_released() {
    let host = Tear::new(3);
    let mut shot = tear_in(&host);
    let (at, strip) = grab(&mut shot, 1);
    let bottom = strip_row(&mut shot).max_y();
    let far = (at.0 + 5.0, bottom + 2.0 * strip);
    shot.move_to(far);
    shot.release(far);
    rest(&mut shot);
    let log = host.log.borrow().clone();
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(log[0].released, "the host is told that nothing is being dragged any more");
    assert_eq!(names(&mut shot), ["Folder 1", "Folder 3"]);
}

#[test]
fn a_tab_dragged_up_out_of_the_strip_is_torn_off_too() {
    let host = Tear::new(2);
    let mut shot = tear_in(&host);
    let (at, strip) = grab(&mut shot, 0);
    let top = strip_row(&mut shot).min_y();
    shot.move_to((at.0, top - 1.2 * strip));
    shot.settle(Duration::from_millis(200));
    assert_eq!(host.log.borrow().len(), 1, "up counts as well as down");
}

#[test]
fn a_lone_tab_is_torn_once_it_is_dragged_at_all_so_its_window_can_follow() {
    let host = Tear::new(1);
    let mut shot = tear_in(&host);
    let (at, _) = grab(&mut shot, 0);
    shot.move_to((at.0 + 3.0, at.1 + 2.0));
    shot.settle(Duration::from_millis(100));
    assert!(host.log.borrow().is_empty(), "a click that wobbles is a click");
    shot.move_to((at.0 + 30.0, at.1));
    shot.settle(Duration::from_millis(100));
    assert_eq!(host.log.borrow().len(), 1);
    assert_eq!(host.log.borrow()[0].id, 1);
}

#[test]
fn a_strip_without_a_detach_handler_keeps_its_tabs() {
    let mut host = Tear::new(3);
    host.tearable = false;
    let mut shot = tear_in(&host);
    let (at, strip) = grab(&mut shot, 1);
    let bottom = strip_row(&mut shot).max_y();
    shot.move_to((at.0, bottom + 3.0 * strip));
    shot.settle(Duration::from_millis(200));
    shot.release((at.0, bottom + 3.0 * strip));
    rest(&mut shot);
    assert!(host.log.borrow().is_empty());
    assert_eq!(names(&mut shot), ["Folder 1", "Folder 2", "Folder 3"], "a drag far from the strip puts the tab back");
}

#[test]
fn an_armed_tab_is_drawn_fainter_than_a_docked_one() {
    let host = Tear::new(3);
    let mut shot = tear_in(&host);
    let (at, strip) = grab(&mut shot, 1);
    let bottom = strip_row(&mut shot).max_y();
    let tab = named(&mut shot, "Tab")[1];
    shot.move_to((at.0 + 10.0, at.1));
    shot.settle(Duration::from_millis(200));
    let docked = Frame::of(&mut shot).pixel(tab.min_x() + 30.0 + 10.0, tab.min_y() + 5.0);
    // Past the arm distance, short of the tear distance.
    shot.move_to((at.0 + 10.0, bottom + 0.75 * strip));
    shot.settle(Duration::from_millis(250));
    assert!(host.log.borrow().is_empty());
    let armed = Frame::of(&mut shot).pixel(tab.min_x() + 30.0 + 10.0, tab.min_y() + 5.0);
    assert_ne!(docked, armed, "the held tab dims once going on would tear it off");
}

#[test]
fn the_strip_publishes_where_its_tabs_are() {
    let host = Tear::new(3);
    let mut shot = tear_in(&host);
    let layout = host.probe.layout();
    let tabs = named(&mut shot, "Tab");
    assert_eq!(layout.spans.len(), 3);
    for (span, tab) in layout.spans.iter().zip(&tabs) {
        assert!(near(span.0, tab.min_x()) && near(span.1, tab.max_x()), "{span:?} is not {tab:?}");
    }
    let row = strip_row(&mut shot);
    assert!(near(layout.rail.0, row.min_x()) && near(layout.rail.2, row.max_x()));
    assert!(near(layout.rail.1, row.min_y()) && near(layout.rail.3, row.max_y()));
    // The places a pointer over each tab asks for, from the real layout.
    for (index, tab) in tabs.iter().enumerate() {
        assert_eq!(layout.index_at(tab.min_x() + 4.0), index, "the left of tab {index}");
        assert_eq!(layout.index_at(tab.max_x() - 4.0), index + 1, "the right of tab {index}");
    }
}

#[test]
fn a_tab_brought_from_another_window_shows_its_marker_between_the_neighbours_it_will_have() {
    let host = Tear::new(3);
    let mut shot = tear_in(&host);
    let tabs = named(&mut shot, "Tab");
    assert!(named(&mut shot, "TabInsertion").is_empty(), "no marker without a tab to land");
    for at in 0..=3 {
        let host = Tear::new(3);
        host.insertion.set(Some(at));
        let mut shot = tear_in(&host);
        let markers = named(&mut shot, "TabInsertion");
        assert_eq!(markers.len(), 1, "one marker for place {at}");
        let marker = markers[0];
        let x = marker.min_x() + marker.width() / 2.0;
        // Between two tabs it is in the gap; at either end of the strip it is the strip's
        // own edge, which the marker's width can only reach into.
        let slack = if at == 0 || at == 3 { marker.width() } else { 0.6 };
        if at > 0 {
            assert!(x >= tabs[at - 1].max_x() - slack, "place {at}: the marker is after the tab before it");
        }
        if at < 3 {
            assert!(x <= tabs[at].min_x() + slack, "place {at}: the marker is before the tab after it");
        }
        assert!(near(marker.height(), tabs[0].height()), "it is a tab tall");
        // It is the accent, drawn over the tabs.
        let accent = Paint::Accent.color(&tokens_of(true));
        let got = Frame::of(&mut shot).pixel(x, marker.min_y() + marker.height() / 2.0);
        assert!(same(got, accent, 2.0), "place {at}: the marker is {got:?}, not the accent {accent:?}");
    }
}

// ---- Scrolling, hover, the fading title and right to left -------------------

/// A wheel event over `at`: `dx` and `dy` as Freya reports them (a positive `dy`
/// is the wheel turned down), from a notched wheel (`Line`) or a touchpad (`Pixel`).
fn wheel(shot: &mut Snapshot, at: (f32, f32), dx: f64, dy: f64, source: WheelSource) {
    let cursor = CursorPoint::new(at.0 as f64, at.1 as f64);
    shot.runner().send_event(PlatformEvent::Wheel {
        name: WheelEventName::Wheel,
        scroll: CursorPoint::new(dx, dy),
        cursor,
        source,
    });
    shot.runner().sync_and_update();
}

/// A live strip with 14 tabs in a 300 px window: it overflows, and the newest
/// tab, selected, is scrolled into view at the end.
fn crowded() -> Snapshot {
    let mut shot = live(300.0);
    shot.key(Key::Named(NamedKey::Tab));
    for _ in 0..11 {
        shot.key_with(Key::Character("t".into()), Modifiers::CONTROL);
        rest(&mut shot);
    }
    assert_eq!(tabs_of(&mut shot).len(), 14);
    shot
}

/// Why tab scrolling did not work: the effect that scrolled the selected tab into
/// view read the scroll controller's layout state, so it ran again on every
/// layout and pulled the strip back to the selected tab after each wheel notch.
/// The strip now keeps its own offset and nothing but a change of selection or
/// width moves it back.
#[test]
fn the_wheel_over_the_strip_scrolls_the_tabs_and_they_stay_where_it_left_them() {
    let mut shot = crowded();
    let strip = one(&mut shot, "TabStrip");
    let at = (strip.min_x() + 60.0, strip.min_y() + 20.0);
    let (first, end) = (named(&mut shot, "Tab")[0].min_x(), one(&mut shot, "TabEnd").min_x());
    assert!(first < strip.min_x() - 100.0, "the first tab is scrolled out to the left: {first}");
    // A notch up goes back towards the start, and glides there.
    wheel(&mut shot, at, 0.0, -128.0, WheelSource::Line);
    rest(&mut shot);
    let back = named(&mut shot, "Tab")[0].min_x();
    assert!(back > first + 100.0, "the wheel scrolled the tabs back: {first} to {back}");
    // It stays: time passing and the pointer moving do not send it back to the selected tab.
    shot.hover(at.0 + 6.0, at.1);
    shot.settle(Duration::from_millis(1200));
    assert!(near(named(&mut shot, "Tab")[0].min_x(), back), "the strip stayed where the wheel left it");
    // The vertical wheel turned down goes forwards again.
    wheel(&mut shot, at, 0.0, 128.0, WheelSource::Line);
    rest(&mut shot);
    assert!(named(&mut shot, "Tab")[0].min_x() < back - 50.0, "the wheel down goes towards the end");
    // A touchpad's sideways movement follows the fingers, and is followed at once.
    let before = named(&mut shot, "Tab")[0].min_x();
    wheel(&mut shot, at, -40.0, 0.0, WheelSource::Pixel);
    shot.settle(Duration::from_millis(50));
    assert!(near(named(&mut shot, "Tab")[0].min_x(), before + 40.0), "content moves right 40 px with the fingers");
    // The buttons at the end never move, and the scroll stops at both ends.
    assert!(near(one(&mut shot, "TabEnd").min_x(), end), "the New tab cluster is pinned");
    for _ in 0..30 {
        wheel(&mut shot, at, 0.0, -400.0, WheelSource::Pixel);
    }
    shot.settle(Duration::from_millis(100));
    assert!(near(named(&mut shot, "Tab")[0].min_x(), strip.min_x()), "the first tab stops at the start");
}

/// A finger dragged along the strip scrolls it, and the tab it began on is not selected.
#[test]
fn a_finger_dragged_along_the_strip_scrolls_it_without_selecting_a_tab() {
    let mut shot = crowded();
    let strip = one(&mut shot, "TabStrip");
    let y = strip.min_y() + 20.0;
    let first = named(&mut shot, "Tab")[0].min_x();
    let picked = selected(&mut shot);
    shot.swipe((strip.min_x() + 40.0, y), (strip.min_x() + 200.0, y));
    rest(&mut shot);
    assert!(named(&mut shot, "Tab")[0].min_x() > first + 100.0, "the tabs followed the finger");
    assert_eq!(selected(&mut shot), picked, "and a swipe is not a tap");
}

/// Exactly one tab is hovered, it fades in, and it is the one the pointer is over
/// even when the tabs move under a pointer that does not.
#[test]
fn one_tab_is_hovered_at_a_time_and_it_follows_the_tabs_under_a_resting_pointer() {
    let tokens = tokens_of(true);
    let (strip, hover) = (BAR.color(&tokens), composite(Paint::Hover.color(&tokens), BAR.color(&tokens)));
    // Four tabs: the pointer on the second, then the third.
    let mut shot = start(REGULAR, Case::files(4, 4));
    let tabs = named(&mut shot, "Tab");
    let y = tabs[1].min_y() + 20.0;
    let fill = |shot: &mut Snapshot, tab: &Area| Frame::of(shot).pixel(tab.min_x() + 20.0, tab.min_y() + 5.0);
    shot.hover(tabs[1].min_x() + 40.0, y);
    shot.settle(Duration::from_millis(40));
    let part = fill(&mut shot, &tabs[1]);
    assert!(!same(part, strip, 1.0) && !same(part, hover, 1.0), "a fill that is on its way in is between nothing and full: {part:?}");
    shot.settle(Duration::from_millis(400));
    assert!(same(fill(&mut shot, &tabs[1]), hover, 2.0), "the tab under the pointer has the hover fill");
    for other in [0, 2] {
        assert!(same(fill(&mut shot, &tabs[other]), strip, 1.0), "no other tab is lit");
    }
    shot.hover(tabs[2].min_x() + 40.0, y);
    shot.settle(Duration::from_millis(400));
    assert!(same(fill(&mut shot, &tabs[1]), strip, 1.0), "the tab the pointer left is not lit");
    assert!(same(fill(&mut shot, &tabs[2]), hover, 2.0), "the one it is on is");
    // Selected, hovered and pressed are three different looks.
    let selected_look = Frame::of(&mut shot).pixel(tabs[3].min_x() + 20.0, tabs[3].min_y() + 5.0);
    assert!(!same(selected_look, hover, 2.0) && !same(selected_look, strip, 2.0), "the selected tab is neither");

    // A pointer that rests while a wheel notch moves other tabs under it: the tab
    // it is on now is lit, and the one it was on is not.
    let mut shot = crowded();
    let strip_area = one(&mut shot, "TabStrip");
    let at = (strip_area.min_x() + 100.0, strip_area.min_y() + 20.0);
    shot.hover(at.0, at.1);
    shot.settle(Duration::from_millis(400));
    let on = |shot: &mut Snapshot| {
        named(shot, "Tab").into_iter().find(|tab| at.0 >= tab.min_x() && at.0 < tab.max_x()).expect("a tab under the pointer")
    };
    wheel(&mut shot, at, 0.0, -64.0, WheelSource::Line);
    shot.settle(Duration::from_millis(500));
    let now = on(&mut shot);
    assert!(at.0 >= now.min_x() && at.0 < now.max_x(), "a tab is under the resting pointer");
    let lit: Vec<Area> = named(&mut shot, "Tab")
        .into_iter()
        .filter(|tab| tab.max_x() > strip_area.min_x() && tab.min_x() < strip_area.max_x())
        .filter(|tab| same(Frame::of(&mut shot).pixel((tab.min_x() + 20.0).max(strip_area.min_x() + 1.0), tab.min_y() + 5.0), hover, 2.0))
        .collect();
    assert_eq!(lit.len(), 1, "one tab is lit after the tabs moved under the pointer: {lit:?}");
    assert!(at.0 >= lit[0].min_x() && at.0 < lit[0].max_x(), "and it is the one the pointer is over");
}

/// The darkest a pixel of the row is against `ground`, 0 to 255: how much ink.
fn ink_at(frame: &Frame, x: f32, rows: std::ops::Range<f32>, ground: Color) -> f32 {
    let luma = |c: Color| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
    let mut row = rows.start;
    let mut most = 0.0_f32;
    while row < rows.end {
        most = most.max((luma(ground) - luma(frame.pixel(x, row))).abs());
        row += 1.0;
    }
    most
}

/// A title (`FadeHost.1`) in a 200 px box on the page, in `direction`.
#[derive(PartialEq)]
struct FadeHost(Direction, &'static str);

impl Component for FadeHost {
    fn render(&self) -> impl IntoElement {
        let tokens = Tokens::new(DEFAULT_ACCENT, Scheme::PreferLight, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        let direction = self.0;
        WindowHost::new(directed(
            direction,
            rect()
                .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg), p(S4)])
                .child(rect().tw(&tokens, &[w(Dim::PlacesColumn)]).child(fade_text(self.1))),
        ))
    }
}

#[test]
fn a_title_that_does_not_fit_fades_out_at_the_end_of_the_reading_direction_and_has_no_ellipsis() {
    let ground = BAR.color(&tokens_of(false));
    for direction in [Direction::Ltr, Direction::Rtl] {
        let mut shot = Snapshot::new(move || FadeHost(direction, LONG).into_element(), (300.0, 80.0));
        shot.settle(Duration::from_millis(400));
        let text = one(&mut shot, "FadeText");
        assert!(near(text.width(), Dim::PlacesColumn.px()), "the box is the room it is given: {}", text.width());
        let frame = Frame::of(&mut shot);
        if let Some(dir) = std::env::var_os("TAB_PNG_DIR") {
            shot.save(std::path::PathBuf::from(dir).join(format!("fade-{direction:?}.png")));
        }
        let rows = text.min_y()..text.max_y();
        let fade = Dim::TextFade.px();
        let room = Dim::PlacesColumn.px();
        // Along the reading direction: `along` px from the start of the room.
        let at = |along: f32| match direction {
            Direction::Ltr => text.min_x() + along,
            Direction::Rtl => text.max_x() - 1.0 - along,
        };
        // Before the fade the text is solid somewhere.
        let solid = (0..(room - fade) as i32 - 1).map(|n| ink_at(&frame, at(n as f32), rows.clone(), ground)).fold(0.0_f32, f32::max);
        assert!(solid > 100.0, "{direction:?}: there is text before the fade ({solid})");
        // The glyphs' alpha goes down across the fade: the envelope of ink falls, to nothing at the end.
        let envelope = |from: f32| (0..6).map(|n| ink_at(&frame, at(from + n as f32), rows.clone(), ground)).fold(0.0_f32, f32::max);
        let (start, middle, end) = (envelope(room - fade), envelope(room - fade / 2.0 - 3.0), envelope(room - 5.0));
        assert!(start > middle && middle > end, "{direction:?}: the ink falls over the fade: {start} {middle} {end}");
        // The ramp is linear: five px from the end of 24 leaves about a fifth of the ink, and the last pixel next to nothing.
        assert!(end < 0.35 * start, "{direction:?}: near the end little is left: {end} of {start}");
        let last = ink_at(&frame, at(room - 1.0), rows.clone(), ground);
        assert!(last < 12.0, "{direction:?}: at the very end nothing is left, and no ellipsis stands there: {last}");
        // No ellipsis glyph past the room: nothing is drawn beyond it.
        let past = ink_at(&frame, at(room + 2.0), rows.clone(), ground);
        assert!(past < 2.0, "{direction:?}: nothing past the room: {past}");
        let labels: Vec<String> = shot.runner().find_many(|_, element| Label::try_downcast(element).map(|label| label.text.to_string()));
        assert!(labels.iter().all(|text| !text.contains('\u{2026}')), "{direction:?}: the text is whole, not cut with an ellipsis");
    }
}

/// The window pixels in `area`'s rows and in the columns `xs` whose colour differs
/// from `ground`, as the `(min_x, max_x)` they span, or `None` when there are none.
fn ink_span(frame: &Frame, area: &Area, xs: std::ops::Range<f32>, ground: Color) -> Option<(f32, f32)> {
    let mut span: Option<(f32, f32)> = None;
    let mut x = xs.start.floor().max(0.0);
    while x < xs.end.min(frame.width as f32) {
        let mut row = area.min_y();
        while row < area.max_y() {
            if !same(frame.pixel(x, row), ground, 24.0) {
                span = Some(span.map_or((x, x), |(low, high)| (low.min(x), high.max(x))));
                break;
            }
            row += 1.0;
        }
        x += 1.0;
    }
    span
}

#[test]
fn a_title_that_fits_is_drawn_inside_its_box_at_its_start_in_both_directions() {
    let ground = Paint::Bg.color(&tokens_of(false));
    let mut widths = Vec::new();
    for direction in [Direction::Ltr, Direction::Rtl] {
        let mut shot = Snapshot::new(move || FadeHost(direction, "Folder 2").into_element(), (300.0, 80.0));
        shot.settle(Duration::from_millis(400));
        let text = one(&mut shot, "FadeText");
        let frame = Frame::of(&mut shot);
        if let Some(dir) = std::env::var_os("TAB_PNG_DIR") {
            shot.save(std::path::PathBuf::from(dir).join(format!("fits-{direction:?}.png")));
        }
        let label: Area = shot.runner().find_many(|node, element| Label::try_downcast(element).map(|_| node.layout().area)).into_iter().next().expect("the label");
        let natural = label.width();
        let (low, high) = ink_span(&frame, &text, 0.0..frame.width as f32, ground).unwrap_or_else(|| panic!("{direction:?}: the title is drawn"));
        assert!(low >= text.min_x() - 0.5 && high <= text.max_x() + 0.5, "{direction:?}: the ink {low}..{high} is inside the box {text:?}");
        // Whole: the title is not cut, so it is about as wide as the text is.
        assert!(label.min_x() >= text.min_x() - 0.5 && label.max_x() <= text.max_x() + 0.5, "{direction:?}: the label {label:?} is inside the box {text:?}");
        assert!(high - low > natural - 4.0, "{direction:?}: the ink {low}..{high} is the whole {natural} px title");
        // It begins at the start edge of its box, to a glyph's side bearing.
        let gap = match direction {
            Direction::Ltr => low - text.min_x(),
            Direction::Rtl => text.max_x() - high,
        };
        assert!((0.0..4.0).contains(&gap), "{direction:?}: the title begins {gap} px from the start edge of {text:?}, ink {low}..{high}");
        widths.push(high - low);
    }
    assert!((widths[0] - widths[1]).abs() < 1.5, "the same title is as wide in both directions: {widths:?}");
}

#[test]
fn a_tabs_title_is_drawn_whole_at_its_start_in_both_directions() {
    for rtl in [false, true] {
        for (window, count) in [(REGULAR, 3), (COMPACT, 3), (REGULAR, 1)] {
            let mut shot = start(window, Case { rtl, ..Case::files(count, 2.min(count as u64)) });
            let frame = Frame::of(&mut shot);
            if let Some(dir) = std::env::var_os("TAB_PNG_DIR") {
                shot.save(std::path::PathBuf::from(dir).join(format!("titles-{window}-{count}-{}.png", if rtl { "rtl" } else { "ltr" })));
            }
            // The selected tab is in view in a Files window at any width; the others may be scrolled under the controls.
            let selected: Area = shot
                .runner()
                .find_many(|node, element| {
                    let data = element.accessibility();
                    (data.builder.class_name() == Some("Tab") && data.builder.is_selected() == Some(true)).then(|| node.layout().area)
                })
                .into_iter()
                .next()
                .expect("the selected tab");
            let titles: Vec<Area> = named(&mut shot, "FadeText").into_iter().filter(|title| selected.contains_rect(title)).collect();
            assert_eq!(titles.len(), 1, "{window}px rtl {rtl}: the selected tab has a title");
            let labels: Vec<Area> = shot.runner().find_many(|node, element| {
                Label::try_downcast(element).filter(|label| label.text.starts_with("Folder")).map(|_| node.layout().area)
            });
            let mut checked = 0;
            for title in &titles {
                let label = labels.iter().find(|label| label.intersects(title)).expect("the title's label");
                // A title that does not fit fades, which is tested elsewhere.
                if label.width() > title.width() + 0.5 {
                    continue;
                }
                checked += 1;
                // The title's own ground: its top left pixel is the tab's fill.
                let ground = frame.pixel(title.min_x() + 0.5, title.min_y() + 0.5);
                let (low, high) = ink_span(&frame, title, title.min_x()..title.max_x(), ground)
                    .unwrap_or_else(|| panic!("{window}px rtl {rtl}: the title at {title:?} is drawn"));
                assert!(high - low > label.width() - 4.0, "{window}px rtl {rtl}: the ink {low}..{high} is the whole {} px title in {title:?}", label.width());
                let gap = if rtl { title.max_x() - high } else { low - title.min_x() };
                assert!((0.0..4.0).contains(&gap), "{window}px rtl {rtl}: the title begins {gap} px from the start edge of {title:?}, label {label:?}, ink {low}..{high}");
            }
            assert!(checked > 0, "{window}px rtl {rtl}: a title that fits is on screen: {titles:?}");
        }
    }
}

#[test]
fn a_right_to_left_strip_is_mirrored() {
    let mut shot = start(REGULAR, Case { rtl: true, ..Case::new(3, 600.0) });
    let g = geometry_for(REGULAR);
    let tabs = named(&mut shot, "Tab");
    let strip = one(&mut shot, "TabStrip");
    // The first tab is at the right, the strip hugs the right edge of its room, and the
    // New tab button follows the last tab on its left.
    let first = tabs.iter().max_by(|a, b| a.min_x().total_cmp(&b.min_x())).unwrap();
    assert!(near(first.max_x(), strip.max_x()), "the first tab is at the right end of the strip: first {first:?} strip {strip:?} tabs {tabs:?}");
    let (cluster, buttons) = end_buttons(&mut shot);
    assert!(cluster.max_x() <= tabs[0].min_x() + 0.6, "the New tab cluster is left of the last tab");
    assert!(near(buttons[0].min_x() - strip.min_x(), g.inset) && near(strip.min_y(), buttons[0].min_y() - g.inset), "and has its insets on the left and the top");
    // A tab's close button is at its left and its title is right of it.
    let closes = named(&mut shot, "TabClose");
    for (tab, close) in tabs.iter().zip(&closes) {
        assert!(near(close.min_x() - tab.min_x(), g.inset), "the close button is {} from the tab's left edge, not the inset {}: tab {tab:?}, close {close:?}", close.min_x() - tab.min_x(), g.inset);
    }
    let titles = named(&mut shot, "FadeText");
    for (title, close) in titles.iter().zip(&closes) {
        assert!(title.min_x() >= close.max_x() - 0.5, "the title is after the close button");
    }
    // Names in screen order are the strip's order reversed.
    assert_eq!(names(&mut shot), ["Folder 3", "Folder 2", "Folder 1"]);
}
