//! The tab strip, rendered: the geometry of the attached tab at every width
//! and density, the colour of the seam to the pixel, and the keyboard and
//! pointer behaviour of the strip.

use std::collections::BTreeSet;
use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use freya_core::elements::label::Label;
use torin::prelude::Area;

use super::tab_frame::{STRIP, SURFACE, tab_frame};
use super::tab_shape::Geometry;
use super::tabs::{TabItem, tab_strip};
use crate::component::ComponentName;
use crate::contrast::{composite, ratio_opaque};
use crate::group::caption;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};
use crate::window::WindowHost;

const LONG: &str = "A name far too long for the room it is given, which must be cut";
const REGULAR: f32 = 900.0;
const COMPACT: f32 = 390.0;

/// A strip in a frame, and what it is built from.
#[derive(PartialEq, Clone, Copy)]
struct Case {
    count: usize,
    /// The width the strip may take.
    container: f32,
    add: bool,
    long: bool,
    dark: bool,
    selected: u64,
    /// A surface other than the page colour.
    surface: Option<Paint>,
}

impl Case {
    fn new(count: usize, container: f32) -> Self {
        Self { count, container, add: true, long: false, dark: true, selected: 1, surface: None }
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
        let bar = rect()
            .tw(&tokens, &[w_full(), h(Dim::Header)])
            .child(rect().component("Container").width(Size::px(case.container)).child(strip));
        let page = rect()
            .component("Page")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), p(S4), gap(S1)])
            .child(caption("The page the selected tab opens."))
            .child(caption("The tab and this page are one surface."));
        let mut frame = tab_frame(bar, page);
        if let Some(surface) = case.surface {
            frame = frame.surface(surface);
        }
        frame
    }
}

fn named(shot: &mut Snapshot, name: &'static str) -> Vec<Area> {
    shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area)
    })
}

/// The tabs left to right. While one is held it is drawn last, so the tree
/// order is not the order on screen.
fn tabs_left_to_right(shot: &mut Snapshot) -> Vec<Area> {
    let mut tabs = named(shot, "Tab");
    tabs.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()));
    tabs
}

fn one(shot: &mut Snapshot, name: &'static str) -> Area {
    named(shot, name).into_iter().next().unwrap_or_else(|| panic!("no {name} on screen"))
}

fn contains(outer: &Area, inner: &Area, slack: f32) -> bool {
    inner.min_x() >= outer.min_x() - slack
        && inner.max_x() <= outer.max_x() + slack
        && inner.min_y() >= outer.min_y() - slack
        && inner.max_y() <= outer.max_y() + slack
}

fn start(window: f32, case: Case) -> Snapshot {
    let mut shot = Snapshot::new(move || Root(case).into_element(), (window, 300.0));
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

// ---- The attached tab's geometry -------------------------------------------

/// Check one strip in one window.
fn check(window: f32, case: Case) {
    let what = format!(
        "{window}px window, {} tabs in {}px, add {}, long {}, selected {}",
        case.count, case.container, case.add, case.long, case.selected
    );
    let mut shot = start(window, case);
    let g = geometry_for(window);
    let (frame, bar, page) = (one(&mut shot, "TabFrame"), one(&mut shot, "TabBar"), one(&mut shot, "TabContent"));
    let strip = one(&mut shot, "TabStrip");
    let container = one(&mut shot, "Container");
    let tabs = named(&mut shot, "Tab");
    let closes = named(&mut shot, "TabClose");
    assert_eq!(tabs.len(), case.count, "{what}: a tab is drawn for each");

    // The bar's bottom edge is the content's top edge: no gap, no overlap.
    assert!(near(bar.height(), g.strip), "{what}: the bar is {} tall, not {}", bar.height(), g.strip);
    assert!(near(bar.max_y(), page.min_y()), "{what}: the bar ends at {} and the content begins at {}", bar.max_y(), page.min_y());
    assert!(contains(&frame, &bar, 0.01) && contains(&frame, &page, 0.01), "{what}: the frame holds both");
    assert!(near(bar.min_y() + page.min_y() - bar.max_y(), bar.min_y()), "{what}: the content follows the bar");

    // The selected tab's bottom edge is the content's top edge.
    let selected = tabs[case.selected as usize - 1];
    assert!(near(selected.max_y(), page.min_y()), "{what}: the selected tab ends at {} not at the content's top {}", selected.max_y(), page.min_y());
    assert!(near(selected.height(), g.tab), "{what}: a tab is {} tall, not {}", selected.height(), g.tab);
    assert!(near(selected.min_y() - bar.min_y(), g.top), "{what}: {} above the tabs, not {}", selected.min_y() - bar.min_y(), g.top);

    // Every tab stands on the same line, is as tall as the token, and is as
    // wide as the others.
    let width = tabs[0].width();
    for tab in &tabs {
        assert!(near(tab.max_y(), page.min_y()) && near(tab.height(), g.tab), "{what}: tabs share a baseline and a height");
        assert!(near(tab.width(), width), "{what}: tab widths differ: {} and {width}", tab.width());
    }
    assert!(width <= g.max + 0.6, "{what}: tabs are {width} wide, past the token");

    // Children inside the strip, the strip inside its container.
    assert!(contains(&container, &strip, 0.5), "{what}: the strip {strip:?} spills out of {container:?}");
    assert!(contains(&bar, &strip, 0.5), "{what}: the strip is outside the bar");
    assert!(near(strip.height(), g.strip), "{what}: the strip is {} tall", strip.height());

    // The cells the tab list and New tab buttons sit in, left to right.
    let buttons: Vec<Area> = {
        let mut all = named(&mut shot, "TabEnd");
        all.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()));
        all
    };
    for button in &buttons {
        assert!(contains(&strip, button, 0.5), "{what}: a button {button:?} is outside the strip {strip:?}");
        assert!(near(button.width(), g.button) && near(button.height(), g.tab), "{what}: a button cell is {}x{}", button.width(), button.height());
        // Level with the tabs: the cell is a tab tall and stands on the same line.
        assert!(near(button.max_y(), page.min_y()), "{what}: a button is level with the tabs");
    }
    if case.add {
        let new = buttons.last().expect("the New tab button");
        assert!(near(strip.max_x(), new.max_x()), "{what}: the New tab button ends the strip: strip {strip:?}, button {new:?}, buttons {buttons:?}");
    }

    // The close slot: inside its tab, the inset from its end, centred.
    assert_eq!(closes.len(), if case.count > 1 { case.count } else { 0 }, "{what}: a lone tab has no close slot");
    for (tab, close) in tabs.iter().zip(&closes) {
        assert!(contains(tab, close, 0.5), "{what}: the close slot is outside its tab");
        assert!(near(tab.max_x() - close.max_x(), g.inset), "{what}: the close slot is {} from the tab's end", tab.max_x() - close.max_x());
        assert!(near(close.min_y() - tab.min_y(), tab.max_y() - close.max_y()), "{what}: the close slot is centred in its tab");
    }

    // Titles are cut before the close slot.
    let titles: Vec<Area> = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        (label.text.starts_with("Folder") || label.text.starts_with("A name")).then(|| node.layout().area)
    });
    for (title, close) in titles.iter().zip(&closes) {
        assert!(title.max_x() <= close.min_x() + 0.5, "{what}: a title runs under the close slot");
    }

    let overflowing = g.content(case.count as f32, g.min) > case.container - if case.add { g.button } else { 0.0 } + 0.01;
    if overflowing {
        assert!(near(width, g.min), "{what}: scrolling tabs are {width} wide, not the minimum {}", g.min);
        assert!(near(strip.width(), container.width()), "{what}: an overflowing strip fills its container");
        assert!(buttons.len() >= 1 + usize::from(case.add), "{what}: the tab list button is drawn when tabs overflow");
        // The selected tab is scrolled into the region, when the region can
        // hold a tab and its feet at all.
        let region_end = buttons.first().map_or(strip.max_x(), |button| button.min_x());
        if region_end - strip.min_x() >= width + 2.0 * g.foot {
            assert!(selected.min_x() >= strip.min_x() - 0.6 && selected.max_x() <= region_end + 0.6, "{what}: the selected tab {selected:?} is not in view before {region_end}");
        }
    } else {
        // Bodies are a foot apart, the first a foot from the start, and the
        // last foot ends before the buttons.
        assert!(near(tabs[0].min_x() - strip.min_x(), g.foot), "{what}: the first tab is a foot from the start");
        for pair in tabs.windows(2) {
            assert!(near(pair[1].min_x() - pair[0].max_x(), g.gap()), "{what}: tabs are a gap apart");
        }
        let limit = buttons.first().map_or(strip.max_x(), |button| button.min_x());
        assert!(tabs.last().unwrap().max_x() + g.foot <= limit + 0.6, "{what}: the last tab's foot overlaps what follows");
        assert!(width >= g.min - 0.6, "{what}: tabs are {width} wide, under the minimum");
    }
}

#[test]
fn the_selected_tab_ends_where_the_content_begins_at_mouse_density() {
    for container in [260.0, 340.0, 440.0, 600.0, 760.0] {
        for (count, add, long, selected) in [
            (1, true, false, 1),
            (2, true, false, 2),
            (3, true, false, 1),
            (3, false, false, 3),
            (6, true, false, 4),
            (9, true, false, 9),
            (12, true, false, 1),
            (2, true, true, 1),
        ] {
            check(REGULAR, Case { add, long, selected, ..Case::new(count, container) });
        }
    }
}

#[test]
fn the_selected_tab_ends_where_the_content_begins_at_compact_density() {
    for container in [200.0, 280.0, 360.0] {
        for (count, add, long, selected) in [
            (1, true, false, 1),
            (3, true, false, 2),
            (6, true, false, 6),
            (9, true, false, 5),
            (2, true, true, 1),
        ] {
            check(COMPACT, Case { add, long, selected, ..Case::new(count, container) });
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
    for close in named(&mut shot, "TabClose") {
        assert!(near(close.width(), 44.0) && near(close.height(), 44.0), "the close target is {}x{}", close.width(), close.height());
    }
    for button in named(&mut shot, "IconButton") {
        assert!(near(button.width(), 44.0) && near(button.height(), 44.0), "a button is {}x{}", button.width(), button.height());
    }
}

// ---- The colour of the seam, to the pixel ----------------------------------

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

#[test]
fn there_is_no_seam_under_the_selected_tab_and_a_line_everywhere_else() {
    for dark in [true, false] {
        let tokens = tokens_of(dark);
        let (surface, strip) = (SURFACE.color(&tokens), STRIP.color(&tokens));
        let line = composite(Paint::Separator.color(&tokens), strip);
        let case = Case { dark, selected: 2, ..Case::new(3, 600.0) };
        let mut shot = start(REGULAR, case);
        let page = one(&mut shot, "TabContent");
        let tabs = named(&mut shot, "Tab");
        let frame = Frame::of(&mut shot);
        let seam = page.min_y();
        let what = if dark { "dark" } else { "light" };

        // Under the selected tab: the same colour on both sides of the seam,
        // across its whole body, three rows above it to three below.
        let selected = tabs[1];
        let mut x = selected.min_x() + 4.0;
        while x < selected.max_x() - 4.0 {
            for dy in [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0] {
                let got = frame.pixel(x, seam + dy);
                assert!(same(got, surface, 1.0), "{what}: at x {x}, {dy} from the seam the pixel {got:?} is not the surface {surface:?}");
            }
            x += 7.0;
        }
        // Above the tab's body, but still inside its top: the surface.
        let inside = frame.pixel(selected.min_x() + 4.0, selected.min_y() + 4.0);
        assert!(same(inside, surface, 1.0), "{what}: the tab's body is the surface, not {inside:?}");
        // Beside the tabs the strip is one step off the surface, and the
        // line is on the seam's last row above the content.
        let beside = frame.pixel(tabs[0].min_x() + 30.0, selected.min_y() - 4.0);
        assert!(same(beside, strip, 1.0), "{what}: the strip behind is {beside:?}, not {strip:?}");
        assert!(!same(strip, surface, 2.0), "{what}: the strip is one step off the surface");
        let under_other = frame.pixel(tabs[0].min_x() + 30.0, seam - 1.0);
        assert!(same(under_other, line, 2.0), "{what}: the hairline under an unselected tab is {under_other:?}, not {line:?}");
        let below = frame.pixel(tabs[0].min_x() + 30.0, seam + 1.0);
        assert!(same(below, surface, 1.0), "{what}: the content below the hairline is the surface");
        // Left and right of the whole strip, the hairline runs on.
        let beyond = frame.pixel(page.max_x() - 10.0, seam - 1.0);
        assert!(same(beyond, line, 2.0), "{what}: the hairline runs under the rest of the bar: {beyond:?}");
        // The tab is the surface and the content is the surface: one colour.
        assert!(ratio_opaque(inside, frame.pixel(selected.min_x() + 4.0, seam + 8.0)) < 1.001, "{what}: tab and content differ");
    }
}

#[test]
fn the_flares_are_concave_and_join_the_bottom_edge() {
    let g = Geometry::in_class(SizeClass::Regular);
    for dark in [true, false] {
        let tokens = tokens_of(dark);
        let (surface, strip) = (SURFACE.color(&tokens), STRIP.color(&tokens));
        let mut shot = start(REGULAR, Case { dark, selected: 2, ..Case::new(3, 600.0) });
        let page = one(&mut shot, "TabContent");
        let selected = named(&mut shot, "Tab")[1];
        let frame = Frame::of(&mut shot);
        let base = page.min_y();
        let what = if dark { "dark" } else { "light" };
        // The left flare's circle is centred a foot out from the wall and a
        // foot and a line up from the base.
        let (cx, cy) = (selected.min_x() - g.foot, base - g.foot - g.hairline);
        // Near the circle's centre: the strip, curving round the foot.
        let inside = frame.pixel(cx + 1.5, cy + 2.0);
        assert!(same(inside, strip, 1.0), "{what}: inside the left flare is {inside:?}, not the strip {strip:?}");
        // Below and beside the arc, against the wall and the base: the surface.
        let outside = frame.pixel(selected.min_x() - 1.5, base - 2.0);
        assert!(same(outside, surface, 1.0), "{what}: the foot of the left wall is {outside:?}, not the surface {surface:?}");
        // The same on the right.
        let (rx, ry) = (selected.max_x() + g.foot, cy);
        let inside = frame.pixel(rx - 1.5, ry + 2.0);
        assert!(same(inside, strip, 1.0), "{what}: inside the right flare is {inside:?}");
        let outside = frame.pixel(selected.max_x() + 1.5, base - 2.0);
        assert!(same(outside, surface, 1.0), "{what}: the foot of the right wall is {outside:?}");
        // The foot ends on the strip's hairline: the last row above the seam,
        // just before the foot, is the hairline over the strip.
        let line = composite(Paint::Separator.color(&tokens), strip);
        let before = frame.pixel(selected.min_x() - g.foot - 3.0, base - 1.0);
        assert!(same(before, line, 2.0), "{what}: the hairline before the foot is {before:?}, not {line:?}");
    }
}

#[test]
fn the_frame_gives_its_content_and_the_selected_tab_one_paint() {
    for dark in [true, false] {
        let tokens = tokens_of(dark);
        let card = Paint::Card.color(&tokens);
        let mut shot = start(REGULAR, Case { dark, surface: Some(Paint::Card), ..Case::new(3, 600.0) });
        let page = one(&mut shot, "TabContent");
        let selected = named(&mut shot, "Tab")[0];
        let frame = Frame::of(&mut shot);
        let what = if dark { "dark" } else { "light" };
        let content = frame.pixel(page.min_x() + 40.0, page.min_y() + 40.0);
        assert!(same(content, card, 1.0), "{what}: the content is {content:?}, not the frame's surface {card:?}");
        let tab = frame.pixel(selected.min_x() + 4.0, selected.min_y() + 4.0);
        assert!(same(tab, card, 1.0), "{what}: the selected tab is {tab:?}, not the frame's surface {card:?}");
        let seam = frame.pixel(selected.min_x() + 4.0, page.min_y() - 1.0);
        assert!(same(seam, card, 1.0), "{what}: the seam under the tab is {seam:?}");
    }
}

// ---- Interaction -----------------------------------------------------------

/// A strip that keeps its own tabs, like a host does.
#[derive(PartialEq)]
struct Live(f32);

impl Component for Live {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut tabs = use_state(|| {
            vec![TabItem::new(1, "Home"), TabItem::new(2, "Documents"), TabItem::new(3, "Downloads")]
        });
        let mut selected = use_state(|| 1_u64);
        let mut next = use_state(|| 4_u64);
        let strip = tab_strip(tabs.read().clone(), selected())
            .on_select(move |id| selected.set(id))
            .on_close(move |id| {
                let mut list = tabs.write();
                let Some(index) = list.iter().position(|tab| tab.id == id) else {
                    return;
                };
                list.remove(index);
                if selected() == id {
                    selected.set(list[index.min(list.len() - 1)].id);
                }
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
        let bar = rect()
            .tw(&tokens, &[w_full(), h(Dim::Header)])
            .child(rect().component("Container").width(Size::px(self.0)).child(strip));
        let page = rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), p(S4), gap(S1)])
            .child(caption("The page the selected tab opens."));
        tab_frame(bar, page)
    }
}

#[derive(PartialEq)]
struct LiveRoot(f32, bool);

impl Component for LiveRoot {
    fn render(&self) -> impl IntoElement {
        let scheme = if self.1 { Scheme::PreferDark } else { Scheme::PreferLight };
        let tokens = Tokens::new(DEFAULT_ACCENT, scheme, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(Live(self.0))
    }
}

fn live(width: f32) -> Snapshot {
    live_in(REGULAR, 300.0, width, true)
}

/// A live strip `width` wide in a window of `window` by `height`.
fn live_in(window: f32, height: f32, width: f32, dark: bool) -> Snapshot {
    let mut shot = Snapshot::new(move || LiveRoot(width, dark).into_element(), (window, height));
    shot.settle(Duration::from_millis(400));
    shot
}

/// The tabs in order: name and whether it is the selected one.
fn tabs_of(shot: &mut Snapshot) -> Vec<(String, bool)> {
    shot.runner().find_many(|_, element| {
        let data = element.accessibility();
        (data.builder.class_name() == Some("Tab")).then(|| {
            (
                data.builder.label().unwrap_or_default().to_string(),
                data.builder.is_selected() == Some(true),
            )
        })
    })
}

fn names(shot: &mut Snapshot) -> Vec<String> {
    tabs_of(shot).into_iter().map(|(name, _)| name).collect()
}

/// The names left to right, as the person sees them: a held tab is drawn
/// last, so the tree order is not the order on screen.
fn on_screen(shot: &mut Snapshot) -> Vec<String> {
    let mut found: Vec<(f32, String)> = shot.runner().find_many(|node, element| {
        let data = element.accessibility();
        (data.builder.class_name() == Some("Tab"))
            .then(|| (node.layout().area.min_x(), data.builder.label().unwrap_or_default().to_string()))
    });
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, name)| name).collect()
}

fn selected(shot: &mut Snapshot) -> String {
    tabs_of(shot).into_iter().find(|(_, selected)| *selected).map(|(name, _)| name).unwrap_or_default()
}

fn rest(shot: &mut Snapshot) {
    shot.settle(Duration::from_millis(500));
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

#[test]
fn a_lone_tab_cannot_be_closed() {
    let mut shot = live(600.0);
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::Delete));
    rest(&mut shot);
    shot.key(Key::Named(NamedKey::Delete));
    rest(&mut shot);
    assert_eq!(tabs_of(&mut shot).len(), 1);
    shot.key(Key::Named(NamedKey::Delete));
    rest(&mut shot);
    assert_eq!(tabs_of(&mut shot).len(), 1, "the last tab stays");
    assert!(named(&mut shot, "TabClose").is_empty(), "and it has no close button");
}

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
            assert!(near(tab.max_y(), one(&mut shot, "TabContent").min_y()), "a tab leaves the baseline while one arrives");
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
    let close = named(&mut shot, "IconButton").into_iter().find(|button| button.width() < 24.0).expect("the selected tab's close button");
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
    let held = tabs_left_to_right(&mut shot)[0];
    assert!(near(held.min_x(), a.min_x() + 20.0), "the held tab follows the pointer: {} not {}", held.min_x(), a.min_x() + 20.0);
    // Past the middle of the next tab: it slides into the place the held tab left.
    shot.move_to((from.0 + step * 0.75, y));
    shot.settle(Duration::from_millis(400));
    let now = tabs_left_to_right(&mut shot);
    assert!(near(now[1].min_x(), a.min_x() + step * 0.75), "still following");
    assert!(near(now[0].min_x(), a.min_x()), "the neighbour slid into the held tab's place: {} not {}", now[0].min_x(), a.min_x());
    assert_eq!(names(&mut shot).len(), 3);
    assert_eq!(on_screen(&mut shot), ["Documents", "Home", "Downloads"], "the neighbour slid aside");
    // Nothing is reordered in the host's list until it is let go.
    shot.release((from.0 + step * 0.75, y));
    rest(&mut shot);
    assert_eq!(names(&mut shot), ["Documents", "Home", "Downloads"]);
    let settled = named(&mut shot, "Tab");
    assert!(near(settled[1].min_x(), b.min_x()), "it settled into its new place");
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
/// in both schemes. Without it, does nothing.
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
            save(&mut scene(9, 5, false), "9-tabs");
            save(&mut scene(3, 1, true), "long-titles");

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
            let mut shot = live_in(window, height, container, dark);
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
