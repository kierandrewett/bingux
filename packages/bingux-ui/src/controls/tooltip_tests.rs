//! Tooltips, frame by frame, with the pointer resting on real icon buttons.
//!
//! Each test steps the headless app one 16 ms frame at a time and looks at what
//! that frame draws: the bubble's text, where it is and how opaque. A bubble
//! that is centred on its control, at the same distance from it, in every
//! frame cannot have jumped.

use std::time::Duration;

use freya::prelude::*;

use crate::controls::icon_button;
use crate::icons::Icon;
use crate::shell::overlays::{OverlayHost, Overlays};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const WINDOW: (f32, f32) = (800.0, 600.0);
const FRAME: Duration = Duration::from_millis(16);
/// Longer than a one-line tip, so it wraps inside the widest bubble.
const LONG: &str =
    "Turns the sound off for every app and every device until you turn it back on, alerts and the volume tone too";

/// An icon button whose tooltip is `off` until it is pressed, `on` after.
#[derive(PartialEq, Clone)]
struct Toggle {
    at: (f32, f32),
    off: &'static str,
    on: &'static str,
}

impl Toggle {
    fn new(at: (f32, f32), off: &'static str, on: &'static str) -> Self {
        Self { at, off, on }
    }

    /// The point in the middle of the button.
    fn middle(&self) -> (f32, f32) {
        let half = Dim::Control.px_in(SizeClass::Regular) / 2.0;
        (self.at.0 + half, self.at.1 + half)
    }

    fn top(&self) -> f32 {
        self.at.1
    }

    fn bottom(&self) -> f32 {
        self.at.1 + Dim::Control.px_in(SizeClass::Regular)
    }

    fn centre_x(&self) -> f32 {
        self.middle().0
    }
}

impl Component for Toggle {
    fn render(&self) -> impl IntoElement {
        let mut flipped = use_state(|| false);
        rect()
            .position(Position::new_absolute().left(self.at.0).top(self.at.1))
            .child(
                icon_button(Icon::ViewReveal, move |_: ()| flipped.toggle())
                    .tooltip(if flipped() { self.on } else { self.off })
                    .size(Dim::Control),
            )
    }
}

/// The window: some toggles, and a menu that opens on the "m" key.
#[derive(PartialEq, Clone)]
struct Page {
    toggles: Vec<Toggle>,
}

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let overlays = try_consume_context::<Overlays>();
        let mut page = rect()
            .expanded()
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if let (Some(overlays), Key::Character(key)) = (overlays, &event.key)
                    && key == "m"
                {
                    overlays.show_menu(
                        (40.0, 40.0),
                        vec![crate::shell::menu_item("Open", |_| {})],
                    );
                }
            });
        for toggle in &self.toggles {
            page = page.child(Element::from(toggle.clone()));
        }
        page
    }
}

/// The window host needs the tokens above it, as an app's root component gives.
#[derive(PartialEq, Clone)]
struct Root {
    page: Page,
}

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(OverlayHost::new(self.page.clone()))
    }
}

fn app(toggles: Vec<Toggle>) -> Snapshot {
    let root = Root {
        page: Page { toggles },
    };
    Snapshot::new(move || root.clone().into_element(), WINDOW)
}

/// One bubble in one frame.
#[derive(Clone, Debug)]
struct Seen {
    text: String,
    area: Area,
    opacity: f32,
}

impl Seen {
    fn centre_x(&self) -> f32 {
        self.area.min_x() + self.area.width() / 2.0
    }
}

/// The visible bubbles in this frame. A label's bubble is the smallest box
/// around it that has an opacity of its own: the bubble's fade, or for the
/// hidden copy that measures new text, the zero that hides it. A bubble that
/// has not begun to fade in has an opacity of zero and does not count either.
fn bubbles(shot: &mut Snapshot, texts: &[&str]) -> Vec<Seen> {
    let found = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        texts
            .iter()
            .any(|text| label.text == *text)
            .then(|| (label.text.to_string(), node.layout().area))
    });
    found
        .into_iter()
        .filter_map(|(text, area)| {
            let centre = Area::center(&area);
            let around = shot.runner().find_many(|node, element| {
                let own = element.effect()?.opacity?;
                let boxed = node.layout().area;
                boxed.contains(centre).then(|| (boxed.width() * boxed.height(), own))
            });
            let (_, opacity) = around.into_iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
            (opacity > 0.0).then_some(Seen { text, area, opacity })
        })
        .collect()
}

/// A pointer move that stops at the frame its own events drew, as [`press_at`].
fn move_to(shot: &mut Snapshot, x: f32, y: f32) {
    shot.runner().move_cursor((x as f64, y as f64));
}

fn frame(shot: &mut Snapshot) {
    shot.runner().poll_n(FRAME, 1);
}

/// Steps until a bubble is fully opaque, and returns it.
fn until_shown(shot: &mut Snapshot, texts: &[&str]) -> Seen {
    for _ in 0..120 {
        frame(shot);
        if let Some(seen) = bubbles(shot, texts).into_iter().find(|seen| seen.opacity > 0.99) {
            return seen;
        }
    }
    panic!("no tooltip showed for {texts:?}");
}

/// Steps `frames` frames, returning each frame's bubbles.
fn watch(shot: &mut Snapshot, texts: &[&str], frames: usize) -> Vec<Vec<Seen>> {
    (0..frames)
        .map(|_| {
            frame(shot);
            bubbles(shot, texts)
        })
        .collect()
}

/// A click that stops at the frame its own events drew. `Snapshot::click` goes
/// on to one more update, which would run the layout events of that frame and
/// hide it: the frame that a label change draws first is the one that jumps.
fn press_at(shot: &mut Snapshot, x: f32, y: f32) {
    shot.runner().click_cursor((x as f64, y as f64));
}

/// Like [`watch`], but the first sample is the frame the last input already
/// drew, before any step: a press that changes the label draws its first frame
/// of new text there.
fn watch_from_now(shot: &mut Snapshot, texts: &[&str], frames: usize) -> Vec<Vec<Seen>> {
    let mut all = vec![bubbles(shot, texts)];
    all.extend(watch(shot, texts, frames - 1));
    all
}

/// One line per frame, for `cargo test -- --nocapture`.
fn trace(name: &str, frames: &[Vec<Seen>]) {
    for (index, seen) in frames.iter().enumerate() {
        let line: Vec<String> = seen
            .iter()
            .map(|seen| {
                format!(
                    "{:?} x {:.2}..{:.2} y {:.2}..{:.2} opacity {:.2}",
                    seen.text,
                    seen.area.min_x(),
                    seen.area.max_x(),
                    seen.area.min_y(),
                    seen.area.max_y(),
                    seen.opacity
                )
            })
            .collect();
        eprintln!("{name} frame {index:2}: {}", if line.is_empty() { "-".into() } else { line.join(" | ") });
    }
}

#[test]
fn a_new_label_never_moves_the_bubble_off_its_control() {
    let toggle = Toggle::new((300.0, 200.0), "Play", "Pause");
    let mut shot = app(vec![toggle.clone()]);
    shot.settle(Duration::from_millis(300));
    let (x, y) = toggle.middle();
    shot.hover(x, y);
    let before = until_shown(&mut shot, &["Play", "Pause"]);
    assert_eq!(before.text, "Play");
    assert!(
        (before.centre_x() - toggle.centre_x()).abs() <= 0.51,
        "the bubble opens centred on its control: {:?}",
        before.area
    );
    let steady_top = before.area.min_y();
    assert!(steady_top > toggle.bottom(), "the bubble opens below its control");

    // Press: the label changes under the resting pointer. Press again to go
    // back. Every frame of both changes is checked.
    let mut texts_seen = Vec::new();
    let mut after_width = None;
    for press in 0..2 {
        press_at(&mut shot, x, y);
        let frames = watch_from_now(&mut shot, &["Play", "Pause"], 24);
        trace(&format!("press {press}"), &frames);
        let mut last_centre = None;
        for (index, seen) in frames.iter().enumerate() {
            let [seen] = seen.as_slice() else {
                panic!("press {press}, frame {index}: {} bubbles, not one: {seen:?}", seen.len());
            };
            assert!(
                (seen.centre_x() - toggle.centre_x()).abs() <= 0.51,
                "press {press}, frame {index}: {:?} is {:.2} px off the control's middle",
                seen.text,
                seen.centre_x() - toggle.centre_x()
            );
            assert!(
                (seen.area.min_y() - steady_top).abs() <= 0.01,
                "press {press}, frame {index}: {:?} moved to y {} from {steady_top}",
                seen.text,
                seen.area.min_y()
            );
            assert!(seen.opacity > 0.99, "press {press}, frame {index}: faded to {}", seen.opacity);
            if let Some(last) = last_centre {
                assert!((seen.centre_x() - last as f32).abs() <= 0.51);
            }
            last_centre = Some(seen.centre_x());
            if texts_seen.last() != Some(&seen.text) {
                texts_seen.push(seen.text.clone());
            }
            if seen.text == "Pause" {
                after_width = Some(seen.area.width());
            }
        }
    }
    assert_eq!(
        texts_seen,
        ["Play", "Pause", "Play"],
        "each press changes the text once, with no flicker between old and new"
    );
    let after = after_width.expect("the Pause bubble was drawn");
    assert!(
        (after - before.area.width()).abs() >= 3.0,
        "the two labels must differ in width for this test to mean anything"
    );
}

#[test]
fn moving_to_a_neighbouring_control_opens_its_tip_at_once_and_never_shows_two() {
    let first = Toggle::new((300.0, 200.0), "Alpha", "Alpha");
    let second = Toggle::new((344.0, 200.0), "Beta", "Beta");
    let mut shot = app(vec![first.clone(), second.clone()]);
    shot.settle(Duration::from_millis(300));
    let (x, y) = first.middle();
    shot.hover(x, y);
    let up = until_shown(&mut shot, &["Alpha", "Beta"]);
    assert_eq!(up.text, "Alpha");

    let (x, y) = second.middle();
    move_to(&mut shot, x, y);
    // Within a dozen frames (about 190 ms, well under the 500 ms wait) the
    // second tip is up, and no frame draws both.
    let frames = watch_from_now(&mut shot, &["Alpha", "Beta"], 12);
    trace("neighbour", &frames);
    for (index, seen) in frames.iter().enumerate() {
        assert!(seen.len() <= 1, "frame {index} draws two tips: {seen:?}");
    }
    let opened = frames
        .iter()
        .position(|seen| seen.iter().any(|seen| seen.text == "Beta"))
        .expect("the neighbour's tip opens at once");
    assert!(opened <= 6, "the second tip took {opened} frames");
    // The first stays until the second is drawn and goes in that same frame:
    // no frame between them draws nothing.
    for (index, seen) in frames[..opened].iter().enumerate() {
        assert!(
            seen.len() == 1 && seen[0].text == "Alpha" && seen[0].opacity > 0.99,
            "frame {index}, before the second tip is drawn: {seen:?}"
        );
    }
    assert!(frames[opened][0].opacity > 0.99, "the second tip shows in full at once");
    let beta = until_shown(&mut shot, &["Alpha", "Beta"]);
    assert_eq!(beta.text, "Beta");
    assert!((beta.centre_x() - second.centre_x()).abs() <= 0.51);
}

#[test]
fn the_wait_comes_back_once_the_pointer_has_been_away() {
    let first = Toggle::new((300.0, 200.0), "Alpha", "Alpha");
    let second = Toggle::new((344.0, 200.0), "Beta", "Beta");
    let mut shot = app(vec![first.clone(), second.clone()]);
    shot.settle(Duration::from_millis(300));
    let (x, y) = first.middle();
    shot.hover(x, y);
    until_shown(&mut shot, &["Alpha", "Beta"]);

    // Off the controls for longer than the grace time.
    shot.hover(600.0, 500.0);
    shot.settle(Duration::from_millis(500));
    assert!(bubbles(&mut shot, &["Alpha", "Beta"]).is_empty(), "the tip goes when the pointer leaves");

    // Back on a control: nothing for the first 300 ms, then the tip.
    let (x, y) = second.middle();
    shot.hover(x, y);
    let early = watch(&mut shot, &["Alpha", "Beta"], 18);
    assert!(
        early.iter().all(Vec::is_empty),
        "a tip opened before the 500 ms rest: {:?}",
        early.iter().find(|seen| !seen.is_empty())
    );
    assert_eq!(until_shown(&mut shot, &["Alpha", "Beta"]).text, "Beta");
}

#[test]
fn a_long_label_wraps_inside_the_widest_bubble_and_flips_once_at_the_edge() {
    // A control near the bottom: a one-line tip fits below it, a wrapped one
    // does not, so it goes above. Going back to the short label keeps it above.
    let toggle = Toggle::new((300.0, WINDOW.1 - 90.0), "Mute", LONG);
    let mut shot = app(vec![toggle.clone()]);
    shot.settle(Duration::from_millis(300));
    let (x, y) = toggle.middle();
    shot.hover(x, y);
    let short = until_shown(&mut shot, &["Mute", LONG]);
    assert_eq!(short.text, "Mute");
    assert!(short.area.min_y() > toggle.bottom(), "a short tip opens below");
    let one_line = short.area.height();

    press_at(&mut shot, x, y);
    let frames = watch_from_now(&mut shot, &["Mute", LONG], 24);
    trace("grow", &frames);
    let mut flips = 0;
    let mut below = true;
    for (index, seen) in frames.iter().enumerate() {
        let [seen] = seen.as_slice() else {
            panic!("frame {index}: {} bubbles: {seen:?}", seen.len());
        };
        assert!(
            (seen.centre_x() - toggle.centre_x()).abs() <= 0.51,
            "frame {index}: {:?} off its control",
            seen.text
        );
        let now_below = seen.area.min_y() > toggle.bottom();
        if now_below != below {
            flips += 1;
            below = now_below;
        }
        assert!(seen.area.max_x() <= WINDOW.0 - 8.0 && seen.area.min_x() >= 8.0, "frame {index}: past the margin");
        assert!(seen.area.max_y() <= WINDOW.1 - 8.0 && seen.area.min_y() >= 8.0, "frame {index}: past the margin");
    }
    assert_eq!(flips, 1, "it flips above once when the text grows, not back and forth");
    let long = frames.last().and_then(|seen| seen.first()).expect("the long tip is up");
    assert_eq!(long.text, LONG);
    assert!(long.area.max_y() <= toggle.top(), "the wrapped tip sits above the control");
    assert!(long.area.height() >= 2.0 * one_line - 1.0, "the long text wraps onto more lines");
    assert!(
        long.area.width() <= Dim::TooltipMax.px_in(SizeClass::Regular),
        "the wrapped tip is {} wide, past the {} limit",
        long.area.width(),
        Dim::TooltipMax.px_in(SizeClass::Regular)
    );

    // Back to the short label: still above, still centred, still one tip.
    press_at(&mut shot, x, y);
    let back = watch_from_now(&mut shot, &["Mute", LONG], 24);
    trace("shrink", &back);
    for (index, seen) in back.iter().enumerate() {
        let [seen] = seen.as_slice() else {
            panic!("frame {index}: {} bubbles: {seen:?}", seen.len());
        };
        assert!(seen.area.max_y() <= toggle.top(), "frame {index}: {:?} came back below", seen.text);
        assert!((seen.centre_x() - toggle.centre_x()).abs() <= 0.51);
    }
}

#[test]
fn a_tip_goes_when_a_menu_opens_and_none_opens_over_it() {
    let toggle = Toggle::new((300.0, 200.0), "Copy", "Copy");
    let mut shot = app(vec![toggle.clone()]);
    shot.settle(Duration::from_millis(300));
    let (x, y) = toggle.middle();
    shot.hover(x, y);
    until_shown(&mut shot, &["Copy"]);

    shot.key(Key::Character("m".into()));
    let frames = watch(&mut shot, &["Copy"], 30);
    assert!(
        frames[10..].iter().all(Vec::is_empty),
        "a tip is still drawn under the open menu"
    );
    // The pointer moves about the control while the menu is open: no tip.
    shot.hover(x + 2.0, y + 2.0);
    shot.settle(Duration::from_millis(700));
    assert!(bubbles(&mut shot, &["Copy"]).is_empty(), "a tip opened over the menu");
}
