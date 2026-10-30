//! A header bar keeps its controls on the same insets in every window, popup
//! and dialog, and keeps its title clear of them. The close button of a
//! dialog is one 36 px (44 px on a phone) icon button: its painted edge must
//! be `S2` from the bar's right edge, as the controls of every other header
//! are. A plain title is centred on the bar whether or not there is a
//! leading control, and a title slot has the same room on both sides, so what
//! it holds never runs under a control.

use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use super::window::HeaderBar;
use crate::controls::icon_button;
use crate::icons::Icon;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::provide_tokens;
use crate::typography::{BODY, Ink};
use crate::window::WindowHost;

#[derive(Clone, Copy, PartialEq)]
struct Case {
    /// The title is a slot holding a label at each end of its width, not a
    /// plain string.
    slot: bool,
    /// A leading button, as a popup's Cancel.
    leading: bool,
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

impl Component for Host {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut bar = HeaderBar::new("Properties")
            .draggable(false)
            .trailing(icon_button(Icon::WindowClose, |_| {}).tooltip("Close"));
        if self.0.leading {
            bar = bar.leading(icon_button(Icon::GoPrevious, |_| {}).tooltip("Back"));
        }
        if self.0.slot {
            bar = bar.title_slot(
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), justify(Align::Between), w_full()])
                    .child(BODY.in_ink(Ink::Text).label(&tokens, "Left").max_lines(1))
                    .child(BODY.in_ink(Ink::Text).label(&tokens, "Right").max_lines(1)),
            );
        }
        rect().expanded().child(bar)
    }
}

struct Measured {
    bar: Area,
    close: Area,
    /// The plain title, or the slot's left label.
    first: Area,
    /// The slot's right label.
    last: Option<Area>,
    right_inset: f32,
    gap: f32,
}

fn measure(case: Case, width: f32) -> Measured {
    let mut shot = Snapshot::new(move || Root(case).into_element(), (width, 160.0));
    shot.settle(Duration::from_millis(400));
    let (right_inset, gap) = shot.runner().run_in(|| (S2.px(), S1.px()));
    let by_class = |shot: &mut Snapshot, name: &'static str| -> Vec<Area> {
        shot.runner().find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area)
        })
    };
    let by_text = |shot: &mut Snapshot, text: &'static str| -> Option<Area> {
        shot.runner()
            .find_many(|node, element| {
                let label = Label::try_downcast(element)?;
                (label.text == text).then(|| node.layout().area)
            })
            .into_iter()
            .next()
    };
    let bar = by_class(&mut shot, "HeaderBar").into_iter().next().expect("the header bar");
    // The close button is the right-most icon button.
    let close = by_class(&mut shot, "IconButton")
        .into_iter()
        .max_by(|a, b| a.max_x().total_cmp(&b.max_x()))
        .expect("the close button");
    let first = by_text(&mut shot, if case.slot { "Left" } else { "Properties" }).expect("the title");
    let last = case.slot.then(|| by_text(&mut shot, "Right").expect("the right label"));
    Measured {
        bar,
        close,
        first,
        last,
        right_inset,
        gap,
    }
}

const CASES: [Case; 4] = [
    Case { slot: false, leading: false },
    Case { slot: false, leading: true },
    Case { slot: true, leading: false },
    Case { slot: true, leading: true },
];

#[test]
fn the_close_button_keeps_the_right_inset_of_every_header() {
    for width in [520.0, 390.0] {
        for case in CASES {
            let m = measure(case, width);
            let label = format!("width {width}, slot {}, leading {}", case.slot, case.leading);
            assert!(
                (m.bar.max_x() - m.close.max_x() - m.right_inset).abs() < 0.5,
                "{label}: the close button ends {} from the bar's right edge, not {}",
                m.bar.max_x() - m.close.max_x(),
                m.right_inset
            );
        }
    }
}

#[test]
fn a_plain_title_is_centred_on_the_bar() {
    for width in [520.0, 390.0] {
        for leading in [false, true] {
            let m = measure(Case { slot: false, leading }, width);
            let bar_centre = (m.bar.min_x() + m.bar.max_x()) / 2.0;
            let title_centre = (m.first.min_x() + m.first.max_x()) / 2.0;
            assert!(
                (bar_centre - title_centre).abs() < 0.5,
                "width {width}, leading {leading}: the title is centred at {title_centre}, the bar at {bar_centre}"
            );
        }
    }
}

#[test]
fn a_title_slot_has_the_same_room_on_both_sides_and_clears_the_controls() {
    for width in [520.0, 390.0] {
        for leading in [false, true] {
            let m = measure(Case { slot: true, leading }, width);
            let right = m.last.expect("the right label");
            let case = format!("width {width}, leading {leading}");
            assert!(
                right.max_x() <= m.close.min_x() - m.gap + 0.5,
                "{case}: the slot ends at {} under the close button from {}",
                right.max_x(),
                m.close.min_x()
            );
            let before = m.first.min_x() - m.bar.min_x();
            let after = m.bar.max_x() - right.max_x();
            assert!(
                (before - after).abs() < 0.5,
                "{case}: the slot has {before} px before it and {after} px after it"
            );
        }
    }
}
