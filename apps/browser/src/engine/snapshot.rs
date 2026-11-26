//! The snapshot scenarios of engine.

use crate::snapshot::{Rig, Scenario, nothing, scenario, wait};
use bingux_ui::snapshot::Snapshot;
use bingux_ui::web_view::test_double::TestDoubleBackend;
use bingux_ui::web_view::{Capabilities, FailureCause};
use std::sync::Arc;

/// An engine that accepts none of the host-answered request kinds: pages and
/// favicons only. (The engine of 2026-11-25 accepts every kind the browser
/// answers; an older or a cut-down one may not.)
fn engine_of_today() -> Arc<TestDoubleBackend> {
    TestDoubleBackend::with_capabilities(Capabilities {
        favicons: true,
        ..Capabilities::default()
    })
}

/// An engine that does not start because it does not trust the profile folder.
fn engine_unsafe_profile() -> Arc<TestDoubleBackend> {
    TestDoubleBackend::failed_start(
        FailureCause::ProfileUnsafe,
        "profile_dir: /home/kieran/.local/share/bingux-browser/profile is writable by group or others.",
    )
}

/// The main menu over an engine that cannot do everything: what it cannot do
/// is disabled, not hidden.
fn capabilities(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    shot.click_name("Main menu");
    wait(shot, 500);
}

/// The engine process is killed: the window says so, and offers to start it again.
fn engine_failed(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    rig.double.fail_engine(
        FailureCause::Crashed,
        "the engine process was killed",
        137,
        true,
    );
    wait(shot, 600);
}

/// "Start again": a new engine opens the tabs again where they were.
fn engine_restart(shot: &mut Snapshot, rig: &Rig) {
    engine_failed(shot, rig);
    shot.click_text("Start again");
    wait(shot, 1_200);
}

/// The engine broke the protocol and the library ended it.
fn engine_violation(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    rig.double.fail_engine(
        FailureCause::ProtocolViolation,
        "a frame was sent for a session that is not there",
        0,
        true,
    );
    wait(shot, 600);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            engine: Some(engine_of_today),
            ..scenario("capabilities", &["example.com"], capabilities)
        },
        scenario(
            "engine-failed",
            &["example.com", "news.bingux.dev"],
            engine_failed,
        ),
        scenario(
            "engine-restart",
            &["example.com", "news.bingux.dev"],
            engine_restart,
        ),
        scenario("engine-violation", &["example.com"], engine_violation),
        Scenario {
            engine: Some(engine_unsafe_profile),
            ..scenario("profile-unsafe", &[], nothing)
        },
    ]
}
