//! A scrolled page keeps its offset through a push and Back, whatever the wheel
//! does while the slides run.
//!
//! Frame by frame like `page_motion_tests`: the clock runs slow and every step
//! moves the animation by the same fixed 16 ms. A bar at the top of each page's
//! content is its scroll position: a page scrolled by `d` px has its bar `d` px
//! above where it rests.
//!
//! The rules these hold:
//! - the page a subpage was opened from is where it was when Back lands;
//! - a wheel notch during the push does not reach that page, and one during the
//!   Back slide does not reach the page that is leaving;
//! - the subpage starts at its own top.

use std::sync::Arc;
use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use super::navigation::NavigationSplitView;
use super::overlays::OverlayHost;
use crate::component::ComponentName;
use crate::group::Group;
use crate::icons::Icon;
use crate::layout::page_column;
use crate::nav::{Location, RouteTable};
use crate::snapshot::Snapshot;
use crate::stage::staged;
use crate::style::*;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (1100.0, 760.0);
const SPEED: f32 = 0.4853;
/// A slide is about 250 ms; this covers it with room.
const SLIDE_STEPS: usize = 24;
/// Where the wheel is sent: over the page, clear of the sidebar.
const WHEEL_AT: (f32, f32) = (700.0, 400.0);

/// A page as long as a real one, mounted a group a frame as real pages are,
/// with a bar at the top of its content and, on the first, a row that opens the
/// subpage a few rows down.
#[derive(PartialEq)]
struct Tall {
    class: &'static str,
    opens: bool,
}

impl Component for Tall {
    fn render(&self) -> impl IntoElement {
        let router = crate::nav::use_router();
        let opens = self.opens;
        let mut column = page_column().child(
            rect()
                .component(self.class)
                .width(Size::fill())
                // ui-lint: allow(an 8 px marker the test looks up by class name, not styling)
                .height(Size::px(8.0)),
        );
        for group in 0..12 {
            let mut card = Group::titled(&format!("Group {group}"));
            for index in 0..5 {
                card =
                    if group == 1 && index == 0 && opens {
                        card.row(crate::rows::row("Open the subpage").navigates().on_press(
                            move |_| {
                                router.push(&Location::new("scroll").join("network").join("saved"));
                            },
                        ))
                    } else {
                        card.row(crate::rows::row(format!("Row {group}.{index}")))
                    };
            }
            column = column.child(card);
        }
        staged(column)
    }
}

fn table() -> Arc<RouteTable> {
    Arc::new(
        RouteTable::new("scroll")
            .index("Scroll", |_| rect().into())
            .root("network")
            .title("Network")
            .icon(Icon::Network)
            .group("Connections")
            .page(|_| {
                Tall {
                    class: "BarRoot",
                    opens: true,
                }
                .into()
            })
            .route("network/saved")
            .title("Saved networks")
            .page(|_| {
                Tall {
                    class: "BarSub",
                    opens: false,
                }
                .into()
            }),
    )
}

#[derive(PartialEq)]
struct Window;

impl Component for Window {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        crate::nav::provide_router(table(), &Location::new("scroll").join("network"));
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(crate::theme::font_family())
            .child(OverlayHost::new(NavigationSplitView::new("Scroll")))
    }
}

fn step(shot: &mut Snapshot) {
    shot.runner().poll_n(Duration::from_millis(40), 1);
}

fn steps(shot: &mut Snapshot, count: usize) {
    for _ in 0..count {
        step(shot);
    }
}

fn bars(shot: &mut Snapshot, class: &'static str) -> Vec<Area> {
    shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some(class)).then(|| node.layout().area)
    })
}

/// Where the bar of `class` is, down the window; `None` when it is not on screen.
fn bar_y(shot: &mut Snapshot, class: &'static str) -> Option<f32> {
    bars(shot, class).first().map(|area| area.min_y())
}

/// Whether a slide is running: the page that leaves and the page that arrives are both mounted.
fn sliding(shot: &mut Snapshot) -> bool {
    bar_y(shot, "BarRoot").is_some() && bar_y(shot, "BarSub").is_some()
}

fn wheel(shot: &mut Snapshot, dy: f32) {
    shot.scroll(WHEEL_AT.0, WHEEL_AT.1, dy);
}

fn press_open(shot: &mut Snapshot) {
    let at = shot
        .runner()
        .find_many(|node, element| {
            let shown: String = Label::try_downcast(element)
                .map(|label| label.text.to_string())
                .or_else(|| {
                    Paragraph::try_downcast(element).map(|paragraph| {
                        paragraph.spans.iter().map(|s| s.text.to_string()).collect()
                    })
                })?;
            (shown == "Open the subpage").then(|| {
                let area = node.layout().area;
                (
                    area.min_x() + area.width() / 2.0,
                    area.min_y() + area.height() / 2.0,
                )
            })
        })
        .into_iter()
        .next()
        .expect("the row that opens the subpage is in view");
    shot.click(at.0, at.1);
}

/// What a walk saw.
struct Seen {
    /// Where the origin page's bar rests, unscrolled.
    rest: f32,
    /// How far the origin page was scrolled before the push.
    before: f32,
    /// How far it is scrolled once Back has landed.
    after: f32,
    /// How far the subpage was scrolled when its slide was over, by the wheel
    /// sent during the push.
    sub_after_push: f32,
    /// The origin page's offset in every frame of the push, from the first.
    during_push: Vec<f32>,
}

/// Scroll the origin page by `scroll` px, open the subpage, go Back with Alt+Left,
/// and send a wheel notch of `notch` px at every step of the slides asked for.
fn walk(scroll: f32, push_wheel: Option<f32>, back_wheel: Option<f32>) -> Seen {
    let mut shot = Snapshot::new(|| Window.into(), SIZE);
    shot.settle(Duration::from_millis(1500));
    let rest = bar_y(&mut shot, "BarRoot").expect("the origin page is on screen");
    shot.runner().animation_clock().set_speed(SPEED);
    wheel(&mut shot, -scroll);
    steps(&mut shot, 40);
    let before = rest - bar_y(&mut shot, "BarRoot").expect("the origin page is on screen");
    assert!(before > 100.0, "the walk scrolled the page: {before} px");

    press_open(&mut shot);
    let mut during_push = Vec::new();
    for _ in 0..SLIDE_STEPS {
        // A notch while the slide runs: both pages are mounted. One that lands after
        // it is over is the person scrolling the page that arrived.
        if let (Some(notch), true) = (push_wheel, sliding(&mut shot)) {
            wheel(&mut shot, notch);
        }
        step(&mut shot);
        if let Some(y) = bar_y(&mut shot, "BarRoot") {
            during_push.push(rest - y);
        }
    }
    steps(&mut shot, 40);
    assert!(bar_y(&mut shot, "BarSub").is_some(), "the subpage is open");
    let sub_rest = rest;
    let sub_after_push = sub_rest - bar_y(&mut shot, "BarSub").unwrap();

    shot.key_with(Key::Named(NamedKey::ArrowLeft), Modifiers::ALT);
    let mut notches = 0;
    for _ in 0..SLIDE_STEPS {
        if let (Some(notch), true) = (back_wheel, sliding(&mut shot)) {
            wheel(&mut shot, notch);
            notches += 1;
        }
        step(&mut shot);
    }
    assert!(
        back_wheel.is_none() || notches > 3,
        "the walk sent notches during the Back slide: {notches}"
    );
    steps(&mut shot, 60);
    let after = rest - bar_y(&mut shot, "BarRoot").expect("Back returned to the origin page");
    Seen {
        rest,
        before,
        after,
        sub_after_push,
        during_push,
    }
}

#[test]
fn back_returns_the_page_to_the_offset_it_was_pushed_from() {
    let seen = walk(220.0, None, None);
    assert!(
        (seen.after - seen.before).abs() < 1.0,
        "the page was scrolled {:.0} px before the push and {:.0} px after Back",
        seen.before,
        seen.after
    );
}

#[test]
fn a_wheel_notch_at_every_step_of_the_push_does_not_move_the_page_below() {
    let seen = walk(220.0, Some(-40.0), None);
    for (frame, offset) in seen.during_push.iter().enumerate() {
        assert!(
            (offset - seen.before).abs() < 1.0,
            "step {frame} of the push: the page below is scrolled {offset:.0} px, it was {:.0} px",
            seen.before
        );
    }
    assert!(
        (seen.after - seen.before).abs() < 1.0,
        "Back returned the page at {:.0} px, it was pushed from {:.0} px",
        seen.after,
        seen.before
    );
}

#[test]
fn a_wheel_notch_at_every_step_of_the_back_slide_does_not_move_the_page_that_leaves_or_the_one_below()
 {
    let seen = walk(220.0, None, Some(-40.0));
    assert!(
        (seen.after - seen.before).abs() < 1.0,
        "Back returned the page at {:.0} px after wheel notches during the slide, it was pushed from {:.0} px",
        seen.after,
        seen.before
    );
}

#[test]
fn the_subpage_opens_at_its_own_top_whatever_the_wheel_did_during_the_push() {
    let seen = walk(220.0, Some(-40.0), None);
    assert!(
        seen.sub_after_push.abs() < 1.0,
        "the subpage opened scrolled {:.0} px (its rest is {:.0})",
        seen.sub_after_push,
        seen.rest
    );
}
