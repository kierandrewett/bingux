//! A menu opened from inside a dialog is drawn above that dialog, to the
//! pixel. The preview card is a dialog and its zoom menu hangs from a button
//! in it; the menu layer sat below the dialog layer, so the menu opened
//! behind the card that owned its button (the zoom list was cut off by the
//! card's own edge and the rest of the page).
//!
//! The dialog here is filled with a colour nothing else uses; the menu panel
//! overlaps it. Every pixel inside the panel must be the panel's, none the
//! card's.

use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;

use super::menu::{menu_button, menu_item};
use super::overlays::{OverlayHost, use_overlays};
use super::framed_dialog;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

/// Loud red: no theme paint is this.
const CARD: (u8, u8, u8) = (255, 0, 0);
const SIZE: (f32, f32) = (900.0, 640.0);

#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(OverlayHost::new(Page))
    }
}

#[derive(PartialEq)]
struct Page;

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        use_hook(move || {
            overlays.show_dialog(framed_dialog(
                "Card",
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .background(Color::from_rgb(CARD.0, CARD.1, CARD.2))
                    .padding(Gaps::new(8.0, 8.0, 8.0, 8.0))
                    .child(
                        rect().width(Size::px(160.0)).child(menu_button(
                            "Zoom",
                            ["Fit width", "Fit page", "50%", "75%", "100%", "125%", "150%", "200%"].map(|label| menu_item(label, |_| {})).to_vec(),
                        )),
                    ),
                480.0,
                360.0,
            ));
        });
        rect().expanded()
    }
}

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

    fn rgb(&self, x: f32, y: f32) -> (u8, u8, u8) {
        let x = (x.floor().max(0.0) as usize).min(self.width - 1);
        let y = (y.floor().max(0.0) as usize).min(self.height - 1);
        let at = y * self.row + x * 4;
        // N32 on a little-endian machine is blue, green, red, alpha.
        (self.bytes[at + 2], self.bytes[at + 1], self.bytes[at])
    }
}

fn is_card(pixel: (u8, u8, u8)) -> bool {
    let near = |a: u8, b: u8| a.abs_diff(b) <= 6;
    near(pixel.0, CARD.0) && near(pixel.1, CARD.1) && near(pixel.2, CARD.2)
}

#[test]
fn a_menu_opened_inside_a_dialog_is_drawn_over_the_dialog() {
    let mut shot = Snapshot::new(|| Root.into_element(), SIZE);
    shot.settle(Duration::from_millis(600));
    assert!(shot.click_component("MenuButton"), "the menu button is in the dialog");
    shot.settle(Duration::from_millis(600));

    // The rows of the menu: where the last label sits is well inside the card.
    let rows: Vec<(String, f32, f32, f32, f32)> = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        let area = node.layout().area;
        ["Fit page", "50%", "100%", "200%"].contains(&&*label.text).then(|| (label.text.to_string(), area.min_x(), area.min_y(), area.width(), area.height()))
    });
    assert_eq!(rows.len(), 4, "the menu's rows are on screen: {rows:?}");

    let frame = Frame::of(&mut shot);
    for (text, x, y, _, height) in &rows {
        // A pixel just left of the text, at the row's middle: panel padding.
        let pixel = frame.rgb(x - 6.0, y + height / 2.0);
        assert!(!is_card(pixel), "the menu row {text:?} is under the card: the pixel beside it is the card's red {pixel:?}");
    }
    // Sanity: the card itself is red where no menu is.
    assert!(is_card(frame.rgb(SIZE.0 / 2.0 + 150.0, SIZE.1 / 2.0 + 120.0)), "the card is drawn");
}
