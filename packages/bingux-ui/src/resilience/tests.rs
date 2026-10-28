//! Every class of failure the runtime catches, provoked in the headless
//! harness. Each test asserts the same four things: the window keeps rendering,
//! the boundary shows its card (or the failure is recorded and toasted when it
//! is not a render), Retry mounts the content again with fresh hook state, and
//! the siblings of the failed part are untouched.
//!
//! The test thread is the UI thread (the harness runs the runner on it), so
//! thread-locals are how a scene is told to fail. The failure log is
//! process-wide and the tests run in parallel, so each test names its failure
//! with its own marker and looks for that.

use std::cell::Cell;
use std::sync::mpsc;
use std::time::Duration;

use freya::prelude::*;
use freya_core::guard::FailureKind;

use super::{error_boundary, recent_errors};
use crate::controls::button::action_button;
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::provide_tokens;
use crate::typography::{BODY, Ink};
use crate::window::{WindowParent, popup, use_window};

thread_local! {
    /// How many scopes the failing content has had: one per mount.
    static MOUNTS: Cell<u32> = const { Cell::new(0) };
    /// Whether the render fault is armed. Cleared before Retry so the new
    /// scope does not fail again.
    static BROKEN: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Fault {
    /// An extra hook once `armed`.
    ConditionalHook,
    /// One more pass of a hook loop once `armed`.
    LoopHook,
    /// `use_state` inside an event handler.
    HookInHandler,
    /// `panic!` in `render` once `armed`.
    Render,
    /// `panic!` in a `use_side_effect` once its button is pressed.
    Effect,
    /// `panic!` in an event handler.
    Handler,
    /// `panic!` on a worker thread.
    Worker,
}

/// The part that fails. Its hooks are `use_hook`, a `State<i32>`, a `State<bool>`
/// and an effect, in that order, whatever the fault.
#[derive(PartialEq)]
struct Faulty {
    fault: Fault,
    armed: bool,
    marker: &'static str,
}

impl Component for Faulty {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        use_hook(|| MOUNTS.with(|mounts| mounts.set(mounts.get() + 1)));
        let _first = use_state(|| 0i32);
        let mut broken = use_state(|| false);
        let fault = self.fault;
        let marker = self.marker;
        use_side_effect(move || {
            if fault == Fault::Effect && *broken.read() {
                panic!("{marker} effect");
            }
        });
        match fault {
            Fault::ConditionalHook => {
                if self.armed {
                    let _extra = use_state(|| 1i32);
                }
            }
            Fault::LoopHook => {
                for _ in 0..if self.armed { 2 } else { 1 } {
                    let _extra = use_state(|| 1i32);
                }
            }
            Fault::Render => {
                if self.armed && BROKEN.with(Cell::get) {
                    panic!("{marker} render");
                }
            }
            _ => {}
        }
        let button = match fault {
            Fault::HookInHandler => Some(action_button("Hook in handler", move |()| {
                // The component's hooks are not this type: Freya cannot match it.
                let _ = use_state(|| 0u8);
            })),
            Fault::Handler => Some(action_button("Panic in handler", move |()| {
                panic!("{marker} handler");
            })),
            Fault::Effect => Some(action_button("Panic in effect", move |()| broken.set(true))),
            Fault::Worker => Some(action_button("Panic in worker", move |()| {
                spawn(async move {
                    crate::task::spawn_blocking(move || -> () { panic!("{marker} worker") }).await;
                });
            })),
            _ => None,
        };
        rect()
            .tw(&tokens, &[flex_col(), gap(S2), p(S2)])
            .child(BODY.in_ink(Ink::Text).label(&tokens, "Faulty content is fine"))
            .maybe_child(button)
    }
}

/// The window: a sibling that counts, a button that arms the fault, the faulty
/// part in its own boundary, and a button that opens a popup whose body fails.
#[derive(PartialEq)]
struct Scene {
    fault: Fault,
    marker: &'static str,
}

#[derive(PartialEq)]
struct SceneBody {
    fault: Fault,
    marker: &'static str,
}

impl Component for SceneBody {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut armed = use_state(|| false);
        let mut clicks = use_state(|| 0u32);
        let window = use_window();
        let marker = self.marker;
        rect()
            .expanded()
            .tw(&tokens, &[flex_col(), gap(S2), p(S4)])
            .child(BODY.in_ink(Ink::Text).label(&tokens, format!("Sibling {}", clicks.read())))
            .child(action_button("Count", move |()| *clicks.write() += 1))
            .child(action_button("Arm", move |()| armed.set(true)))
            .child(action_button("Open popup", move |()| {
                let _ = window.open(popup(
                    "Popup body",
                    PopupBody { marker },
                ));
            }))
            .child(error_boundary(
                "Demo",
                Faulty {
                    fault: self.fault,
                    armed: *armed.read(),
                    marker: self.marker,
                },
            ))
    }
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg)])
            .child(OverlayHost::new(SceneBody {
                fault: self.fault,
                marker: self.marker,
            }))
    }
}

/// A popup body that fails the first time it renders.
#[derive(PartialEq)]
struct PopupBody {
    marker: &'static str,
}

impl Component for PopupBody {
    fn render(&self) -> impl IntoElement {
        let _state = use_state(|| 0);
        if BROKEN.with(Cell::get) {
            panic!("{} popup", self.marker);
        }
        rect().child("Popup content is fine")
    }
}

fn start(fault: Fault, marker: &'static str) -> Snapshot {
    MOUNTS.with(|mounts| mounts.set(0));
    BROKEN.with(|broken| broken.set(false));
    let mut shot = Snapshot::new(move || Scene { fault, marker }.into(), (420.0, 700.0));
    shot.settle(Duration::from_millis(300));
    shot
}

/// The text of every label and paragraph on screen.
fn texts(shot: &mut Snapshot) -> Vec<String> {
    shot.runner().find_many(|_, element| {
        if let Some(label) = Label::try_downcast(element) {
            return Some(label.text.to_string());
        }
        Paragraph::try_downcast(element)
            .map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect())
    })
}

fn shows(shot: &mut Snapshot, needle: &str) -> bool {
    texts(shot).iter().any(|text| text.contains(needle))
}

fn press(shot: &mut Snapshot, text: &str) {
    assert!(shot.click_text(text), "no button labelled {text:?} on screen");
    shot.settle(Duration::from_millis(250));
}

fn mounts() -> u32 {
    MOUNTS.with(Cell::get)
}

fn recorded(marker: &str, kind: FailureKind) -> bool {
    recent_errors()
        .iter()
        .any(|record| record.kind == kind && record.message.contains(marker))
}

/// The card, the siblings and Retry, for a fault that fails a render.
fn assert_render_fault_is_recoverable(fault: Fault, marker: &'static str, retry_arms_off: bool) {
    let mut shot = start(fault, marker);
    assert!(shows(&mut shot, "Faulty content is fine"));
    assert_eq!(mounts(), 1);
    // Sibling state before the failure, to be found again after it.
    press(&mut shot, "Count");
    assert!(shows(&mut shot, "Sibling 1"));
    BROKEN.with(|broken| broken.set(true));

    press(&mut shot, "Arm");
    // The window keeps rendering; the boundary shows its card in place of the part.
    assert!(shows(&mut shot, "Demo ran into a problem"), "the boundary shows its card");
    assert!(shows(&mut shot, "Retry") && shows(&mut shot, "Copy details"));
    assert!(!shows(&mut shot, "Faulty content is fine"), "the failed content is gone");
    // Nothing outside the boundary was touched: it did not rerun, it kept its state.
    assert!(shows(&mut shot, "Sibling 1"), "the sibling kept its state");
    assert!(!shows(&mut shot, "The app ran into a problem"), "the window's own card stays away");
    assert!(
        recorded(marker, FailureKind::Render) || fault != Fault::Render,
        "the failure is in the log"
    );
    if fault != Fault::Render {
        assert!(
            recent_errors()
                .iter()
                .any(|record| record.kind == FailureKind::Render
                    && record.message.contains("hook")
                    && record.path.iter().any(|name| name == "Faulty")),
            "the hook-order failure names the component: {:?}",
            recent_errors().first()
        );
    }
    // The siblings are interactive while the card is up.
    press(&mut shot, "Count");
    assert!(shows(&mut shot, "Sibling 2"));

    // Retry mounts the content again in a new scope: fresh hook state.
    if retry_arms_off {
        BROKEN.with(|broken| broken.set(false));
    }
    press(&mut shot, "Retry");
    assert!(
        shows(&mut shot, "Faulty content is fine"),
        "Retry brings the content back: {:?}",
        texts(&mut shot)
    );
    assert!(!shows(&mut shot, "Demo ran into a problem"));
    assert_eq!(mounts(), 2, "Retry mounted a new scope, it did not reuse the failed one");
    assert!(shows(&mut shot, "Sibling 2"), "the sibling still has its state after Retry");
}

#[test]
fn a_conditional_hook_is_caught_and_retry_starts_clean() {
    // The fault stays armed, as the props that failed are unchanged: only the
    // fresh hook state of the new scope lets the same props render.
    assert_render_fault_is_recoverable(Fault::ConditionalHook, "conditional-hook", false);
}

#[test]
fn a_hook_in_a_loop_is_caught_and_retry_starts_clean() {
    assert_render_fault_is_recoverable(Fault::LoopHook, "hook-loop", false);
}

#[test]
fn a_panic_in_render_is_caught_and_retry_remounts() {
    assert_render_fault_is_recoverable(Fault::Render, "render-panic", true);
}

#[test]
fn a_retry_that_fails_again_shows_the_card_again() {
    let mut shot = start(Fault::Render, "render-again");
    BROKEN.with(|broken| broken.set(true));
    press(&mut shot, "Arm");
    assert!(shows(&mut shot, "Demo ran into a problem"));
    // Still broken: Retry fails again, and the window is still here to try once more.
    press(&mut shot, "Retry");
    assert!(shows(&mut shot, "Demo ran into a problem"));
    assert_eq!(mounts(), 2);
    BROKEN.with(|broken| broken.set(false));
    press(&mut shot, "Retry");
    assert!(shows(&mut shot, "Faulty content is fine"));
    assert_eq!(mounts(), 3);
}

/// A failure that is not a render leaves the content standing, is logged and
/// reaches the user as a toast.
fn assert_non_render_fault_is_a_toast(fault: Fault, marker: &'static str, button: &str, kind: FailureKind) {
    let mut shot = start(fault, marker);
    press(&mut shot, "Count");
    press(&mut shot, button);
    shot.settle(Duration::from_millis(400));
    assert!(recorded(marker, kind), "the {kind:?} failure is in the log: {:?}", recent_errors());
    assert!(shows(&mut shot, "Faulty content is fine"), "the content stays: no card for a handler");
    assert!(!shows(&mut shot, "Demo ran into a problem"));
    assert!(shows(&mut shot, "Something went wrong"), "a toast tells the user: {:?}", texts(&mut shot));
    // The window is alive and the siblings answer.
    press(&mut shot, "Count");
    assert!(shows(&mut shot, "Sibling 2"));
    assert_eq!(mounts(), 1, "nothing was remounted");
}

#[test]
fn a_panic_in_an_event_handler_is_a_toast_not_a_crash() {
    assert_non_render_fault_is_a_toast(Fault::Handler, "handler-panic", "Panic in handler", FailureKind::Event);
}

#[test]
fn a_hook_inside_an_event_handler_is_a_toast_not_a_crash() {
    assert_non_render_fault_is_a_toast(
        Fault::HookInHandler,
        "hook",
        "Hook in handler",
        FailureKind::Event,
    );
}

#[test]
fn a_panic_in_an_effect_is_a_toast_and_the_effect_lives_on() {
    let mut shot = start(Fault::Effect, "effect-panic");
    press(&mut shot, "Panic in effect");
    shot.settle(Duration::from_millis(400));
    assert!(recorded("effect-panic", FailureKind::Effect), "{:?}", recent_errors());
    assert!(shows(&mut shot, "Faulty content is fine"));
    assert!(shows(&mut shot, "Something went wrong"), "{:?}", texts(&mut shot));
    press(&mut shot, "Count");
    assert!(shows(&mut shot, "Sibling 1"));
}

#[test]
fn a_panic_on_a_worker_thread_is_recorded_and_the_pool_survives() {
    let mut shot = start(Fault::Worker, "worker-panic");
    press(&mut shot, "Panic in worker");
    // The job runs on the pool; give it, and the hook, a moment.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !recorded("worker-panic", FailureKind::Thread) && std::time::Instant::now() < deadline {
        shot.settle(Duration::from_millis(50));
    }
    assert!(recorded("worker-panic", FailureKind::Thread), "{:?}", recent_errors());
    // The pool still runs jobs after the panic.
    let (sender, receiver) = mpsc::channel();
    crate::task::spawn_detached("canary", move || {
        let _ = sender.send(7);
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(3)), Ok(7), "the pool took no damage");
    assert!(shows(&mut shot, "Faulty content is fine"));
    press(&mut shot, "Count");
    assert!(shows(&mut shot, "Sibling 1"));
}

#[test]
fn a_panic_in_popup_content_is_an_error_card_in_the_popup() {
    let mut shot = start(Fault::Render, "popup-panic");
    BROKEN.with(|broken| broken.set(true));
    press(&mut shot, "Open popup");
    shot.settle(Duration::from_millis(600));
    assert!(
        shows(&mut shot, "Popup body ran into a problem"),
        "the popup shows its card: {:?}",
        texts(&mut shot)
    );
    assert!(shows(&mut shot, "Sibling 0"), "the window behind the popup is still there");
    assert!(recorded("popup-panic", FailureKind::Render));
    assert!(!shows(&mut shot, "The app ran into a problem"));
    // Retry works inside the popup once the body stops failing.
    BROKEN.with(|broken| broken.set(false));
    press(&mut shot, "Retry");
    assert!(shows(&mut shot, "Popup content is fine"), "{:?}", texts(&mut shot));
}

/// A part with no boundary of its own fails into the window's: a full-window
/// card, and Retry works there too.
#[test]
fn a_failure_with_no_boundary_of_its_own_reaches_the_window_boundary() {
    #[derive(PartialEq)]
    struct Bare;

    impl Component for Bare {
        fn render(&self) -> impl IntoElement {
            let tokens = provide_tokens();
            if BROKEN.with(Cell::get) {
                panic!("bare-root render");
            }
            rect().tw(&tokens, &[p(S4)]).child("Window content is fine")
        }
    }

    BROKEN.with(|broken| broken.set(true));
    let mut shot = Snapshot::new(|| Bare.into(), (420.0, 500.0));
    shot.settle(Duration::from_millis(300));
    assert!(shows(&mut shot, "The app ran into a problem"), "{:?}", texts(&mut shot));
    BROKEN.with(|broken| broken.set(false));
    press(&mut shot, "Retry");
    assert!(shows(&mut shot, "Window content is fine"), "{:?}", texts(&mut shot));
}

/// The window root failing before it provides the tokens leaves the standard
/// card nothing to read; the plain card, which reads nothing, takes over.
#[test]
fn a_root_that_fails_before_the_tokens_gets_the_plain_card() {
    #[derive(PartialEq)]
    struct NoTokens;

    impl Component for NoTokens {
        fn render(&self) -> impl IntoElement {
            let _ = use_tokens();
            rect()
        }
    }

    let mut shot = Snapshot::new(|| NoTokens.into(), (420.0, 500.0));
    shot.settle(Duration::from_millis(300));
    assert!(shows(&mut shot, "The app ran into a problem"), "{:?}", texts(&mut shot));
    assert!(shows(&mut shot, "Retry") && shows(&mut shot, "Copy details"));
    assert!(recent_errors().iter().any(|record| record.message.contains("SharedTokens")));
}

#[test]
fn the_failure_log_keeps_the_path_and_a_report() {
    let mut shot = start(Fault::Render, "report-text");
    BROKEN.with(|broken| broken.set(true));
    press(&mut shot, "Arm");
    let record = recent_errors()
        .into_iter()
        .find(|record| record.message.contains("report-text"))
        .expect("the failure is recorded");
    assert_eq!(record.kind, FailureKind::Render);
    assert_eq!(record.component, "Faulty");
    assert_eq!(record.path.last().map(String::as_str), Some("Faulty"));
    assert!(record.location.as_deref().is_some_and(|at| at.contains("tests.rs")));
    assert!(super::report_text().contains("report-text"));
}
