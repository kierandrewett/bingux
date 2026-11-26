//! The file chooser's preview-pane toggle is on the side the pane opens on: the
//! docked preview is a column at the end of the file area, so its button is the
//! last control of the dialog's toolbar and draws a window with its panel at the
//! end edge (`shell::pane_toggle`).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use bingux_files::ChooserRequest;
use freya::prelude::*;

use super::{ChooserBody, Done, Host};
use crate::shell::OverlayHost;
use crate::shell::pane_toggle_tests::{InkSide, ink_side};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (1000.0, 700.0);
const PREVIEW: &str = "Preview pane (Alt+P)";

#[derive(PartialEq)]
struct Dialog;

impl Component for Dialog {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .child(OverlayHost::new(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_col()])
                    .child(ChooserBody {
                        request: ChooserRequest::open(),
                        host: Host::Dialog(Done(Rc::new(RefCell::new(None)))),
                    }),
            ))
    }
}

#[test]
fn the_preview_pane_toggle_is_at_the_end_of_the_toolbar_and_draws_a_pane_at_the_end() {
    let mut shot = Snapshot::new(|| Dialog.into_element(), SIZE);
    shot.settle(Duration::from_millis(600));
    let buttons = shot.icon_buttons();
    let pane = buttons
        .iter()
        .find(|button| button.name == PREVIEW)
        .expect("the preview pane button")
        .area;
    // Every other icon button of the toolbar's row is before it.
    for other in buttons.iter().filter(|button| button.name != PREVIEW) {
        if (other.area.min_y() - pane.min_y()).abs() < pane.height() / 2.0 {
            assert!(
                other.area.max_x() <= pane.min_x() + 0.5,
                "{} ends at {}, after the preview pane button at {}: the toggle of a pane at the end is the last control of the row",
                other.name,
                other.area.max_x(),
                pane.min_x()
            );
        }
    }
    assert_eq!(
        ink_side(&mut shot, pane, SIZE.0),
        InkSide::Right,
        "the preview pane button draws its panel at the end edge, not the start"
    );
}
