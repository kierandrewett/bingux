//! A tab torn off a window, followed by the window, and dropped on another's
//! strip. See `design/tab-tearoff.md` for what is asked of the compositor and
//! what each one does.
//!
//! The window that holds the tab *is* the window that follows the pointer: it
//! asks the compositor to move it (`xdg_toplevel.move`, as a drag on its title
//! bar does), because that is the one request every compositor grants for a
//! press that began in the window, and a window that has just been made is not
//! one (mutter refuses a move whose press began in another surface). A window
//! with other tabs hands them to a new window first, so what follows the
//! pointer holds the one tab, as it would in Chrome.
//!
//! While the compositor moves the window the client hears nothing, and knows
//! no one's place. So a worker thread asks the compositor (sway's IPC or
//! Gnoblin's control socket, see [`crate::compositor`]) where the windows are,
//! about thirty times a second;
//! the dragged window's content origin and the pointer's place inside it
//! (constant for the whole move) give the pointer's place on the screen, and
//! each other window's published strip says whether that is over it and
//! between which tabs. The window shows the marker there. The first pointer
//! event any of the application's windows gets after the move began is the
//! compositor returning the pointer: the button is up. Over a strip, the tabs
//! join it at the marker, and the dragged window closes; elsewhere it stays.
//! Where no compositor can be asked there is no marker and no drop, and the
//! end of the move is the same.

use std::cell::RefCell;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use bingux_ui::controls::StripLayout;
use bingux_ui::shell::with_window;
use freya::prelude::*;
use freya::winit::window::WindowId;

use crate::compositor::{Anchor, Frame, Locator};
use crate::windows::{self, Entry};

/// How often the compositor is asked where the windows are.
const POLL: Duration = Duration::from_millis(25);

/// How often what was learned is looked at, and the marker moved.
const TICK: Duration = Duration::from_millis(30);

/// Pointer events that arrive this soon after the move was asked for are ones
/// that were already on their way; later ones mean the move is over.
const SETTLE: Duration = Duration::from_millis(120);

/// A strip is dropped on when the pointer is over it, or this much above or
/// below it, as a fraction of its height: a drop is not exact.
const SLACK: f32 = 0.5;

/// A window as a target for a dropped tab.
pub(crate) struct Target<W> {
    pub id: W,
    pub title: String,
    pub layout: StripLayout,
}

/// Where the pointer of a window being moved is, and which strip it is over.
///
/// `frames` is what the compositor said, `dragged` the title of the window
/// being moved, `inside` the pointer's place in that window's content, and
/// `targets` the other windows. The answer is the window and the place in its
/// tabs a tab dropped now would take, or `None` when the pointer is over none
/// of the strips, or a window cannot be found.
pub(crate) fn hover<W: Copy>(
    frames: &[Frame],
    pid: u32,
    dragged: &str,
    inside: (f32, f32),
    targets: &[Target<W>],
) -> Option<(W, usize)> {
    let of = |title: &str| frames.iter().find(|frame| frame.is_ours(pid) && frame.name == title);
    let moving = of(dragged)?;
    let (x, y) = (moving.x + inside.0, moving.y + inside.1);
    targets.iter().filter(|target| target.title != dragged).find_map(|target| {
        let frame = of(&target.title).filter(|frame| frame.shows_with(moving))?;
        let (rail_top, rail_bottom) = (target.layout.rail.1, target.layout.rail.3);
        let slack = (rail_bottom - rail_top) * SLACK;
        let local = (x - frame.x, y - frame.y);
        target
            .layout
            .accepts(local.0, local.1, frame.width, slack)
            .then(|| (target.id, target.layout.index_at(local.0)))
    })
}

/// `frames` with each of this application's windows (`sizes` has each one's
/// title and the size of its content) put where its content is, which is what
/// the pointer's place in it is measured from: a compositor that draws a frame
/// around a window reports the window with it (see [`Frame::content`]).
fn inside_the_frames(frames: Vec<Frame>, pid: u32, sizes: &[(String, (f32, f32))]) -> Vec<Frame> {
    frames
        .into_iter()
        .map(|frame| match sizes.iter().find(|(title, _)| frame.is_ours(pid) && *title == frame.name) {
            Some((_, size)) => frame.content(*size),
            None => frame,
        })
        .collect()
}

/// How long a carry is watched at most. A move that ends where no Files window
/// hears the pointer come back (over the desktop, the dock, another
/// application) is only noticed by this, and it stops the polling and fades.
const MAX_CARRY: Duration = Duration::from_secs(90);

/// This many answers in a row missed and what was learned is forgotten (and
/// the marker with it): a compositor that has stopped answering is not
/// telling where the windows are.
const FORGET_AFTER: u32 = 3;

/// The worker that asks the compositor where the windows are. It stops when
/// this is dropped.
struct Poller {
    /// What the worker last learned.
    latest: Arc<Mutex<Option<Vec<Frame>>>>,
    _stop: mpsc::Sender<()>,
}

/// Ask `locator` where the windows are, every [`POLL`], on a thread of its own.
fn poll(mut locator: Locator) -> Poller {
    let (stop, stopped) = mpsc::channel::<()>();
    let latest: Arc<Mutex<Option<Vec<Frame>>>> = Arc::default();
    let learned = latest.clone();
    std::thread::spawn(move || {
        let mut missed = 0;
        loop {
            match locator.frames() {
                Some(frames) => {
                    missed = 0;
                    *learned.lock().unwrap_or_else(PoisonError::into_inner) = Some(frames);
                }
                None => {
                    missed += 1;
                    if missed == FORGET_AFTER {
                        *learned.lock().unwrap_or_else(PoisonError::into_inner) = None;
                    }
                }
            }
            // The stop is the sender going away; waiting is the pacing.
            if !matches!(stopped.recv_timeout(POLL), Err(RecvTimeoutError::Timeout)) {
                break;
            }
        }
    });
    Poller { latest, _stop: stop }
}

/// A tear-off in progress.
struct Session {
    dragged: WindowId,
    /// The pointer's place in the dragged window when the move began.
    inside: (f32, f32),
    started: Instant,
    /// Asking the compositor where the windows are; none where there is no
    /// way to ask, and the session then only waits for the end of the move.
    poller: Option<Poller>,
    /// The strip the pointer is over now, and the place its marker is at.
    over: Option<(WindowId, usize)>,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// Where in the window the compositor holds the pointer for the whole move: a
/// compositor that begins the move where the pointer is has the pointer where
/// it was when it was asked, one that begins it where the button went down has
/// it where it went down.
fn held_at(anchor: Anchor, pointer: (f32, f32), press: (f32, f32)) -> (f32, f32) {
    match anchor {
        Anchor::Pointer => pointer,
        Anchor::Press => press,
    }
}

/// The window `dragged` is following the pointer, which was at `pointer` in it
/// when the move was asked for, and at `press` when its button went down. Asks
/// the compositor to move it, and starts watching for where it goes.
///
/// Where the compositor offers no way to ask where windows are (GNOME Shell,
/// KDE; see [`crate::compositor`]) the window is still carried, no marker is
/// shown and a release anywhere leaves it where it is: the tab is in a window
/// of its own and nothing is waiting on it but the first pointer event that
/// ends the session.
pub(crate) fn begin(dragged: &Entry, pointer: (f32, f32), press: (f32, f32)) {
    with_window(|window| {
        let _ = window.drag_window();
    });
    let locator = Locator::detect();
    let inside = locator.as_ref().map_or(pointer, |locator| held_at(locator.anchor(), pointer, press));
    let session = Session {
        dragged: dragged.id,
        inside,
        started: Instant::now(),
        poller: locator.map(poll),
        over: None,
    };
    if let Some(old) = SESSION.with_borrow_mut(|slot| slot.replace(session)) {
        clear_marker(old.over);
    }
    spawn(async move {
        loop {
            timer(TICK).await;
            if !tick() {
                break;
            }
        }
    });
}

/// Look at what the worker learned and move the marker. False once there is
/// no session to watch.
fn tick() -> bool {
    let Some((dragged, inside, learned, was, age)) = SESSION.with_borrow(|slot| {
        slot.as_ref().map(|session| {
            let learned = session.poller.as_ref().map(|poller| poller.latest.clone());
            (session.dragged, session.inside, learned, session.over, session.started.elapsed())
        })
    }) else {
        return false;
    };
    if age > MAX_CARRY {
        // Nothing told the application the move had ended (it ended over a
        // window that is not its own): put the window down where it is.
        clear_marker(was);
        finish(None);
        return false;
    }
    // Without a poller there is nothing to look at, and no marker to show.
    let Some(learned) = learned else {
        return true;
    };
    let frames = learned.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let Some(title) = windows::find(dragged).map(|window| window.title.borrow().clone()) else {
        return true;
    };
    let targets: Vec<Target<WindowId>> = windows::all()
        .into_iter()
        .filter(|window| window.id != dragged)
        .map(|window| Target { id: window.id, title: window.title.borrow().clone(), layout: window.probe.layout() })
        .collect();
    // What is not known (the first answer is not in, or the compositor has
    // stopped answering) shows no marker.
    let sizes: Vec<(String, (f32, f32))> =
        windows::all().into_iter().map(|window| (window.title.borrow().clone(), window.size.get())).collect();
    let now = frames.and_then(|frames| {
        hover(&inside_the_frames(frames, std::process::id(), &sizes), std::process::id(), &title, inside, &targets)
    });
    if now == was {
        return true;
    }
    // The window being carried is faded while it is over a strip, so the
    // marker the strip shows under it can be seen.
    if let Some(mut ghost) = windows::find(dragged).map(|window| window.ghost) {
        ghost.set_if_modified(now.is_some());
    }
    if was.map(|(id, _)| id) != now.map(|(id, _)| id) {
        clear_marker(was);
    }
    if let Some((id, at)) = now {
        if let Some(mut marker) = windows::find(id).map(|window| window.marker) {
            marker.set_if_modified(Some(at));
        }
    }
    SESSION.with_borrow_mut(|slot| {
        if let Some(session) = slot.as_mut() {
            session.over = now;
        }
    });
    true
}

fn clear_marker(over: Option<(WindowId, usize)>) {
    if let Some((id, _)) = over {
        if let Some(mut marker) = windows::find(id).map(|window| window.marker) {
            marker.set_if_modified(None);
        }
    }
}

/// A pointer event reached one of the application's windows. While the
/// compositor moves a window it sends the pointer to no one; the first event
/// to reach any window after the move had time to begin is the compositor
/// giving the pointer back: the button is up. (Not necessarily the carried
/// window's own: when it is over another window's strip, the pointer comes
/// back to that window.)
pub(crate) fn pointer_arrived() {
    let over = SESSION.with_borrow(|slot| {
        slot.as_ref().filter(|session| session.started.elapsed() >= SETTLE).map(|session| session.over)
    });
    if let Some(over) = over {
        finish(over);
    }
}

/// A window was closed: a session that was moving it, or marking it, ends.
pub(crate) fn window_closed(window: WindowId) {
    let ended = SESSION.with_borrow_mut(|slot| {
        let ends = slot.as_ref().is_some_and(|session| {
            session.dragged == window || session.over.is_some_and(|(over, _)| over == window)
        });
        ends.then(|| slot.take()).flatten()
    });
    if let Some(session) = ended {
        clear_marker(session.over.filter(|(over, _)| *over != window));
    }
}

/// The move is over. Over a strip, the dragged window's tabs join it at the
/// marker and the window closes.
fn finish(over: Option<(WindowId, usize)>) {
    let Some(session) = SESSION.with_borrow_mut(Option::take) else {
        return;
    };
    drop(session.poller);
    clear_marker(over);
    if let Some(mut ghost) = windows::find(session.dragged).map(|window| window.ghost) {
        ghost.set_if_modified(false);
    }
    let Some((target, at)) = over else {
        return;
    };
    let (Some(from), Some(to)) = (windows::find(session.dragged), windows::find(target)) else {
        return;
    };
    let (mut from_set, mut to_set) = (from.set, to.set);
    let moved = from_set.write().detach_all();
    if moved.is_empty() {
        return;
    }
    to_set.write().insert_moved(at, moved);
    Platform::get().focus_window(target);
    Platform::get().close_window(session.dragged);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(name: &str, x: f32, y: f32, width: f32, height: f32) -> Frame {
        Frame {
            name: name.into(),
            app_id: "bingux-files".into(),
            pid: Some(7),
            x,
            y,
            width,
            height,
            workspace: None,
            hidden: false,
        }
    }

    fn strip() -> StripLayout {
        // Three bodies from x = 20, 100 wide, 2 apart; the row is 36 tall.
        StripLayout {
            rail: (12.0, 0.0, 400.0, 36.0),
            spans: vec![(20.0, 120.0), (122.0, 222.0), (224.0, 324.0)],
            rtl: false,
        }
    }

    fn target(id: u8, title: &str) -> Target<u8> {
        Target { id, title: title.into(), layout: strip() }
    }

    #[test]
    fn a_pointer_over_another_windows_strip_asks_for_the_place_between_its_tabs() {
        let frames = [
            frame("Home \u{2013} Files", 100.0, 100.0, 600.0, 400.0),
            frame("Docs \u{2013} Files (2)", 800.0, 50.0, 600.0, 400.0),
        ];
        let targets = [target(2, "Docs \u{2013} Files (2)")];
        // The pointer is at (980, 68) on the screen: 180 across the target's
        // window and 18 down, on its strip. In the dragged window, whose content
        // begins at (100, 100), that is (880, -32).
        let got = hover(&frames, 7, "Home \u{2013} Files", (880.0, -32.0), &targets);
        assert_eq!(got, Some((2, 2)), "x 180 is past the second tab's middle (172) and short of the third's (274)");
    }

    #[test]
    fn a_pointer_over_the_page_or_over_nothing_is_over_no_strip() {
        let frames = [
            frame("Home \u{2013} Files", 100.0, 100.0, 600.0, 400.0),
            frame("Docs \u{2013} Files (2)", 800.0, 50.0, 600.0, 400.0),
        ];
        let targets = [target(2, "Docs \u{2013} Files (2)")];
        // Over the target's page (200 below its top).
        assert_eq!(hover(&frames, 7, "Home \u{2013} Files", (800.0 + 100.0 - 100.0, 50.0 + 200.0 - 100.0), &targets), None);
        // Over the desktop.
        assert_eq!(hover(&frames, 7, "Home \u{2013} Files", (10.0, 10.0), &targets), None);
        // Over the dragged window's own strip is not a drop on anything.
        assert_eq!(hover(&frames, 7, "Home \u{2013} Files", (50.0, 10.0), &[target(1, "Home \u{2013} Files")]), None);
    }

    #[test]
    fn a_window_the_compositor_does_not_list_is_not_a_target() {
        let frames = [frame("Home \u{2013} Files", 100.0, 100.0, 600.0, 400.0)];
        let targets = [target(2, "Docs \u{2013} Files (2)")];
        assert_eq!(hover(&frames, 7, "Home \u{2013} Files", (700.0, 0.0), &targets), None, "no frame, no place");
        // Nor is the dragged window itself when it cannot be found.
        assert_eq!(hover(&frames, 7, "Gone", (0.0, 0.0), &targets), None);
        // Windows of another process with the same title are not ours.
        assert_eq!(hover(&frames, 8, "Home \u{2013} Files", (0.0, 0.0), &targets), None);
    }

    #[test]
    fn the_empty_bar_beside_the_tabs_drops_at_the_end_and_a_drop_a_little_off_still_counts() {
        let frames = [
            frame("A \u{2013} Files", 0.0, 0.0, 600.0, 400.0),
            frame("B \u{2013} Files (2)", 700.0, 0.0, 600.0, 400.0),
        ];
        let targets = [target(2, "B \u{2013} Files (2)")];
        // x = 560 in the target, y = 18: the bar, past the last tab.
        let at = |x: f32, y: f32| hover(&frames, 7, "A \u{2013} Files", (700.0 + x, y), &targets);
        assert_eq!(at(560.0, 18.0), Some((2, 3)));
        assert_eq!(at(560.0, 36.0 + 17.0), Some((2, 3)), "half a strip below still counts");
        assert_eq!(at(560.0, 36.0 + 19.0), None, "but not a whole one");
        assert_eq!(at(30.0, 18.0), Some((2, 0)), "before the middle of the first tab");
    }

    // ---- The compositor is Gnoblin: logical pixels, many monitors, no pids ----

    use crate::compositor::fake::{Fake, windows_list};
    use crate::compositor::parse_gnoblin;
    use std::sync::atomic::Ordering;

    const HOME: &str = "Home \u{2013} Files";
    const DOCS: &str = "Documents \u{2013} Files (2)";

    /// Wait for `condition`, up to two seconds.
    fn eventually(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        condition()
    }

    fn learned(poller: &Poller) -> Option<Vec<Frame>> {
        poller.latest.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    #[test]
    fn mutters_move_holds_the_pointer_where_the_button_went_down_and_the_drop_is_found_from_there() {
        // Measured on Gnoblin 0.1.10: a tab pressed at (215, 22) of its window
        // was torn when the pointer was at (215, 94); the window followed the
        // pointer with (215, 22) under it. Documents is at (1940, 60), its strip
        // 44 tall. The pointer is at (2070, 100) on the layout: Home is at
        // (1855, 78), and the pointer over Documents's strip, 130 across it.
        let strip = StripLayout { rail: (2.0, 0.0, 408.0, 44.0), spans: vec![(12.0, 178.0), (184.0, 350.0)], rtl: false };
        let targets = [Target { id: 2_u8, title: DOCS.into(), layout: strip }];
        let frames = [frame(HOME, 1855.0, 78.0, 640.0, 420.0), frame(DOCS, 1940.0, 60.0, 640.0, 420.0)];
        let (pointer, press) = ((215.0, 94.0), (215.0, 22.0));
        assert_eq!(held_at(Anchor::Press, pointer, press), press);
        assert_eq!(hover(&frames, 7, HOME, held_at(Anchor::Press, pointer, press), &targets), Some((2, 1)));
        // Taking the pointer's place at the tear for the anchor puts the pointer
        // 72 pixels too low, on Documents's page: no drop, and no marker.
        assert_eq!(held_at(Anchor::Pointer, pointer, press), pointer);
        assert_eq!(hover(&frames, 7, HOME, held_at(Anchor::Pointer, pointer, press), &targets), None);
    }

    #[test]
    fn a_drop_on_a_monitor_left_of_the_first_is_found_with_gnoblins_negative_coordinates() {
        // Home is on the first monitor at (100, 60). Documents is on a second
        // monitor left of it, at (-1280, -120), as Gnoblin places it. The
        // pointer is at (-1100, -102) on the layout: 180 across Documents and
        // 18 down, on its strip, so in Home it is at (-1200, -162).
        let frames = parse_gnoblin(&windows_list());
        let targets = [target(2, DOCS)];
        assert_eq!(hover(&frames, 4242, HOME, (-1200.0, -162.0), &targets), Some((2, 2)));
        // The same pointer one page-height lower is over its page.
        assert_eq!(hover(&frames, 4242, HOME, (-1200.0, -162.0 + 200.0), &targets), None);
        // Nobody is asked for a pid: Gnoblin names none, the application id does.
        assert_eq!(hover(&frames, 1, HOME, (-1200.0, -162.0), &targets), Some((2, 2)));
    }

    #[test]
    fn on_a_scaled_monitor_the_pointer_and_the_frames_are_the_same_logical_pixels() {
        // A monitor at scale 1.5 draws 1.5 pixels for each logical one, and
        // Gnoblin's frames and Freya's pointer events are both in logical
        // pixels. A pointer 45 x 105 physical pixels into Home is at 30 x 70
        // logical, and the window's strip is laid out in logical pixels too.
        let scale = 1.5_f32;
        let logical = |physical: f32| physical / scale;
        let frames = [frame(HOME, 1000.0, 200.0, 600.0, 400.0), frame(DOCS, 1700.0, 100.0, 600.0, 400.0)];
        let targets = [target(2, DOCS)];
        // Documents's tab strip, 180 logical pixels across and 18 down, is at
        // (1880, 118): 880 x -82 in Home's own logical pixels. In physical ones
        // the pointer is 1320 x -123 from Home's origin.
        let inside = (logical(1320.0), logical(-123.0));
        assert_eq!(inside, (880.0, -82.0));
        assert_eq!(hover(&frames, 7, HOME, inside, &targets), Some((2, 2)));
        // The physical pixels read as logical ones are over the page or beyond
        // it: the unit has to be the compositor's.
        assert_eq!(hover(&frames, 7, HOME, (1320.0, -123.0), &targets), None);
    }

    #[test]
    fn a_fractional_pointer_is_not_rounded_across_a_tab_middle() {
        let frames = [frame(HOME, 100.0, 100.0, 600.0, 400.0), frame(DOCS, 800.0, 50.0, 600.0, 400.0)];
        let targets = [target(2, DOCS)];
        // The second tab's middle is at x = 172 in the target. 171.9 is short
        // of it and 172.1 is past it, whatever whole pixel the compositor's
        // frame was rounded to.
        let at = |x: f32| hover(&frames, 7, HOME, (700.0 + x, -32.0), &targets);
        assert_eq!(at(171.9).map(|got| got.1), Some(1));
        assert_eq!(at(172.1).map(|got| got.1), Some(2));
    }

    #[test]
    fn a_window_that_is_minimized_or_on_another_workspace_is_not_a_target() {
        let mut frames = parse_gnoblin(&windows_list());
        let targets = [target(2, DOCS)];
        let over = (-1200.0, -162.0);
        assert_eq!(hover(&frames, 1, HOME, over, &targets), Some((2, 2)));
        frames[1].hidden = true;
        assert_eq!(hover(&frames, 1, HOME, over, &targets), None, "minimized: nowhere on the screen");
        frames[1].hidden = false;
        frames[1].workspace = Some("workspace-2".into());
        assert_eq!(hover(&frames, 1, HOME, over, &targets), None, "on another workspace");
        frames[1].workspace = None;
        assert_eq!(hover(&frames, 1, HOME, over, &targets), Some((2, 2)), "on every workspace");
    }

    #[test]
    fn the_poller_asks_gnoblin_over_one_connection_and_follows_the_windows() {
        let fake = Fake::start(windows_list());
        let poller = poll(fake.locator());
        assert!(eventually(|| learned(&poller).is_some()), "the first answer comes in");
        assert_eq!(learned(&poller), Some(parse_gnoblin(&windows_list())));
        // The pointer is over Documents's strip, then Documents is moved
        // (the compositor's list changes) and the pointer is no longer there.
        let targets = [target(2, DOCS)];
        let frames = learned(&poller).expect("frames");
        assert_eq!(hover(&frames, 1, HOME, (-1200.0, -162.0), &targets), Some((2, 2)));
        let mut moved = windows_list();
        moved[1]["frame"]["x"] = serde_json::json!(400);
        *fake.result.lock().expect("result") = moved.clone();
        assert!(eventually(|| learned(&poller).is_some_and(|frames| frames[1].x == 400.0)), "the new place is learned");
        let frames = learned(&poller).expect("frames");
        assert_eq!(hover(&frames, 1, HOME, (-1200.0, -162.0), &targets), None);
        assert_eq!(fake.accepted.load(Ordering::Relaxed), 1, "one connection for all the polls");
        assert!(
            eventually(|| fake.requests.lock().expect("requests").len() >= 3),
            "and several questions, a poll apart"
        );
        // Dropping the poller (the session ending) ends the worker, which
        // closes its connection.
        drop(poller);
        assert!(eventually(|| fake.hung_up.load(Ordering::Relaxed) == 1), "the worker stopped and closed the socket");
    }

    #[test]
    fn a_frame_drawn_around_a_window_is_not_part_of_the_content_the_pointer_is_measured_from() {
        // Home has Gnoblin's native frame, {32, 1, 1, 1}, so its frame is 602 x
        // 433 at (99, 60) and its content begins at (100, 92). Documents has
        // Bingux's, {36, 0, 0, 0}: 600 x 436 at (-1280, -120), content at
        // (-1280, -84). The pointer is 180 across Documents's content and 52
        // down, on the strip's slack, which is (-1100, -32) on the layout and
        // (-1200, -124) in Home's content.
        let raw = vec![frame(HOME, 99.0, 60.0, 602.0, 433.0), frame(DOCS, -1280.0, -120.0, 600.0, 436.0)];
        let sizes = [(HOME.to_string(), (600.0, 400.0)), (DOCS.to_string(), (600.0, 400.0))];
        let placed = inside_the_frames(raw.clone(), 7, &sizes);
        assert_eq!((placed[0].x, placed[0].y, placed[0].width, placed[0].height), (100.0, 92.0, 600.0, 400.0));
        assert_eq!((placed[1].x, placed[1].y, placed[1].width, placed[1].height), (-1280.0, -84.0, 600.0, 400.0));
        let targets = [target(2, DOCS)];
        assert_eq!(hover(&placed, 7, HOME, (-1200.0, -124.0), &targets), Some((2, 2)));
        assert_eq!(hover(&raw, 7, HOME, (-1200.0, -124.0), &targets), None, "the frames as listed put the pointer off the strip");
        // A window that is not ours, or that no window of ours has the title of,
        // is left as the compositor said.
        let other = Frame { app_id: "org.example.Editor".into(), pid: None, ..frame(HOME, 1.0, 2.0, 602.0, 433.0) };
        assert_eq!(inside_the_frames(vec![other.clone()], 7, &sizes), vec![other]);
        let unknown = frame("Other \u{2013} Files", 1.0, 2.0, 602.0, 433.0);
        assert_eq!(inside_the_frames(vec![unknown.clone()], 7, &sizes), vec![unknown]);
    }

    /// Against a running Gnoblin: `GNOBLIN_COMPOSITOR_SOCKET=... cargo test
    /// live_gnoblin -- --ignored --nocapture`. Prints what the compositor says
    /// and requires one answer with at least one window.
    #[test]
    #[ignore = "needs a running Gnoblin and its control socket"]
    fn live_gnoblin_lists_its_windows() {
        let mut locator = Locator::detect().expect("a compositor to ask: set GNOBLIN_COMPOSITOR_SOCKET");
        let frames = (0..40)
            .find_map(|_| {
                let frames = locator.frames();
                std::thread::sleep(Duration::from_millis(250));
                frames.filter(|frames| !frames.is_empty())
            })
            .expect("Gnoblin answered windows.list with at least one window");
        for frame in &frames {
            eprintln!("{frame:?}");
        }
    }

    #[test]
    fn a_compositor_that_stops_answering_is_forgotten_and_found_again() {
        let fake = Fake::start(windows_list());
        let poller = poll(fake.locator());
        assert!(eventually(|| learned(&poller).is_some()));
        *fake.refuse.lock().expect("refuse") = Some("Lua worker is restarting");
        assert!(eventually(|| learned(&poller).is_none()), "after a few misses no place is claimed");
        *fake.refuse.lock().expect("refuse") = None;
        assert!(eventually(|| learned(&poller).is_some()), "and the answers resume on the same connection");
        assert_eq!(fake.accepted.load(Ordering::Relaxed), 1);
    }
}
