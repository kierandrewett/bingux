//! A sidebar or pane toggle is on the side of the window its panel is on, and
//! draws that side (`shell::pane_toggle`).
//!
//! Rendered: a header bar with a toggle for a panel at the start and one for a
//! panel at the end puts the first in the start half of the bar and the second
//! in the end half, whatever order the bar was built in and whichever way the
//! interface reads; built in the order the docs give (the start toggle first,
//! the end toggle before the window buttons) they open the bar and close it
//! before the window buttons; and each glyph has its panel's side
//! inked (the glyph is read off the pixels, so a toggle that drew the other
//! side's icon fails). `pane_toggle_scan_tests` scans the source for a button
//! built by hand.

use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use torin::prelude::Area;

use super::pane_toggle::{PanelSide, pane_toggle};
use super::window::{HeaderBar, WindowControls};
use crate::controls::direction::{Direction, window_directed};
use crate::controls::icon_button;
use crate::icons::Icon;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const WIDTH: f32 = 640.0;
const PLACES: &str = "Places";
const PREVIEW: &str = "Preview pane";

/// Which half of a glyph's box carries most of its ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InkSide {
    Left,
    Right,
}

/// The side of `area` (a button's box, in the logical pixels of a window `logical_width` wide) that most
/// of the ink in `shot` is on: the glyph's weight, against the colour of the
/// pixel at the box's corner. A sidebar glyph is a window with its panel filled
/// at one edge, so the side of the weight is the side of the panel.
pub(crate) fn ink_side(shot: &mut Snapshot, area: Area, logical_width: f32) -> InkSide {
    let png = shot.runner().render();
    let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
    let mut surface =
        raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
    surface.canvas().draw_image(&image, (0, 0), None);
    let info = surface.canvas().image_info();
    let row = info.min_row_bytes();
    let mut bytes = vec![0u8; info.compute_byte_size(row)];
    assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
    // Logical pixels to device pixels.
    let scale = image.width() as f32 / logical_width;
    let luma = |x: usize, y: usize| {
        let at = y * row + x * 4;
        0.114 * f32::from(bytes[at])
            + 0.587 * f32::from(bytes[at + 1])
            + 0.299 * f32::from(bytes[at + 2])
    };
    let (x0, x1) = (
        (area.min_x() * scale) as usize,
        (area.max_x() * scale) as usize,
    );
    let (y0, y1) = (
        (area.min_y() * scale) as usize,
        (area.max_y() * scale) as usize,
    );
    let ground = luma(x0 + 1, y0 + 1);
    let centre = (x0 + x1) as f32 / 2.0;
    let (mut left, mut right) = (0.0_f32, 0.0_f32);
    for y in y0..y1 {
        for x in x0..x1 {
            let weight = (luma(x, y) - ground).abs();
            if (x as f32 + 0.5) < centre {
                left += weight;
            } else {
                right += weight;
            }
        }
    }
    assert!(left + right > 0.0, "the button {area:?} draws a glyph");
    if left > right {
        InkSide::Left
    } else {
        InkSide::Right
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Case {
    direction: Direction,
    /// The docs' order: the start toggle first, the end toggle after the other
    /// trailing actions. Otherwise both toggles are added after everything else.
    conventional: bool,
}

#[derive(PartialEq)]
struct Root(Case);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        window_directed(self.0.direction, WindowHost::new(Host(self.0)))
    }
}

#[derive(PartialEq)]
struct Host(Case);

impl Component for Host {
    fn render(&self) -> impl IntoElement {
        let places = || pane_toggle(PanelSide::Start, |()| {}).tooltip(PLACES);
        let preview = || pane_toggle(PanelSide::End, |()| {}).tooltip(PREVIEW);
        let mut bar = HeaderBar::new("Files").draggable(false);
        if self.0.conventional {
            bar = bar.pane_toggle(places());
        }
        bar = bar
            .leading(icon_button(Icon::GoPrevious, |()| {}).tooltip("Back"))
            .trailing(icon_button(Icon::Search, |()| {}).tooltip("Search"));
        if self.0.conventional {
            bar = bar.pane_toggle(preview()).trailing(WindowControls);
        } else {
            bar = bar
                .trailing(WindowControls)
                .pane_toggle(places())
                .pane_toggle(preview());
        }
        rect().expanded().child(bar)
    }
}

struct Measured {
    bar: Area,
    places: Area,
    preview: Area,
    back: Area,
    search: Area,
    controls: Area,
    places_ink: InkSide,
    preview_ink: InkSide,
}

fn measure(case: Case) -> Measured {
    let mut shot = Snapshot::new(move || Root(case).into_element(), (WIDTH, 160.0));
    shot.settle(Duration::from_millis(400));
    let button = |shot: &mut Snapshot, name: &str| {
        shot.icon_buttons()
            .into_iter()
            .find(|button| button.name == name)
            .unwrap_or_else(|| panic!("a button named {name}"))
            .area
    };
    let by_class = |shot: &mut Snapshot, name: &'static str| -> Area {
        shot.runner()
            .find_many(|node, element| {
                (element.accessibility().builder.class_name() == Some(name))
                    .then(|| node.layout().area)
            })
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("a {name}"))
    };
    let places = button(&mut shot, PLACES);
    let preview = button(&mut shot, PREVIEW);
    let places_ink = ink_side(&mut shot, places, WIDTH);
    let preview_ink = ink_side(&mut shot, preview, WIDTH);
    Measured {
        bar: by_class(&mut shot, "HeaderBar"),
        places,
        preview,
        back: button(&mut shot, "Back"),
        search: button(&mut shot, "Search"),
        controls: by_class(&mut shot, "WindowControls"),
        places_ink,
        preview_ink,
    }
}

const CASES: [Case; 4] = [
    Case {
        direction: Direction::Ltr,
        conventional: true,
    },
    Case {
        direction: Direction::Ltr,
        conventional: false,
    },
    Case {
        direction: Direction::Rtl,
        conventional: true,
    },
    Case {
        direction: Direction::Rtl,
        conventional: false,
    },
];

/// The distance from the start edge of the bar to the start edge of `area`, in
/// the direction the interface reads.
fn from_start(direction: Direction, bar: Area, area: Area) -> f32 {
    match direction {
        Direction::Ltr => area.min_x() - bar.min_x(),
        Direction::Rtl => bar.max_x() - area.max_x(),
    }
}

#[test]
fn the_toggle_of_a_start_panel_is_in_the_start_half_and_the_end_panels_in_the_end_half() {
    for case in CASES {
        let m = measure(case);
        let label = format!(
            "{:?}, conventional order {}",
            case.direction, case.conventional
        );
        let at = |area| from_start(case.direction, m.bar, area);
        assert!(
            at(m.places) < m.bar.width() / 2.0,
            "{label}: Places is in the start half"
        );
        assert!(
            at(m.preview) > m.bar.width() / 2.0,
            "{label}: the preview toggle is in the end half"
        );
        if case.conventional {
            // The start toggle opens the bar, before Back; the end toggle is after
            // Search and before the window buttons.
            assert!(
                at(m.places) < at(m.back),
                "{label}: Places is at {}, Back at {}; the start toggle opens the bar",
                at(m.places),
                at(m.back)
            );
            assert!(
                at(m.preview) > at(m.search),
                "{label}: the preview toggle ({}) is after Search ({})",
                at(m.preview),
                at(m.search)
            );
            assert!(
                at(m.preview) < at(m.controls),
                "{label}: the preview toggle ({}) is before the window buttons ({})",
                at(m.preview),
                at(m.controls)
            );
        }
    }
}

#[test]
fn each_toggle_draws_the_side_of_its_panel() {
    for case in CASES {
        let m = measure(case);
        // A glyph is mirrored in a right to left interface, as its panel is: the
        // start panel is at the right there.
        let (start, end) = match case.direction {
            Direction::Ltr => (InkSide::Left, InkSide::Right),
            Direction::Rtl => (InkSide::Right, InkSide::Left),
        };
        let label = format!("{:?}", case.direction);
        assert_eq!(
            m.places_ink, start,
            "{label}: Places draws its panel at the start edge"
        );
        assert_eq!(
            m.preview_ink, end,
            "{label}: the preview toggle draws its panel at the end edge"
        );
    }
}

#[test]
fn the_side_picks_the_glyph_and_both_glyphs_turn_over_in_a_right_to_left_interface() {
    assert_eq!(PanelSide::Start.icon(), Icon::SidebarShow);
    assert_eq!(PanelSide::End.icon(), Icon::SidebarShowRight);
    assert!(PanelSide::Start.icon().mirrors_in_rtl());
    assert!(PanelSide::End.icon().mirrors_in_rtl());
}
