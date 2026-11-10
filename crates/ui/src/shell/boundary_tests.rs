//! The window shell paints the tiers and keeps chrome controls out of the
//! content (`design/boundaries.md`): the header and the sidebar share one
//! chrome tone, the content has its own, the strip a third; a view switcher
//! sits in the header on a desktop and in a row under it on a phone, never in
//! the content; the footer runs the full width under the sidebar; and a
//! switcher drawn inside the content is reported.

use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use torin::prelude::Area;

use super::boundary::{View, clear_violations, view, view_switcher, violations};
use super::screen::Screen;
use super::window::HeaderBar;
use crate::component::ComponentName;
use crate::icons::Icon;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const DESKTOP: (f32, f32) = (1000.0, 700.0);
const PHONE: (f32, f32) = (390.0, 800.0);

#[derive(Clone, Copy, PartialEq)]
enum Case {
    /// A strip, a header with a switcher (views with icons), a sidebar, the
    /// content and a footer.
    Full,
    /// The same window with views that have no icons.
    Plain,
    /// A header bar with a switcher drawn inside the content.
    Nested,
}

#[derive(PartialEq)]
struct Root(Case);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(Host(self.0))
    }
}

#[derive(PartialEq)]
struct Host(Case);

fn modes(icons: bool) -> Vec<View> {
    let views = [
        ("basic", "Basic", Icon::AccessoriesCalculator),
        ("scientific", "Scientific", Icon::ViewGrid),
    ];
    views
        .into_iter()
        .map(|(key, label, glyph)| {
            let one = view(key, label);
            if icons { one.icon(glyph) } else { one }
        })
        .collect()
}

impl Component for Host {
    fn render(&self) -> impl IntoElement {
        let box_named = |name: &'static str| rect().component(name).expanded();
        match self.0 {
            Case::Full | Case::Plain => {
                Screen::new(HeaderBar::new("Calculator").switcher(view_switcher(
                    modes(self.0 == Case::Full),
                    "basic",
                    |_| {},
                )))
                .strip(HeaderBar::new("").tab_slot(rect().component("Tabs").expanded()))
                .sidebar(box_named("SidebarBody"))
                .content(box_named("ContentBody"))
                .footer(
                    rect()
                        .component("FooterBody")
                        .width(Size::fill())
                        .height(Size::px(48.0)),
                )
            }
            Case::Nested => Screen::new(HeaderBar::new("Calculator")).content(
                HeaderBar::new("Inner").switcher(view_switcher(modes(false), "basic", |_| {})),
            ),
        }
    }
}

fn shot(case: Case, size: (f32, f32)) -> Snapshot {
    let mut shot = Snapshot::new(move || Root(case).into_element(), size);
    shot.settle(Duration::from_millis(400));
    shot
}

fn area(shot: &mut Snapshot, name: &'static str) -> Area {
    shot.runner()
        .find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area)
        })
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("a {name}"))
}

/// The window's header: the second bar, under the strip.
fn header(shot: &mut Snapshot) -> Area {
    let bars = shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some("HeaderBar"))
            .then(|| node.layout().area)
    });
    assert_eq!(bars.len(), 2, "the strip and the header");
    bars.into_iter()
        .max_by(|a, b| a.min_y().total_cmp(&b.min_y()))
        .expect("the header")
}
/// The colour at a window point of the frame as drawn.
fn pixel(shot: &mut Snapshot, at: (f32, f32)) -> (u8, u8, u8) {
    let png = shot.runner().render();
    let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
    let mut surface =
        raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
    surface.canvas().draw_image(&image, (0, 0), None);
    let info = surface.canvas().image_info();
    let row = info.min_row_bytes();
    let mut bytes = vec![0u8; info.compute_byte_size(row)];
    assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
    // A snapshot draws at one pixel per point.
    let scale = image.width() as f32 / shot.runner().run_in(|| crate::surface::window_size().width);
    let i = (at.1 * scale).round() as usize * row + (at.0 * scale).round() as usize * 4;
    (bytes[i + 2], bytes[i + 1], bytes[i])
}

#[test]
fn the_shell_paints_three_tiers() {
    let mut shot = shot(Case::Full, DESKTOP);
    let strip = area(&mut shot, "Tabs");
    let header = header(&mut shot);
    let sidebar = area(&mut shot, "SidebarBody");
    let content = area(&mut shot, "ContentBody");
    // The empty middle of each region: the header to the side of its switcher.
    let strip_tone = pixel(&mut shot, (strip.max_x() - 200.0, strip.center().y));
    let header_tone = pixel(&mut shot, (header.min_x() + 120.0, header.center().y));
    let sidebar_tone = pixel(&mut shot, (sidebar.center().x, sidebar.center().y));
    let content_tone = pixel(&mut shot, (content.center().x, content.center().y));
    assert_eq!(
        header_tone, sidebar_tone,
        "the header and the sidebar are one chrome tone"
    );
    assert_ne!(content_tone, header_tone, "the content has its own tone");
    assert_ne!(strip_tone, header_tone, "the strip is off the chrome");
    assert_ne!(strip_tone, content_tone, "the strip is off the content");
}

#[test]
fn the_switcher_sits_in_the_header_on_a_desktop() {
    let mut shot = shot(Case::Full, DESKTOP);
    let header = header(&mut shot);
    let switcher = area(&mut shot, "ViewSwitcher");
    let content = area(&mut shot, "ContentBody");
    assert!(switcher.min_y() >= header.min_y() && switcher.max_y() <= header.max_y());
    assert!(switcher.max_y() <= content.min_y(), "above the content");
    let centre = (switcher.center().x - header.center().x).abs();
    assert!(centre < 2.0, "centred on the bar, off by {centre}");
}

#[test]
fn a_switcher_with_icons_is_the_bottom_bar_on_a_phone() {
    let mut shot = shot(Case::Full, PHONE);
    let header = header(&mut shot);
    let bar = area(&mut shot, "BottomBar");
    let content = area(&mut shot, "ContentBody");
    let footer = area(&mut shot, "ScreenFooter");
    // From the window's bottom edge up: the bottom bar, the footer, the content.
    let seen = format!("bar {bar:?}, footer {footer:?}, content {content:?}, header {header:?}");
    assert!(
        bar.min_y() >= footer.max_y() - 0.5,
        "under the footer: {seen}"
    );
    assert!(
        footer.min_y() >= content.max_y() - 0.5,
        "the footer under the content: {seen}"
    );
    assert!(bar.max_y() <= PHONE.1 + 0.5, "inside the window: {seen}");
    assert!(
        bar.max_y() >= PHONE.1 - 1.0,
        "on the window's bottom edge: {seen}"
    );
    assert!(
        header.max_y() <= content.min_y(),
        "the header keeps the top"
    );
    let in_header = shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some("ViewSwitcher"))
            .then(|| node.layout().area)
    });
    assert!(in_header.is_empty(), "no second switcher at the top");
}

#[test]
fn a_switcher_without_icons_is_a_menu_in_the_one_row_header_on_a_phone() {
    let mut shot = shot(Case::Plain, PHONE);
    let header = header(&mut shot);
    let switcher = area(&mut shot, "ViewSwitcher");
    let content = area(&mut shot, "ContentBody");
    assert!(
        switcher.min_y() >= header.min_y() && switcher.max_y() <= header.max_y(),
        "in the header's one row: {switcher:?} in {header:?}"
    );
    assert!(header.max_y() <= content.min_y(), "above the content");
    let bars = shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some("BottomBar"))
            .then(|| node.layout().area)
    });
    assert!(bars.is_empty(), "no bottom bar without icons");
}

#[test]
fn the_footer_runs_under_the_sidebar() {
    let mut shot = shot(Case::Full, DESKTOP);
    let sidebar = area(&mut shot, "SidebarBody");
    let footer = area(&mut shot, "ScreenFooter");
    assert!(
        footer.min_x() <= 0.5 && footer.width() >= DESKTOP.0 - 0.5,
        "full width"
    );
    assert!(
        sidebar.max_y() <= footer.min_y(),
        "the sidebar ends above it"
    );
}

#[test]
fn a_switcher_inside_the_content_is_reported() {
    clear_violations();
    let _ = shot(Case::Full, DESKTOP);
    assert!(violations().is_empty(), "{:?}", violations());
    let _ = shot(Case::Nested, DESKTOP);
    let found = violations();
    assert!(
        found
            .iter()
            .any(|line| line.starts_with("ViewSwitcher is drawn inside the content")),
        "{found:?}"
    );
    clear_violations();
}
