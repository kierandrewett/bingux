//! A subpage opening and closing, rendered frame by frame.
//!
//! The animation clock runs slow and each step waits longer than the
//! animation's 34 ms tick cap, so every step moves the animation by the same
//! fixed 16 ms whatever the machine's load, and the table of where the page is
//! at each step is the same on every run. A bar the page carries is its
//! visible position: the bar's centre, in pixels from where the page rests.
//!
//! An open and a close are compared step by step. A close is the open played
//! backwards: the page is where the opening page was the same time before the
//! end, and both finish on the same step. The curve is symmetric, so it is
//! also the open mirrored: a page that closes has covered as much of the way
//! as an opening page had left at the same time.

use std::sync::Arc;
use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use super::navigation::NavigationSplitView;
use super::overlays::OverlayHost;
use crate::component::ComponentName;
use crate::icons::Icon;
use crate::nav::gesture::SLIDE_MS;
use crate::nav::{Location, RouteTable};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const DESKTOP: (f32, f32) = (1100.0, 760.0);
const PHONE: (f32, f32) = (390.0, 800.0);
/// What the animation moves each step, in milliseconds: it counts whole
/// milliseconds, so this is the 34 ms tick cap at the clock's speed, 16.5 ms,
/// cut to 16. A slide is a whole number of steps.
const STEP_MS: u64 = 16;
const SPEED: f32 = 0.4853;
const STEPS: usize = (SLIDE_MS / STEP_MS) as usize;

/// A page with a bar the test can find: a row under it opens the subpage.
#[derive(PartialEq)]
struct Marked {
    class: &'static str,
    opens: bool,
}

impl Component for Marked {
    fn render(&self) -> impl IntoElement {
        let router = crate::nav::use_router();
        let opens = self.opens;
        rect()
            .component(self.class)
            .width(Size::fill())
            .child(
                crate::rows::row("Open the subpage")
                    .navigates()
                    .on_press(move |_| {
                        if opens {
                            router.push(&Location::new("motion").join("network").join("saved"));
                        }
                    }),
            )
    }
}

fn table() -> Arc<RouteTable> {
    Arc::new(
        RouteTable::new("motion")
            .index("Motion", |_| rect().into())
            .root("network")
            .title("Network")
            .icon(Icon::Network)
            .group("Connections")
            .page(|_| Marked { class: "MarkRoot", opens: true }.into())
            .root("sound")
            .title("Sound")
            .icon(Icon::Sound)
            .group("Hardware")
            .page(|_| Marked { class: "MarkOther", opens: false }.into())
            .route("network/saved")
            .title("Saved networks")
            .page(|_| Marked { class: "MarkSub", opens: false }.into()),
    )
}

#[derive(PartialEq)]
struct Window {
    at: &'static str,
}

impl Component for Window {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut start = Location::new("motion");
        for segment in self.at.split('/') {
            start = start.join(segment);
        }
        crate::nav::provide_router(table(), &start);
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(crate::theme::font_family())
            .child(OverlayHost::new(NavigationSplitView::new("Motion")))
    }
}

fn open(at: &'static str, size: (f32, f32)) -> Snapshot {
    let mut shot = Snapshot::new(move || Window { at }.into_element(), size);
    shot.settle(Duration::from_millis(900));
    shot.runner().animation_clock().set_speed(SPEED);
    shot
}

fn marker(shot: &mut Snapshot, class: &'static str) -> Option<Area> {
    shot.runner()
        .find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(class)).then(|| node.layout().area)
        })
        .into_iter()
        .next()
}

fn centre(area: Area) -> f32 {
    area.min_x() + area.width() / 2.0
}

/// One step of the animation clock: wait past the tick cap, so the animation
/// moves by exactly the cap at the clock's speed.
fn step(shot: &mut Snapshot) {
    shot.runner().poll_n(Duration::from_millis(40), 1);
}

/// Click the centre of the text `text` (a label, or a paragraph on a phone).
fn press(shot: &mut Snapshot, text: &str) {
    let at = shot
        .runner()
        .find_many(|node, element| {
            let shown: String = Label::try_downcast(element).map(|label| label.text.to_string()).or_else(|| {
                Paragraph::try_downcast(element)
                    .map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect())
            })?;
            (shown == text).then(|| {
                let area = node.layout().area;
                (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0)
            })
        })
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("{text} is on screen"));
    shot.click(at.0, at.1);
}

/// What a close or an open does to the sliding page.
struct Run {
    /// The page's centre, in pixels from where it rests, at each step from
    /// the step the animation began on (time 0) while the page was on screen.
    at: Vec<f32>,
    /// The step the move was over on: an open's page is in place, a close's
    /// page has gone.
    done: usize,
    /// The window's width.
    width: f32,
}

impl Run {
    /// The page's place at step `k` as a fraction of the window's width.
    fn share(&self, k: usize) -> f32 {
        self.at[k] / self.width
    }

    fn print(&self, label: &str) {
        println!("# {label}: over on step {} ({} ms)", self.done, self.done as u64 * STEP_MS);
        for (k, x) in self.at.iter().enumerate() {
            println!("{:5} ms  {x:8.1} px  {:6.3}", k as u64 * STEP_MS, x / self.width);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Way {
    Row,
    BackButton,
    AltLeft,
    Sidebar,
    Escape,
    /// A touch dragged in from the left edge to `x` and let go.
    Swipe(f32),
    /// A touch dragged in quickly to `x`, still moving as it lifts.
    Fling(f32),
}

/// Do `way` on the window at `at` and follow the bar `class` until the move
/// is over. The page's rest is where the bar is before the move (a close) or
/// where it ends (an open). The steps before the first movement are dropped,
/// but for one: the animation's first tick counts for nothing, so that step is
/// its time 0 and the first movement is one step in.
fn run(at: &'static str, size: (f32, f32), way: Way, class: &'static str) -> Run {
    let mut shot = open(at, size);
    let before = marker(&mut shot, class).map(centre);
    match way {
        Way::Row => press(&mut shot, "Open the subpage"),
        Way::BackButton => {
            let back = shot
                .icon_buttons()
                .into_iter()
                .find(|button| button.name == "Back")
                .expect("a Back button is on screen");
            shot.click(back.area.min_x() + back.area.width() / 2.0, back.area.min_y() + back.area.height() / 2.0);
        }
        Way::AltLeft => {
            shot.key_with(Key::Named(NamedKey::ArrowLeft), Modifiers::ALT);
        }
        Way::Escape => {
            shot.key(Key::Named(NamedKey::Escape));
        }
        Way::Sidebar => press(&mut shot, "Network"),
        Way::Swipe(to) => {
            // Slowly, with the finger at rest before it lifts: no fling.
            shot.press((4.0, 400.0));
            for x in [30.0, to * 0.5, to] {
                shot.move_to((x, 400.0));
                shot.settle(Duration::from_millis(40));
            }
            shot.settle(Duration::from_millis(160));
            shot.release((to, 400.0));
        }
        Way::Fling(to) => {
            shot.press((4.0, 400.0));
            for x in [to / 3.0, to * 2.0 / 3.0, to] {
                shot.move_to((x, 400.0));
                shot.settle(Duration::from_millis(15));
            }
            shot.release((to, 400.0));
        }
    }
    let mut seen: Vec<f32> = Vec::new();
    let mut gone = None;
    let mut still = 0;
    let mut moved = false;
    for k in 0..90 {
        step(&mut shot);
        // BINGUX_MOTION_FRAMES=DIR (made beforehand) keeps a PNG of every step.
        if let Ok(dir) = std::env::var("BINGUX_MOTION_FRAMES") {
            shot.save(format!("{dir}/{}-{way:?}-{k:02}.png", size.0 as u32));
        }
        let Some(area) = marker(&mut shot, class) else {
            if !seen.is_empty() {
                gone = Some(k);
                break;
            }
            continue;
        };
        let x = centre(area);
        if let Some(&previous) = seen.last() {
            if (x - previous).abs() > 0.01 {
                moved = true;
                still = 0;
            } else {
                still += 1;
            }
        }
        seen.push(x);
        if (moved && still >= 3) || (!moved && seen.len() > 14) {
            break;
        }
    }
    let rest = before.unwrap_or_else(|| seen.last().copied().unwrap_or(0.0));
    let first = seen
        .windows(2)
        .position(|pair| (pair[1] - pair[0]).abs() > 0.01)
        .unwrap_or(0);
    let at: Vec<f32> = seen[first..].iter().map(|x| x - rest).collect();
    // An open is over on the first step its page is in place; a close when the
    // page is no longer there.
    let done = match gone {
        Some(_) => at.len(),
        None => at.iter().position(|x| x.abs() < 0.01).unwrap_or(at.len()),
    };
    Run { at, done, width: size.0 }
}

#[test]
fn a_subpage_close_is_its_open_played_backwards_on_a_desktop_window() {
    let opening = run("network", DESKTOP, Way::Row, "MarkSub");
    opening.print("desktop open");
    assert_eq!(opening.done, STEPS, "an open takes {SLIDE_MS} ms");
    for way in [Way::BackButton, Way::AltLeft, Way::Sidebar] {
        let closing = run("network/saved", DESKTOP, way, "MarkSub");
        closing.print(&format!("desktop close ({way:?})"));
        assert_eq!(closing.done, STEPS, "{way:?}: a close takes as long as an open");
        // The page is where the opening page was, the same time before the end.
        for k in 0..STEPS {
            let (back, forth) = (closing.share(k), opening.share(STEPS - k));
            assert!(
                (back - forth).abs() < 0.002,
                "{way:?}, {} ms: closing {back:.4}, the open's mirror {forth:.4}",
                k as u64 * STEP_MS
            );
            // And it has covered as much of the way as the opening page had
            // left at the same time: neither is faster than the other.
            let left = 1.0 - opening.share(k);
            assert!(
                (back - left).abs() < 0.002,
                "{way:?}, {} ms: closing {back:.4}, the open's remainder {left:.4}",
                k as u64 * STEP_MS
            );
        }
    }
}

#[test]
fn a_subpage_close_is_its_open_played_backwards_on_a_phone() {
    let opening = run("network", PHONE, Way::Row, "MarkSub");
    opening.print("phone open");
    assert_eq!(opening.done, STEPS);
    for way in [Way::BackButton, Way::AltLeft] {
        let closing = run("network/saved", PHONE, way, "MarkSub");
        closing.print(&format!("phone close ({way:?})"));
        assert_eq!(closing.done, STEPS, "{way:?}: a close takes as long as an open");
        // The phone's close is predictive back: the screen also shrinks and
        // goes past the edge (a 5 % margin), so its centre ends at 1.05 of the
        // width and runs a little ahead of the plain slide while it shrinks
        // (the pose adds up to 8 % of the width of lead). Normalised to where
        // it ends, the screen is where the opening page was, within that lead.
        for k in 0..STEPS {
            let back = closing.share(k) / 1.05;
            let forth = opening.share(STEPS - k);
            assert!(
                (back - forth).abs() < 0.06,
                "{way:?}, {} ms: closing {back:.4}, the open's mirror {forth:.4}",
                k as u64 * STEP_MS
            );
        }
    }
}

#[test]
fn a_swipe_released_past_its_threshold_closes_over_the_same_time() {
    let closing = run("network/saved", PHONE, Way::Swipe(200.0), "MarkSub");
    closing.print("phone close (swipe released at 200 px)");
    assert_eq!(closing.done, STEPS, "a released swipe takes as long as a press of Back");
    // It carries on from the finger: never back, and ahead of where it was let go.
    for k in 1..closing.at.len() {
        assert!(closing.at[k] >= closing.at[k - 1] - 0.01, "{k}: {:?}", closing.at);
    }
    assert!(closing.at[0] > 0.0, "it starts where the finger left it");
}

#[test]
fn a_fling_carries_on_at_the_speed_of_the_finger_and_does_not_stall() {
    let rest = run("network/saved", PHONE, Way::Swipe(160.0), "MarkSub");
    let fling = run("network/saved", PHONE, Way::Fling(160.0), "MarkSub");
    fling.print("phone close (fling released at 160 px)");
    assert_eq!(fling.done, STEPS, "a fling takes as long as a press of Back");
    for k in 1..fling.at.len() {
        assert!(fling.at[k] >= fling.at[k - 1] - 0.01, "{k}: {:?}", fling.at);
    }
    // A finger let go at rest starts the close gently (a few px a frame); one
    // let go moving carries on at its speed: at least twice as far in the first
    // frame, never a stall where it lifted.
    let (slow, fast) = (rest.at[1] - rest.at[0], fling.at[1] - fling.at[0]);
    assert!(fast > 2.0 * slow && fast > 6.0, "from rest {slow:.1} px, flung {fast:.1} px");
}

#[test]
fn escape_does_not_close_a_page() {
    let closing = run("network/saved", DESKTOP, Way::Escape, "MarkSub");
    assert!(closing.at.iter().all(|x| x.abs() < 0.01));
    assert!(closing.at.len() > 10, "the page was watched for a while");
}
