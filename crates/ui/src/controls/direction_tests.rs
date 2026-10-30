//! Reading direction, rendered: what is painted is the reflection of what is
//! written for left to right (borders, corner radii, shadows, gradients), and
//! what the engine lays out is its reflection too.

use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use torin::prelude::Area;

use super::direction::{Direction, directed, window_directed};
use crate::component::ComponentName;
use crate::snapshot::Snapshot;

const W: f32 = 200.0;
const H: f32 = 100.0;

/// The pixels of one frame, as `[b, g, r, a]` rows.
struct Pixels {
    bytes: Vec<u8>,
    row: usize,
    width: usize,
    height: usize,
}

impl Pixels {
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

    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let at = y * self.row + x * 4;
        [self.bytes[at], self.bytes[at + 1], self.bytes[at + 2], self.bytes[at + 3]]
    }
}

fn render(direction: Direction, content: impl Fn() -> Element + 'static) -> Pixels {
    let mut shot = Snapshot::new(
        move || {
            window_directed(
                direction,
                rect().width(Size::fill()).height(Size::fill()).background((20, 20, 20)).child(content()),
            )
            .into_element()
        },
        (W, H),
    );
    shot.settle(Duration::from_millis(300));
    Pixels::of(&mut shot)
}

/// How many pixels of the right-to-left frame are not the reflection of the
/// left-to-right one, beyond rounding of a colour.
fn unmirrored(ltr: &Pixels, rtl: &Pixels) -> usize {
    assert_eq!((ltr.width, ltr.height), (rtl.width, rtl.height));
    let mut wrong = 0;
    for y in 0..ltr.height {
        for x in 0..ltr.width {
            let (a, b) = (ltr.at(x, y), rtl.at(ltr.width - 1 - x, y));
            if a.iter().zip(&b).any(|(a, b)| a.abs_diff(*b) > 3) {
                wrong += 1;
            }
        }
    }
    wrong
}

/// A box written as for left to right with something different on each side.
fn boxed(style: fn(Rect) -> Rect) -> impl Fn() -> Element {
    move || {
        style(rect().margin((10.0, 10.0, 10.0, 12.0)).width(Size::px(120.0)).height(Size::px(60.0)).background((90, 90, 90)))
            .into()
    }
}

fn check(what: &str, style: fn(Rect) -> Rect) {
    let ltr = render(Direction::Ltr, boxed(style));
    let rtl = render(Direction::Rtl, boxed(style));
    let wrong = unmirrored(&ltr, &rtl);
    // A rounded corner is anti-aliased, so one pixel of it may land a shade apart.
    assert!(wrong <= 4, "{what}: {wrong} pixels of the right-to-left frame are not the reflection of the left-to-right one");
}

#[test]
fn a_plain_box_is_the_reflection() {
    check("plain", |r| r);
}

#[test]
fn a_corner_radius_is_the_reflection() {
    check("radius", |r| r.corner_radius(CornerRadius::new(24.0, 0.0, 0.0, 8.0)));
}

#[test]
fn a_border_is_the_reflection() {
    check("border", |r| {
        r.border(Border::new().width(BorderWidth { top: 0.0, right: 0.0, bottom: 2.0, left: 6.0 }).fill((255, 0, 0)).alignment(BorderAlignment::Inner))
    });
}

#[test]
fn a_shadow_is_the_reflection() {
    check("shadow", |r| r.shadow(Shadow::new().x(8.0).y(0.0).blur(4.0).color((0, 0, 255, 200))));
}

#[test]
fn a_gradient_is_the_reflection() {
    check("gradient", |r| {
        r.background(
            LinearGradient::new()
                .angle(270.0)
                .stop(GradientStop::new((0, 255, 0), 0.0))
                .stop(GradientStop::new((255, 255, 255), 100.0)),
        )
    });
}

#[test]
fn a_row_runs_from_the_right_and_its_frame_is_the_reflection() {
    let row = || {
        rect()
            .horizontal()
            .padding((4.0, 4.0, 4.0, 20.0))
            .spacing(6.0)
            .child(rect().width(Size::px(30.0)).height(Size::px(30.0)).background((255, 0, 0)))
            .child(rect().width(Size::px(50.0)).height(Size::px(30.0)).background((0, 255, 0)))
            .child(rect().width(Size::px(10.0)).height(Size::px(30.0)).background((0, 0, 255)))
            .into()
    };
    let ltr = render(Direction::Ltr, row);
    let rtl = render(Direction::Rtl, row);
    assert_eq!(unmirrored(&ltr, &rtl), 0);
    // Red is the first child: at the left in one frame, at the right in the other.
    assert!(ltr.at(30, 20)[2] > 200 && rtl.at(W as usize - 31, 20)[2] > 200);
}

/// A row that lays its children out again after its own element was replaced.
/// The engine's mirroring flag is the inherited direction; the tree dropped it
/// when a node's element was replaced without its text style changing (a new
/// colour is enough), and the row then measured from the left again.
#[derive(PartialEq)]
struct Restyled;

impl Component for Restyled {
    fn render(&self) -> impl IntoElement {
        let mut pressed = use_state(|| false);
        rect()
            .component("Row")
            .horizontal()
            .width(Size::fill())
            .background(if pressed() { (60, 60, 60) } else { (40, 40, 40) })
            .child(rect().component("First").width(Size::px(30.0)).height(Size::px(30.0)))
            .child(
                rect()
                    .component("Second")
                    .width(Size::px(if pressed() { 70.0 } else { 50.0 }))
                    .height(Size::px(30.0))
                    .on_pointer_down(move |_| pressed.set(true)),
            )
    }
}

fn areas(shot: &mut Snapshot, name: &'static str) -> Area {
    shot.runner()
        .find(|node, element| (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area))
        .unwrap_or_else(|| panic!("no {name} on screen"))
}

#[test]
fn a_node_whose_element_is_replaced_still_mirrors_its_children() {
    let mut shot = Snapshot::new(|| window_directed(Direction::Rtl, Restyled).into_element(), (W, H));
    shot.settle(Duration::from_millis(300));
    assert_eq!(areas(&mut shot, "First").max_x(), W, "the first child starts at the right edge");
    // The row's element is replaced (a new background) and its second child is
    // wider, so the row measures its children again.
    assert!(shot.click_component("Second"));
    shot.settle(Duration::from_millis(300));
    assert_eq!(areas(&mut shot, "First").max_x(), W, "the row still starts from the right: {:?}", areas(&mut shot, "First"));
    assert_eq!(areas(&mut shot, "Second").max_x(), W - 30.0, "and the second child follows on its left");
}

/// A row that reads the other way once it is pressed.
#[derive(PartialEq)]
struct Flips;

impl Component for Flips {
    fn render(&self) -> impl IntoElement {
        let mut flipped = use_state(|| false);
        let direction = if flipped() { Direction::Rtl } else { Direction::Ltr };
        directed(
            direction,
            rect()
                .component("Row")
                .horizontal()
                .width(Size::fill())
                .child(rect().component("First").width(Size::px(30.0)).height(Size::px(30.0)).on_pointer_down(move |_| flipped.set(true)))
                .child(rect().component("Second").width(Size::px(50.0)).height(Size::px(30.0))),
        )
    }
}

#[test]
fn a_subtree_that_changes_direction_is_laid_out_again() {
    let mut shot = Snapshot::new(|| window_directed(Direction::Ltr, Flips).into_element(), (W, H));
    shot.settle(Duration::from_millis(300));
    assert_eq!((areas(&mut shot, "First").min_x(), areas(&mut shot, "Second").min_x()), (0.0, 30.0));
    assert!(shot.click_component("First"));
    shot.settle(Duration::from_millis(300));
    assert_eq!(areas(&mut shot, "First").max_x(), W, "the first child is at the right edge after the flip");
    assert_eq!(areas(&mut shot, "Second").max_x(), W - 30.0, "and the second follows on its left");
}

#[test]
fn a_default_label_is_aligned_like_an_explicit_left_in_left_to_right() {
    let plain = || label().text("A line of text").font_size(14.0).into();
    let left = || label().text("A line of text").font_size(14.0).text_align(TextAlign::Left).into();
    let boxed = |label: fn() -> Element| move || rect().width(Size::px(180.0)).child(label()).into();
    let (a, b) = (render(Direction::Ltr, boxed(plain)), render(Direction::Ltr, boxed(left)));
    assert_eq!(unmirrored_same(&a, &b), 0, "start is left in left to right: the same pixels as an explicit left");
}

/// Pixels that differ between two frames of one direction.
fn unmirrored_same(a: &Pixels, b: &Pixels) -> usize {
    let mut wrong = 0;
    for y in 0..a.height {
        for x in 0..a.width {
            if a.at(x, y) != b.at(x, y) {
                wrong += 1;
            }
        }
    }
    wrong
}

/// One frame of a scroll view with content taller than it, wheeled once so that its overlay
/// scrollbar shows.
fn scrolled(direction: Direction) -> Pixels {
    let mut shot = Snapshot::new(
        move || {
            window_directed(
                direction,
                rect().width(Size::fill()).height(Size::fill()).background((20, 20, 20)).child(
                    ScrollView::new()
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(rect().width(Size::fill()).height(Size::px(600.0)).background((60, 90, 140))),
                ),
            )
            .into_element()
        },
        (W, H),
    );
    shot.settle(Duration::from_millis(300));
    shot.scroll(W / 2.0, H / 2.0, 40.0);
    shot.settle(Duration::from_millis(300));
    Pixels::of(&mut shot)
}

/// The overlay scrollbar of a scroll view is placed by an absolute box with an offset of its own;
/// right to left it must come out at the other edge of the window, not outside it.
#[test]
fn a_scrollbar_is_the_reflection() {
    let (ltr, rtl) = (scrolled(Direction::Ltr), scrolled(Direction::Rtl));
    // The bar shows at the end edge in the frame that reads from the left, and at the other edge in
    // the one that reads from the right: pixels of the outer 20 columns that are not the content's.
    let ink = |frame: &Pixels, columns: std::ops::Range<usize>| {
        (0..frame.height)
            .flat_map(|y| columns.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| frame.at(x, y) != frame.at(frame.width / 2, y))
            .count()
    };
    assert!(ink(&ltr, ltr.width - 20..ltr.width) > 0, "the scrollbar shows at the right edge, left to right");
    assert!(ink(&rtl, 0..20) > 0, "the scrollbar shows at the left edge, right to left");
    let wrong = unmirrored(&ltr, &rtl);
    assert!(wrong <= 40, "{wrong} pixels of the right-to-left frame are not the reflection of the left-to-right one");
}

/// The overlay scrollbar tracks of the frame: the 20 px wide, tall boxes the scroll bar draws in.
fn tracks(shot: &mut Snapshot) -> Vec<Area> {
    shot.runner().find_many(|node, _| {
        let area = node.layout().area;
        (area.width() == 20.0 && area.height() >= 60.0).then_some(area)
    })
}

/// A page whose content gets taller when its strip at the top is pressed, as a page does when a
/// row opens or a list loads: the scroll bar's thumb changes length while it is on screen.
#[derive(PartialEq)]
struct Growing;

impl Component for Growing {
    fn render(&self) -> impl IntoElement {
        let mut tall = use_state(|| false);
        rect()
            .width(Size::fill())
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(50.0))
                    .background((90, 90, 90))
                    .on_pointer_down(move |_| tall.set(true)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(if *tall.read() { 1800.0 } else { 1600.0 }))
                    .background((60, 90, 140)),
            )
    }
}

/// A page the way a phone window holds it: a header, then a scroll view in an absolute box that
/// fills the area under it (`NavigationSplitView`'s compact screens). Returns the scroll bar's
/// tracks after the content has grown.
fn phone_page(direction: Direction, scrollbar_inset_top: f32) -> Vec<Area> {
    let mut shot = Snapshot::new(
        move || {
            window_directed(
                direction,
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .background((20, 20, 20))
                    .child(rect().width(Size::fill()).height(Size::px(56.0)))
                    .child(
                        rect().width(Size::fill()).height(Size::flex(1.0)).child(
                            rect()
                                .position(Position::new_absolute().top(0.0).left(0.0))
                                .width(Size::fill())
                                .height(Size::fill())
                                .child(
                                    ScrollView::new()
                                        .scrollbar_inset_top(scrollbar_inset_top)
                                        .scrollbar_inset_bottom(15.0)
                                        .child(Growing),
                                ),
                        ),
                    ),
            )
            .into_element()
        },
        (390.0, 800.0),
    );
    shot.settle(Duration::from_millis(300));
    shot.click(195.0, 56.0 + 25.0);
    shot.settle(Duration::from_millis(300));
    tracks(&mut shot)
}

/// A phone window that reads from the right keeps the overlay scrollbar of a page inside the
/// window, on its left edge, also after the page's content changed size. The bar is an absolute box
/// with an offset of its own: the offset must be mirrored once (the bar came out 40 px outside the
/// window).
#[test]
fn a_page_scrollbar_in_a_phone_window_is_on_the_left_edge_inside_the_window() {
    for inset in [0.0, 56.0] {
        let ltr = phone_page(Direction::Ltr, inset);
        let rtl = phone_page(Direction::Rtl, inset);
        // The outer box is the track, the inner ones the bar and its thumb: the thumb is what shows.
        let thumb = |areas: &[Area]| areas.iter().map(|area| area.min_x()).fold(f32::NAN, f32::min);
        assert_eq!(thumb(&ltr), 370.0, "left to right the bar is at the right edge: {ltr:?}");
        assert_eq!(thumb(&rtl), 0.0, "right to left the bar is at the left edge, inside the window: {rtl:?}");
        assert!(rtl.iter().all(|area| area.min_x() >= -20.0), "no part of the bar is more than its own width outside: {rtl:?}");
    }
}
