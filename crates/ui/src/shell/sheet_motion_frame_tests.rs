//! A popup sheet on a phone, rendered frame by frame.
//!
//! The animation clock runs slow and each step waits longer than the
//! animation's 34 ms tick cap, so every step moves the animation by the same
//! fixed 16 ms whatever the machine's load, and the table of where the sheet
//! is at each step is the same on every run. The sheet's place is the top of
//! its title, in pixels from where it rests; the scrim is the opacity of the
//! box that covers the window.
//!
//! `BINGUX_SHEET_TABLES=1` prints every table. `BINGUX_MOTION_FRAMES=DIR`
//! (made beforehand) keeps a PNG of every step.

use std::time::Duration;

use freya::prelude::*;

use crate::controls::button::action_button;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;
use crate::window::{
    PopupAction, PopupActions, PopupFlags, Window, WindowHost, WindowLike, WindowParent, popup,
    use_popup_actions, use_window,
};

const PHONE: (f32, f32) = (390.0, 800.0);
/// What the animation moves each step, in milliseconds (the 34 ms tick cap at
/// the clock's speed, cut to whole milliseconds).
const STEP_MS: u64 = 16;
const SPEED: f32 = 0.4853;
const TITLE: &str = "Wi-Fi";

#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(Page)
    }
}

#[derive(PartialEq)]
struct Page;

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        rect()
            .expanded()
            .child(action_button("Open sheet", move |_: ()| {
                if let Window::Main(main) = window {
                    main.open(popup(TITLE, Body).flags(PopupFlags::FORM));
                }
            }))
    }
}

#[derive(PartialEq)]
struct Body;

impl Component for Body {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Accept".into(),
                enabled: true,
                on_press: EventHandler::new(move |_: ()| window.close()),
            }),
        });
        let tokens = crate::surface::use_tokens();
        rect()
            .tw(&tokens, &[w_full(), h(Dim::Box24)])
            .child("The body of the sheet")
    }
}

fn start() -> Snapshot {
    let mut shot = Snapshot::new(|| Root.into_element(), PHONE);
    shot.settle(Duration::from_millis(900));
    shot.runner().animation_clock().set_speed(SPEED);
    shot
}

/// One step of the animation clock.
fn step(shot: &mut Snapshot) {
    shot.runner().poll_n(Duration::from_millis(40), 1);
}

/// The top of the sheet's title, `NAN` when there is none.
fn title_y(shot: &mut Snapshot) -> f32 {
    shot.runner()
        .find_many(|node, element| {
            let label = Label::try_downcast(element)?;
            (label.text == TITLE).then(|| node.layout().area.min_y())
        })
        .first()
        .copied()
        .unwrap_or(f32::NAN)
}

/// The opacity of the box that covers the window, `NAN` when there is none.
fn scrim(shot: &mut Snapshot) -> f32 {
    shot.runner()
        .find_many(|node, element| {
            let opacity = element.effect()?.opacity?;
            let area = node.layout().area;
            (area.width() >= PHONE.0 - 1.0 && area.height() >= PHONE.1 - 1.0).then_some(opacity)
        })
        .first()
        .copied()
        .unwrap_or(f32::NAN)
}

/// The top of the panel (the dialog the sheet is), `NAN` when there is none.
fn panel_top(shot: &mut Snapshot) -> f32 {
    shot.runner()
        .find_many(|node, element| {
            (element.accessibility().builder.role() == accesskit::Role::Dialog)
                .then(|| node.layout().area.min_y())
        })
        .first()
        .copied()
        .unwrap_or(f32::NAN)
}

/// A run of steps: the sheet's place and the scrim at each.
#[derive(Clone, Debug, Default)]
struct Run {
    y: Vec<f32>,
    scrim: Vec<f32>,
}

impl Run {
    fn sample(&mut self, shot: &mut Snapshot, tag: &str, k: usize) {
        if let Ok(dir) = std::env::var("BINGUX_MOTION_FRAMES") {
            shot.save(format!("{dir}/{tag}-{k:02}.png"));
        }
        self.y.push(title_y(shot));
        self.scrim.push(scrim(shot));
    }

    /// `steps` steps from now, sampled after each.
    fn follow(shot: &mut Snapshot, tag: &str, steps: usize) -> Self {
        let mut run = Self::default();
        for k in 0..steps {
            step(shot);
            run.sample(shot, tag, k);
        }
        run
    }

    /// The last place it was seen at.
    fn last(&self) -> f32 {
        self.y
            .iter()
            .rev()
            .copied()
            .find(|y| !y.is_nan())
            .unwrap_or(f32::NAN)
    }

    /// The step the sheet was last seen moving on (a step after it, it has not
    /// changed by a hair or it is gone).
    fn over_on(&self) -> usize {
        let mut over = 0;
        for k in 1..self.y.len() {
            let (a, b) = (self.y[k - 1], self.y[k]);
            if b.is_nan() != a.is_nan() || (!a.is_nan() && (a - b).abs() > 0.01) {
                over = k;
            }
        }
        over
    }

    fn print(&self, label: &str, rest: f32) {
        if std::env::var_os("BINGUX_SHEET_TABLES").is_none() {
            return;
        }
        println!(
            "# {label}: over on step {} ({} ms)",
            self.over_on(),
            self.over_on() as u64 * STEP_MS
        );
        for k in 0..self.y.len().min(self.over_on() + 3) {
            let y = self.y[k];
            println!(
                "{:5} ms  {}  scrim {:.3}",
                (k as u64 + 1) * STEP_MS,
                if y.is_nan() {
                    "     gone".to_owned()
                } else {
                    format!("{:8.1} px", y - rest)
                },
                self.scrim[k]
            );
        }
    }
}

/// Open the sheet and follow it until it is in place. Returns the run and where
/// the sheet rests.
fn opened(shot: &mut Snapshot, tag: &str) -> (Run, f32) {
    shot.click_text("Open sheet");
    let run = Run::follow(shot, tag, 40);
    let rest = run.last();
    (run, rest)
}

#[derive(Clone, Copy, Debug)]
enum Way {
    Cancel,
    Escape,
    Backdrop,
    Accept,
}

fn close_by(shot: &mut Snapshot, way: Way) {
    match way {
        Way::Cancel | Way::Accept => {
            let name = if matches!(way, Way::Cancel) {
                "Close"
            } else {
                "Accept"
            };
            let button = shot
                .icon_buttons()
                .into_iter()
                .find(|button| button.name == name)
                .unwrap_or_else(|| panic!("a {name} button is on screen"));
            shot.click(
                button.area.min_x() + button.area.width() / 2.0,
                button.area.min_y() + button.area.height() / 2.0,
            );
        }
        Way::Escape => {
            shot.key(Key::Named(NamedKey::Escape));
        }
        Way::Backdrop => {
            shot.click(PHONE.0 / 2.0, 40.0);
        }
    }
}

/// A sheet opened, left to rest and closed by `way`.
fn closed(way: Way) -> (Run, f32) {
    let mut shot = start();
    let (_, rest) = opened(&mut shot, "open");
    close_by(&mut shot, way);
    (Run::follow(&mut shot, &format!("close-{way:?}"), 40), rest)
}

/// The sheet pulled down by `dy` with the pointer held still before it is let
/// go (no fling). Returns the run from the release and where it rests.
fn pulled(dy: f32, tag: &str) -> (Run, f32, f32) {
    let mut shot = start();
    let (_, rest) = opened(&mut shot, "open");
    let top = panel_top(&mut shot);
    let at = (PHONE.0 / 2.0, top + 8.0);
    shot.press(at);
    shot.move_to((at.0, at.1 + dy * 0.5));
    shot.settle(Duration::from_millis(30));
    shot.move_to((at.0, at.1 + dy));
    shot.settle(Duration::from_millis(150));
    let held = title_y(&mut shot);
    shot.release((at.0, at.1 + dy));
    (Run::follow(&mut shot, tag, 40), rest, held - rest)
}

/// Everything measured, once: the tests read the same frames.
struct Measured {
    rest: f32,
    open: Run,
    closes: Vec<(Way, Run)>,
    /// A release past the dismiss distance, and how far the sheet was held.
    dismissed: (Run, f32),
    /// A release short of it, and how far the sheet was held.
    sprung: (Run, f32),
    /// Closed 128 ms after it started to open; the step the click was on.
    turned: (Run, usize),
}

const TURN_AT: usize = 8;

fn measured() -> &'static Measured {
    &MEASURED
}

static MEASURED: std::sync::LazyLock<Measured> = std::sync::LazyLock::new(|| {
    let (open, rest) = {
        let mut shot = start();
        opened(&mut shot, "open")
    };
    let closes = [Way::Cancel, Way::Escape, Way::Backdrop, Way::Accept]
        .into_iter()
        .map(|way| (way, closed(way).0))
        .collect();
    let (past, _, held) = pulled(150.0, "pull-dismiss");
    let (short, _, short_held) = pulled(60.0, "pull-rest");
    let mut shot = start();
    shot.click_text("Open sheet");
    let mut turned = Run::default();
    for k in 0..TURN_AT {
        step(&mut shot);
        turned.sample(&mut shot, "interrupt", k);
    }
    close_by(&mut shot, Way::Cancel);
    for k in TURN_AT..48 {
        step(&mut shot);
        turned.sample(&mut shot, "interrupt", k);
    }
    Measured {
        rest,
        open,
        closes,
        dismissed: (past, held),
        sprung: (short, short_held),
        turned: (turned, TURN_AT),
    }
});

/// The sheet's place in pixels from rest at each step.
fn from_rest(run: &Run, rest: f32) -> Vec<f32> {
    run.y.iter().map(|y| y - rest).collect()
}

#[test]
fn print_the_sheet_motion_tables() {
    let m = measured();
    m.open.print("open", m.rest);
    for (way, run) in &m.closes {
        run.print(&format!("close ({way:?})"), m.rest);
    }
    m.dismissed.0.print(
        &format!("release past the threshold (held {:.0} px)", m.dismissed.1),
        m.rest,
    );
    m.sprung.0.print(
        &format!("release short of it (held {:.0} px)", m.sprung.1),
        m.rest,
    );
    m.turned.0.print("opened 128 ms, then closed", m.rest);
    assert!(
        m.rest.is_finite() && m.rest < PHONE.1,
        "the sheet comes to rest on screen: {}",
        m.rest
    );
}

/// The sheet's travel: from where it rests to where it is hidden, off the
/// window (the first frame, before it moves).
fn travel(m: &Measured) -> f32 {
    assert!(m.open.y[0] >= PHONE.1, "the sheet's first frame is off the window");
    m.open.y[0] - m.rest
}

#[test]
fn a_sheet_opens_hidden_then_eases_in_and_out_to_rest_without_a_flash() {
    let m = measured();
    let open = from_rest(&m.open, m.rest);
    let travel = travel(m);
    // Before it moves it is off the window: no frame at the final place.
    let first = open
        .iter()
        .position(|&d| d < travel - 0.5)
        .expect("it moves");
    assert!(first <= 2, "it starts within three frames: {open:?}");
    for (k, d) in open[..first].iter().enumerate() {
        assert!(
            (d - travel).abs() < 0.5,
            "frame {k} is hidden, at {d} of {travel}"
        );
    }
    // From there it never goes back, and ends exactly at rest.
    let moving = &open[first..];
    assert!(
        moving.windows(2).all(|pair| pair[1] <= pair[0] + 0.01),
        "{moving:?}"
    );
    assert_eq!(*open.last().unwrap(), 0.0, "it rests exactly");
    // Eased in and out: the pace builds to one peak in the middle of the run
    // and falls away, the first and last frames that move are a fraction of
    // it, and no frame is more than 2.6 times a constant-speed slide's (the
    // symmetric ease peaks at twice).
    let steps: Vec<f32> = moving.windows(2).map(|pair| pair[0] - pair[1]).collect();
    // The frames after it has come to rest do not count.
    let steps = &steps[..steps.iter().rposition(|&s| s > 0.01).unwrap() + 1];
    let peak = steps.iter().copied().fold(0.0_f32, f32::max);
    let at_peak = steps.iter().position(|&s| s == peak).unwrap();
    let average = travel / (crate::nav::gesture::SLIDE_MS / STEP_MS) as f32;
    assert!(peak <= 2.6 * average, "peak {peak} against an average of {average}");
    assert!(peak >= 1.5 * average, "peak {peak} against an average of {average}");
    assert!(
        (steps.len() / 3..=2 * steps.len() / 3).contains(&at_peak),
        "the peak is in the middle third: frame {at_peak} of {}",
        steps.len()
    );
    assert!(
        steps[..at_peak].windows(2).all(|pair| pair[1] >= pair[0] - 0.01),
        "the pace only builds up to the peak: {steps:?}"
    );
    assert!(
        steps[at_peak..].windows(2).all(|pair| pair[1] <= pair[0] + 0.01),
        "and only falls after it: {steps:?}"
    );
    assert!(steps[0] < 0.5 * peak, "starts gently: {} of {peak}", steps[0]);
    let last_move = steps.iter().rev().copied().find(|&s| s > 0.01).unwrap();
    assert!(last_move < 0.5 * peak, "settles gently: {last_move} of {peak}");
    // And it is as far on as a symmetric ease says before the end: more than
    // 90 % of the way with a quarter of the time left.
    let late = open[(0.75 * (crate::nav::gesture::SLIDE_MS / STEP_MS) as f32) as usize + 1];
    assert!(late < 0.12 * travel, "{late} of {travel} still to go with a quarter of the time left");
}

#[test]
fn every_way_to_close_is_the_same_motion_and_the_opening_played_backwards() {
    let m = measured();
    let open = from_rest(&m.open, m.rest);
    let travel = travel(m);
    let (_, first) = &m.closes[0];
    let close = from_rest(first, m.rest);
    for (way, run) in &m.closes {
        let this = from_rest(run, m.rest);
        for (k, (a, b)) in this.iter().zip(&close).enumerate() {
            assert!(
                (a.is_nan() && b.is_nan()) || (a - b).abs() < 0.01,
                "{way:?} differs from Cancel at step {k}: {a} vs {b}"
            );
        }
    }
    // Never forward, never back; and gone at the end.
    let on_screen: Vec<f32> = close.iter().copied().filter(|d| !d.is_nan()).collect();
    assert!(
        on_screen.windows(2).all(|pair| pair[1] >= pair[0] - 0.01),
        "{on_screen:?}"
    );
    assert!(close.last().unwrap().is_nan());
    // The opening, played backwards: step k of the close is where step
    // `N - k` of the open was, at N steps of 16 ms in the 320 ms. (The first
    // step of a run is before the animation's first tick.)
    let n = (crate::nav::gesture::SLIDE_MS / STEP_MS) as usize + 2;
    for k in 2..=n.min(close.len() - 1) {
        let (back, forth) = (close[k], open[n - k]);
        if back.is_nan() {
            continue;
        }
        assert!(
            (back - forth).abs() < 0.01 * travel,
            "step {k}: closing {back:.1}, the open's mirror {forth:.1}"
        );
    }
    // The opening, mirrored in space: at every step the close is as far from
    // rest as the open still had to go, so neither is faster than the other.
    for k in 2..=n.min(close.len() - 1) {
        let back = close[k];
        if back.is_nan() {
            continue;
        }
        // An open that has finished is at rest.
        let left = travel - open.get(k).copied().unwrap_or(0.0);
        assert!(
            (back - left).abs() < 0.01 * travel,
            "step {k}: closing {back:.1}, the open's remainder {left:.1}"
        );
    }
    // The same time to go: the last frame on screen is on the same step.
    let gone = close.iter().position(|d| d.is_nan()).unwrap();
    let done = open.iter().position(|&d| d == 0.0).unwrap();
    assert!(
        gone.abs_diff(done) <= 2,
        "the open is over on step {done}, the close on {gone}"
    );
}

#[test]
fn the_scrim_has_the_sheets_timing() {
    let m = measured();
    let travel = travel(m);
    let runs = [
        &m.open,
        &m.closes[0].1,
        &m.dismissed.0,
        &m.sprung.0,
        &m.turned.0,
    ];
    for run in runs {
        for (y, scrim) in run.y.iter().zip(&run.scrim) {
            if y.is_nan() {
                continue;
            }
            let expected = 1.0 - ((y - m.rest) / travel).clamp(0.0, 1.0);
            assert!(
                (scrim - expected).abs() < 0.01,
                "sheet at {y}, scrim {scrim}, expected {expected}"
            );
        }
    }
}

#[test]
fn a_release_past_the_threshold_carries_on_from_the_pull_and_leaves() {
    let m = measured();
    let (run, held) = &m.dismissed;
    let path = from_rest(&run, m.rest);
    assert!(
        (held - 150.0).abs() < 1.0,
        "the sheet followed the pointer: {held}"
    );
    // It starts where it was let go: not at rest, and not off the window.
    assert!(
        (path[0] - held).abs() < 1.0,
        "starts at {} from {held}",
        path[0]
    );
    let on_screen: Vec<f32> = path.iter().copied().filter(|d| !d.is_nan()).collect();
    assert!(
        on_screen.iter().all(|&d| d >= held - 1.0),
        "never back above the pull: {on_screen:?}"
    );
    assert!(
        on_screen.windows(2).all(|pair| pair[1] >= pair[0] - 0.01),
        "{on_screen:?}"
    );
    // It leaves over the same time as a close, and slows to a stop at the end.
    assert!(path.last().unwrap().is_nan());
    let steps: Vec<f32> = on_screen.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let peak = steps.iter().copied().fold(0.0_f32, f32::max);
    assert!(
        steps.last().copied().unwrap() < 0.1 * peak,
        "it comes to a stop: {steps:?}"
    );
    let gone = path.iter().position(|d| d.is_nan()).unwrap();
    let close_gone = from_rest(&m.closes[0].1, m.rest)
        .iter()
        .position(|d| d.is_nan())
        .unwrap();
    assert!(
        gone.abs_diff(close_gone) <= 2,
        "{gone} steps against a close's {close_gone}"
    );
}

#[test]
fn a_release_short_of_the_threshold_springs_back_to_rest() {
    let m = measured();
    let (run, held) = &m.sprung;
    let path = from_rest(run, m.rest);
    assert!(
        (held - 60.0).abs() < 1.0,
        "the sheet followed the pointer: {held}"
    );
    assert!(
        (path[0] - held).abs() < 1.0,
        "starts at {} from {held}",
        path[0]
    );
    // It comes back (the spring may dip a hair below rest) and ends at rest.
    assert!(path.iter().all(|&d| d.is_finite() && d > -3.0), "{path:?}");
    assert!(path.last().unwrap().abs() < 0.01, "it ends at rest: {path:?}");
    let moved = path.iter().position(|&d| d < held - 0.5).expect("it moves");
    let settled = path
        .iter()
        .position(|&d| d.abs() < 0.5)
        .expect("it settles");
    assert!(settled > moved + 4, "it takes time: {moved} to {settled}");
    let lasted = (settled - moved) as u64 * STEP_MS;
    assert!(
        (80..=240).contains(&lasted),
        "it is back within a hair of rest after {lasted} ms"
    );
    // And it is over, the dip under rest included, within its spring's time.
    let over = run.over_on() as u64 * STEP_MS;
    assert!(over <= crate::nav::gesture::SPRING_MS + 3 * STEP_MS, "it is over after {over} ms");
    // Eased, not linear: it leaves rest gently, goes fastest after a while,
    // and ends slowly.
    let steps: Vec<f32> = path[moved - 1..=settled]
        .windows(2)
        .map(|pair| pair[0] - pair[1])
        .collect();
    let fastest = steps.iter().copied().fold(0.0_f32, f32::max);
    assert!(
        steps[0] < 0.6 * fastest,
        "the first step {} of {fastest}",
        steps[0]
    );
    assert!(
        steps.last().copied().unwrap() < 0.25 * fastest,
        "the last {}",
        steps.last().unwrap()
    );
}

#[test]
fn closing_a_sheet_half_way_up_turns_round_where_it_is() {
    let m = measured();
    let (run, at) = &m.turned;
    let path = from_rest(run, m.rest);
    // Where it turned: the step after the click is the lowest it gets
    // (highest on screen); the frames either side are mirrors of each other.
    let top = path
        .iter()
        .copied()
        .filter(|d| !d.is_nan())
        .fold(f32::MAX, f32::min);
    let turn = path.iter().position(|&d| d == top).unwrap();
    assert!(
        turn >= *at - 1 && turn <= *at + 1,
        "it turned on step {turn}, the click was on {at}"
    );
    // No jump at the turn: the frame before and after it differ by no more
    // than the frames of the run up to it.
    let before = path[turn - 1] - path[turn];
    let fastest_up = path[..=turn]
        .windows(2)
        .map(|pair| pair[0] - pair[1])
        .fold(0.0_f32, f32::max);
    assert!(
        before <= fastest_up + 0.01,
        "{before} at the turn, {fastest_up} on the way up"
    );
    // It retraces the same path down: the open's mirror, at the same speed.
    let mirror = if path[turn + 1] == path[turn] {
        turn + 1
    } else {
        turn
    };
    for j in 1..=6 {
        let (down, up) = (path[mirror + j], path[turn - j]);
        assert!(
            (down - up).abs() < 1.5,
            "{j} frames later {down:.1}, {j} frames before {up:.1}"
        );
    }
    // And it closes in the time it took to open, no more: it started down on
    // the way it had come.
    assert!(path.last().unwrap().is_nan());
}
