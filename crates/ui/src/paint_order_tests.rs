//! Nodes on one paint layer are drawn, and hit, in tree order.
//!
//! Freya gives a node its parent's layer plus one (plus its `Layer::Relative`), so siblings with
//! no layer of their own share one, and so do cousins at the same depth. The vendored freya-core
//! kept the nodes of a layer in a hash set and drew them in the set's iteration order, so which of
//! two overlapping nodes showed depended on their node ids: the browser's load bar track
//! vanished under the page on a phone-sized window. A later node paints over an earlier one, as
//! in a document, and takes the pointer first.

use std::cell::Cell;
use std::rc::Rc;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;

use crate::snapshot::Snapshot;

/// Enough nodes on one layer that a hash order matching the tree order is not luck.
const COUNT: u8 = 24;
const SIDE: f32 = 64.0;

/// A colour of its own for each node.
fn colour(index: u8) -> Color {
    // ui-lint: allow(probe colours: each node needs one of its own, read back from the frame)
    Color::from_rgb(index * 10, 255 - index * 10, 128)
}

/// A square at the window's corner, over whatever an earlier sibling drew there.
fn square(index: u8) -> Rect {
    rect()
        .position(Position::new_absolute().top(0.0).left(0.0))
        .width(Size::px(SIDE))
        .height(Size::px(SIDE))
        // ui-lint: allow(a probe square of an exact colour, not interface)
        .background(colour(index))
}

/// The pixel at the middle of the squares, as red, green and blue.
fn middle(shot: &mut Snapshot) -> [u8; 3] {
    let png = shot.runner().render();
    let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
    let mut surface =
        raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
    surface.canvas().draw_image(&image, (0, 0), None);
    let info = surface.canvas().image_info();
    let row = info.min_row_bytes();
    let mut bytes = vec![0u8; info.compute_byte_size(row)];
    assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
    let scale = image.width() as f32 / (SIDE * 2.0);
    let at = (SIDE / 2.0 * scale) as usize * row + (SIDE / 2.0 * scale) as usize * 4;
    // N32 on a little-endian machine is blue, green, red, alpha.
    [bytes[at + 2], bytes[at + 1], bytes[at]]
}

fn rgb(colour: Color) -> [u8; 3] {
    [colour.r(), colour.g(), colour.b()]
}

fn open(app: impl Fn() -> Element + 'static) -> Snapshot {
    let mut shot = Snapshot::new(app, (SIDE * 2.0, SIDE * 2.0));
    shot.runner().sync_and_update();
    shot
}

#[test]
fn the_last_of_overlapping_siblings_paints_on_top() {
    let mut shot = open(|| {
        rect()
            .expanded()
            .children((0..COUNT).map(|index| square(index).into_element()))
            .into_element()
    });
    assert_eq!(middle(&mut shot), rgb(colour(COUNT - 1)));
}

#[test]
fn a_later_cousin_paints_over_an_earlier_one_on_the_same_layer() {
    // Each square is the only child of its own wrapper, so the squares are cousins: one depth,
    // one layer, different parents.
    let mut shot = open(|| {
        rect()
            .expanded()
            .children((0..COUNT).map(|index| {
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .child(square(index))
                    .into_element()
            }))
            .into_element()
    });
    assert_eq!(middle(&mut shot), rgb(colour(COUNT - 1)));
}

#[test]
fn the_last_of_overlapping_siblings_takes_the_press() {
    let pressed = Rc::new(Cell::new(None));
    let heard = pressed.clone();
    let mut shot = open(move || {
        let heard = heard.clone();
        rect()
            .expanded()
            .children((0..COUNT).map(move |index| {
                let heard = heard.clone();
                square(index)
                    .on_pointer_down(move |_: Event<PointerEventData>| heard.set(Some(index)))
                    .into_element()
            }))
            .into_element()
    });
    shot.click(SIDE / 2.0, SIDE / 2.0);
    assert_eq!(pressed.get(), Some(COUNT - 1));
}
