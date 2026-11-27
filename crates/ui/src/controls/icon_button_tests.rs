//! Icon buttons pressed with a real pointer in the headless app.
//!
//! Failures these tests guard against:
//!   1. a button that shows work in progress swallows the press that would
//!      show the person the work (a downloads button that does nothing while
//!      something downloads);
//!   2. `working` swallows presses like `busy` does, or `busy` stops
//!      swallowing them, so an action that is running is asked twice.

use std::sync::atomic::{AtomicU32, Ordering};

use freya::prelude::*;

use crate::controls::icon_button;
use crate::icons::Icon;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

static PLAIN: AtomicU32 = AtomicU32::new(0);
static WORKING: AtomicU32 = AtomicU32::new(0);
static BUSY: AtomicU32 = AtomicU32::new(0);

const SIDE: f32 = 48.0;

/// Three buttons in a row, each counting its presses.
#[derive(PartialEq, Clone)]
struct Row;

impl Component for Row {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        rect()
            .expanded()
            .horizontal()
            .child(icon_button(Icon::ArrowCw, |_: ()| {
                PLAIN.fetch_add(1, Ordering::SeqCst);
            }))
            .child(
                icon_button(Icon::ArrowCw, |_: ()| {
                    WORKING.fetch_add(1, Ordering::SeqCst);
                })
                .working(true),
            )
            .child(
                icon_button(Icon::ArrowCw, |_: ()| {
                    BUSY.fetch_add(1, Ordering::SeqCst);
                })
                .busy(true),
            )
    }
}

fn press(shot: &mut Snapshot, index: f32) {
    // Each button is a control wide; the row starts at the window's corner.
    let side = Dim::Control.px_in(SizeClass::Regular);
    shot.click(index * side + side / 2.0, side / 2.0);
}

#[test]
fn a_working_button_still_runs_its_press_and_a_busy_one_does_not() {
    let mut shot = Snapshot::new(|| Row.into_element(), (SIDE * 4.0, SIDE * 2.0));
    for index in [0.0, 1.0, 2.0] {
        press(&mut shot, index);
    }
    assert_eq!(PLAIN.load(Ordering::SeqCst), 1, "a plain button runs");
    assert_eq!(WORKING.load(Ordering::SeqCst), 1, "a working one runs");
    assert_eq!(
        BUSY.load(Ordering::SeqCst),
        0,
        "a busy one is asked once, not twice"
    );
}
