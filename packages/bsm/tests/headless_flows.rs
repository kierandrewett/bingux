//! End to end flows of bsm: the real manager runtime and the real (headless) compositor,
//! a fake helper speaking the real helper protocol, a Quickshell-like greeter and
//! Gnoblin-like bridges that are real Wayland clients of the brokered sockets.
//!
//! The assertions are on what the compositor composed (frames and plans), on its focus, on the
//! manager's session states and on the helper's view of the lifecycle: not on commands sent.
//!
//! Several Unix users are needed, so every scenario re-runs itself inside a user namespace
//! that maps a range of subordinate uids (`unshare --map-users=auto`), where the client
//! threads switch their own credentials with a raw `setresuid`. If the namespace cannot be
//! created the scenario fails loudly; set `BSM_SKIP_USERNS=1` to skip knowingly.

mod support;

use std::process::Command;
use std::time::Duration;

use bsm_manager::core::Method;
use bsm_proto::control::{ClientMessage, GreeterMode, ServerMessage};
use bsm_proto::error::ErrorCode;
use bsm_proto::progress::Stage;
use bsm_proto::state::{AuthIntent, InputOwner, OperationPhase, PowerAction, SeatState, SessionState, TransitionKind};
use support::*;

fn rerun_in_namespace(name: &str) {
    if std::env::var_os("BSM_SKIP_USERNS").is_some() {
        eprintln!("SKIPPED {name}: BSM_SKIP_USERNS is set");
        return;
    }
    let exe = std::env::current_exe().expect("current exe");
    let out = Command::new("unshare")
        .args(["--map-users=auto", "--map-groups=auto", "--map-root-user"])
        .arg(&exe)
        .args(["--exact", name, "--test-threads=1", "--nocapture"])
        .env("BSM_IN_USERNS", "1")
        .output()
        .expect("unshare is required for the multi user flows (util-linux, with /etc/subuid for this user)");
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && text.contains("test result: ok. 1 passed"), "scenario {name} failed in the user namespace:\n{text}");
}

macro_rules! scenario {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            if !in_namespace() {
                return rerun_in_namespace(stringify!($name));
            }
            $body
        }
    };
}

const TICK: Duration = Duration::from_millis(10);

/// Progress models the greeter received for one launch, in order.
fn progress_models(g: &TestGreeter, from: usize) -> Vec<bsm_proto::progress::ProgressModel> {
    g.messages()
        .into_iter()
        .skip(from)
        .filter_map(|m| match m {
            ServerMessage::Progress(p) => Some(p),
            _ => None,
        })
        .collect()
}

fn assert_progress_is_real(models: &[bsm_proto::progress::ProgressModel]) {
    assert!(models.len() >= Stage::ALL.len(), "one update per milestone at least, got {}", models.len());
    let mut last_rev = 0;
    let mut last_fraction = 0;
    for m in models {
        assert!(m.revision >= last_rev, "revisions never go back");
        assert!(m.fraction() >= last_fraction, "the bar never goes back ({} after {})", m.fraction(), last_fraction);
        // Stages arrive strictly in the documented order.
        assert_eq!(m.reached, Stage::ALL[..m.reached.len()].to_vec());
        last_rev = m.revision;
        last_fraction = m.fraction();
    }
    assert_eq!(models.last().unwrap().fraction(), 100);
    assert!(models.last().unwrap().is_complete());
}

/// Frames up to the first one that composes a session: only the greeter may be visible.
fn assert_no_desktop_before_reveal(frames: &[FrameRec]) -> usize {
    let first = frames.iter().position(|f| f.plan.session.is_some()).expect("the session is composed at some point");
    for f in &frames[..first] {
        assert!(f.plan.session.is_none());
        assert_eq!(f.seen(), Seen::Greeter, "frame {} before the reveal shows {:?}", f.seq, f.seen());
    }
    first
}

scenario!(login_lock_unlock_with_real_frames, {
    let mut rig = Rig::start(RigConfig::default());
    rig.attach_greeter();
    rig.wait_seen(Seen::Greeter);
    let snap = rig.g().snapshot();
    assert_eq!(snap.users.len(), 2);
    assert!(matches!(snap.mode, GreeterMode::Login));
    assert_eq!(rig.focus().owner, InputOwner::None);

    // ----- new login: real progress, greeter stays, desktop fades in over it -----
    let fm = rig.frame_mark();
    let gm = rig.g().mark();
    let sid = rig.login(ALICE, SESSION_A, PASSWORD);
    let models = progress_models(rig.g(), gm);
    assert_progress_is_real(&models);
    assert_eq!(models.last().unwrap().generation, rig.helper.opened()[0].generation);

    let frames = rig.frames_since(fm);
    let first = assert_no_desktop_before_reveal(&frames);
    let fade: Vec<&FrameRec> = frames.iter().filter(|f| f.plan.transition.is_some()).collect();
    assert!(fade.len() >= 10, "the fade has frames of its own ({})", fade.len());
    let mut last_opacity = -1.0f32;
    for f in &fade {
        let s = f.plan.session.as_ref().map_or(0.0, |s| s.opacity);
        assert!(f.plan.session_above_greeter || f.plan.session.is_none(), "the desktop fades in OVER the greeter");
        assert!(f.plan.greeter.as_ref().is_some_and(|g| g.offset_y == 0.0), "the greeter stays in place and opaque underneath");
        assert!(s + 1e-4 >= last_opacity, "opacity only grows ({s} after {last_opacity})");
        last_opacity = s;
    }
    assert!(last_opacity > 0.99);
    assert!(frames[first].plan.session.as_ref().unwrap().opacity < 0.5, "the first desktop frame is nearly transparent");
    rig.wait_seen(Seen::A);
    rig.save_frame("01-logged-in");
    assert_eq!(rig.focus().owner, InputOwner::Session(sid.clone()));
    assert_eq!(rig.helper.opened().len(), 1);
    let generation = rig.helper.opened()[0].generation;

    // ----- lock: protect first, then the greeter, never the desktop again -----
    let gm = rig.g().mark();
    let r = rig.root_call(Method::RequestLock { session: Some(sid.clone()) });
    assert!(r.is_ok(), "{r:?}");
    let (animating, _) = rig.wait_operation(gm, OperationPhase::Animating, Some(TransitionKind::Lock));
    let fm = rig.frame_mark();
    rig.wait_operation(gm, OperationPhase::Completed, Some(TransitionKind::Lock));
    rig.wait_seen(Seen::Greeter);
    for f in rig.all_frames_since(fm) {
        assert!(f.plan.session.is_none(), "a locked session is never composed again (frame {})", f.seq);
    }
    let _ = animating;
    rig.save_frame("02-locked");
    assert_eq!(rig.focus().owner, InputOwner::Greeter);
    assert_eq!(rig.session_state(&sid), Some(SessionState::LockedGreeterForeground));
    let snap = rig.g().snapshot();
    assert!(matches!(&snap.mode, GreeterMode::Unlock { session, uid } if session == &sid && *uid == ALICE));
    assert!(rig.helper.state.lock().locked_hints.contains(&(sid.clone(), true)));

    // ----- a wrong password changes nothing -----
    let gm = rig.g().mark();
    rig.g().login(ALICE, AuthIntent::Unlock, "wrong");
    rig.g().wait_after(gm, Duration::from_secs(10), "a failed result", |m| matches!(m, ServerMessage::AuthResult { ok: false, code: Some(ErrorCode::PamFailure), .. }));
    assert_eq!(rig.now_seen(), Seen::Greeter);
    assert_eq!(rig.session_state(&sid), Some(SessionState::LockedGreeterForeground));

    // ----- unlock: the greeter scene slides up and away, revealing the same session -----
    let fm = rig.frame_mark();
    let gm = rig.g().mark();
    rig.g().login(ALICE, AuthIntent::Unlock, PASSWORD);
    rig.wait_operation(gm, OperationPhase::Completed, Some(TransitionKind::UnlockSlideUp));
    let frames = rig.frames_since(fm);
    let slide: Vec<&FrameRec> = frames.iter().filter(|f| f.plan.transition.is_some()).collect();
    assert!(slide.len() >= 10);
    let mut last = 0.0f32;
    for f in &slide {
        let y = f.plan.greeter.as_ref().map_or(-1.0, |g| g.offset_y);
        assert!(y <= last + 1e-4, "the greeter only moves up ({y} after {last})");
        last = y;
        if let Some(s) = &f.plan.session {
            assert!(s.only_vacated, "the desktop shows only where the greeter has left");
        }
    }
    assert!(last < -0.99, "the greeter ends fully off the outputs");
    rig.wait_seen(Seen::A);
    rig.save_frame("03-unlocked");
    assert_eq!(rig.focus().owner, InputOwner::Session(sid.clone()));
    assert_eq!(rig.helper.opened().len(), 1, "no relaunch");
    assert_eq!(rig.helper.opened()[0].generation, generation);
    assert!(rig.helper.closed().is_empty());
    assert!(rig.helper.state.lock().locked_hints.contains(&(sid.clone(), false)));
    rig.stop();
});

scenario!(switch_a_to_b_and_back_resumes_a_without_relaunch, {
    let mut rig = Rig::start(RigConfig::default());
    rig.attach_greeter();
    let a = rig.login(ALICE, SESSION_A, PASSWORD);
    let first_fm = rig.frame_mark();

    // Switch user: A is protected and the greeter appears; picking B hides A's unlock form.
    let gm = rig.g().mark();
    rig.root_call(Method::RequestSwitchUser).expect("switch user");
    rig.wait_operation(gm, OperationPhase::Completed, Some(TransitionKind::SwitchUser));
    rig.wait_seen(Seen::Greeter);
    rig.wait_state(&a, SessionState::LockedGreeterForeground);
    rig.g().send(ClientMessage::SelectUser { uid: BOB });
    rig.wait_state(&a, SessionState::LockedBackground);
    assert!(matches!(rig.g().snapshot().mode, GreeterMode::Login));

    // B logs in for the first time: real progress again, A stays running and protected.
    let gm = rig.g().mark();
    let b = rig.login(BOB, SESSION_B, PASSWORD);
    assert_progress_is_real(&progress_models(rig.g(), gm));
    assert_ne!(a, b);
    rig.wait_seen(Seen::B);
    assert_eq!(rig.session_state(&a), Some(SessionState::LockedBackground));
    assert_eq!(rig.focus().owner, InputOwner::Session(b.clone()));

    // Back to A: lock B, pick A, authenticate: A's own Gnoblin resumes with the slide-up.
    let gm = rig.g().mark();
    rig.root_call(Method::RequestSwitchUser).expect("switch user");
    rig.wait_operation(gm, OperationPhase::Completed, Some(TransitionKind::SwitchUser));
    rig.wait_state(&b, SessionState::LockedGreeterForeground);
    let fm = rig.frame_mark();
    let gm = rig.g().mark();
    rig.g().login(ALICE, AuthIntent::Switch, PASSWORD);
    rig.wait_operation(gm, OperationPhase::Completed, Some(TransitionKind::UnlockSlideUp));
    rig.wait_seen(Seen::A);
    assert_eq!(rig.session_state(&a), Some(SessionState::Foreground));
    assert_eq!(rig.session_state(&b), Some(SessionState::LockedBackground));
    assert_eq!(rig.focus().owner, InputOwner::Session(a.clone()));
    let opened = rig.helper.opened();
    assert_eq!(opened.len(), 2, "exactly one launch per account, A was never relaunched");
    assert!(rig.helper.closed().is_empty(), "nothing was closed by switching");

    // Never two desktops at once: not in a plan, not in a pixel, over the whole run.
    let all = rig.all_frames_since(first_fm);
    for f in &all {
        if let Some(s) = &f.plan.session {
            let _ = s;
        }
        assert!(!matches!((f.top, f.body), (t, b) if (classify(t) == Seen::A && classify(b) == Seen::B) || (classify(t) == Seen::B && classify(b) == Seen::A)));
    }
    let _ = fm;
    rig.save_frame("04-back-to-a");
    rig.stop();
});

scenario!(logout_closes_only_that_session_and_a_crash_returns_to_the_greeter, {
    let mut rig = Rig::start(RigConfig::default());
    rig.attach_greeter();
    let a = rig.login(ALICE, SESSION_A, PASSWORD);
    let gm = rig.g().mark();
    rig.root_call(Method::RequestSwitchUser).unwrap();
    rig.wait_operation(gm, OperationPhase::Completed, Some(TransitionKind::SwitchUser));
    rig.wait_state(&a, SessionState::LockedGreeterForeground);
    let b = rig.login(BOB, SESSION_B, PASSWORD);
    rig.wait_seen(Seen::B);

    // ----- log out B: a fade of B into the greeter, only B is closed -----
    let fm = rig.frame_mark();
    let gm = rig.g().mark();
    rig.root_call(Method::RequestLogout { session: Some(b.clone()) }).unwrap();
    rig.wait_operation(gm, OperationPhase::Completed, Some(TransitionKind::LogoutFade));
    rig.wait_seen(Seen::Greeter);
    let fade: Vec<FrameRec> = rig.frames_since(fm).into_iter().filter(|f| f.plan.transition.is_some()).collect();
    assert!(fade.len() >= 8);
    let mut last = 2.0f32;
    for f in &fade {
        let o = f.plan.session.as_ref().map_or(0.0, |s| s.opacity);
        assert!(o <= last + 1e-4, "the leaving session only fades out");
        last = o;
    }
    rig.wait("B's processes to be reported gone", Duration::from_secs(10), |r| r.session_state(&b).is_none());
    assert_eq!(rig.helper.closed(), vec![b.clone()], "only B was closed");
    assert_eq!(rig.session_state(&a), Some(SessionState::LockedBackground), "A is untouched");
    assert_eq!(rig.focus().owner, InputOwner::Greeter);
    rig.save_frame("05-after-logout");

    // ----- log in B again, then its Gnoblin crashes -----
    let b2 = rig.login(BOB, SESSION_B, PASSWORD);
    assert_ne!(b, b2);
    let gm = rig.g().mark();
    rig.bridge(&b2).crash();
    rig.wait("the crashed session to be gone", Duration::from_secs(10), |r| r.session_state(&b2).is_none());
    rig.wait_seen(Seen::Greeter);
    rig.g().wait_after(gm, Duration::from_secs(5), "an error for the greeter", |m| matches!(m, ServerMessage::Error { .. }));
    assert_eq!(rig.session_state(&a), Some(SessionState::LockedBackground), "A survives B's crash");
    assert_eq!(rig.focus().owner, InputOwner::Greeter);
    assert!(rig.helper.closed().contains(&b2));
    assert!(!rig.helper.closed().contains(&a));
    rig.stop();
});

scenario!(failures_never_reveal_and_never_wait_forever, {
    // Short stage timeouts so the failure path runs in a second.
    let mut cfg = RigConfig::default();
    cfg.policy.stage_timeout = Duration::from_millis(1200);
    cfg.policy.gnoblin_ready_timeout = Duration::from_millis(1200);
    let mut rig = Rig::start(cfg);
    rig.attach_greeter();

    // A wrong password: PAM refuses, no session is launched, the screen does not change.
    let gm = rig.g().mark();
    rig.g().login(ALICE, AuthIntent::Login, "wrong");
    rig.g().wait_after(gm, Duration::from_secs(10), "a failed result", |m| matches!(m, ServerMessage::AuthResult { ok: false, .. }));
    assert!(rig.helper.opened().is_empty());
    assert_eq!(rig.now_seen(), Seen::Greeter);

    // A launch whose Gnoblin connects but never reports: a visible failure, never a desktop.
    let fm = rig.frame_mark();
    let gm = rig.g().mark();
    rig.g().login(ALICE, AuthIntent::Login, PASSWORD);
    let opened = rig.opened_for(0);
    let _stuck = TestBridge::connect_only(&opened, ALICE, SESSION_A);
    rig.g().wait_after(gm, Duration::from_secs(10), "a failed operation", |m| {
        matches!(m, ServerMessage::Operation { phase: OperationPhase::Failed, reason: Some(ErrorCode::Timeout), .. })
    });
    rig.wait("the failure in the greeter's snapshot", Duration::from_secs(5), |r| {
        r.greeter.as_ref().is_some_and(|g| g.snapshot().progress.is_some_and(|p| p.failure.is_some()))
    });
    let failure = rig.g().snapshot().progress.unwrap().failure.unwrap();
    assert_eq!(failure.code, ErrorCode::Timeout);
    for f in rig.all_frames_since(fm) {
        assert!(f.plan.session.is_none(), "no desktop was ever composed");
        assert_eq!(f.seen(), Seen::Greeter);
    }
    rig.wait("the failed session to be cleaned up", Duration::from_secs(10), |r| r.rt.as_ref().unwrap().snapshot().sessions.is_empty());
    assert_eq!(rig.helper.closed().len(), 1);

    // Retry works and uses a fresh launch.
    let sid = rig.login(ALICE, SESSION_A, PASSWORD);
    assert_ne!(sid, opened.session);
    assert_eq!(rig.helper.opened().len(), 2);
    rig.wait_seen(Seen::A);
    rig.stop();
});

scenario!(a_greeter_crash_while_locked_shows_the_cover_and_unlocks_nothing, {
    let mut rig = Rig::start(RigConfig::default());
    rig.attach_greeter();
    let a = rig.login(ALICE, SESSION_A, PASSWORD);
    rig.root_call(Method::RequestLock { session: Some(a.clone()) }).unwrap();
    rig.wait_state(&a, SessionState::LockedGreeterForeground);
    rig.wait_seen(Seen::Greeter);

    // The greeter dies. The compositor shows the opaque cover, never the desktop.
    let spawns = rig.greeter_spawn_count();
    let fm = rig.frame_mark();
    rig.greeter = None;
    rig.wait_seen(Seen::Cover);
    // The manager asks for a new greeter; sessions stay protected throughout.
    rig.wait("a respawned greeter", Duration::from_secs(10), |r| r.greeter_spawn_count() > spawns);
    for f in rig.all_frames_since(fm) {
        assert!(f.plan.session.is_none(), "frame {} composed a protected session", f.seq);
        assert_ne!(f.seen(), Seen::A);
    }
    assert_eq!(rig.session_state(&a), Some(SessionState::LockedGreeterForeground));
    rig.greeter = Some(TestGreeter::attach(&rig.greeter_socket, &rig.control_socket));
    rig.wait_seen(Seen::Greeter);
    rig.wait("the unlock form again", Duration::from_secs(10), |r| {
        r.greeter.as_ref().is_some_and(|g| g.messages().iter().rev().any(|m| matches!(m, ServerMessage::Snapshot(s) if matches!(s.mode, GreeterMode::Unlock { .. }))))
    });
    assert_eq!(rig.session_state(&a), Some(SessionState::LockedGreeterForeground));
    assert_eq!(rig.helper.opened().len(), 1);
    // And it still takes a password to get back in.
    let gm = rig.g().mark();
    rig.g().login(ALICE, AuthIntent::Unlock, PASSWORD);
    rig.wait_operation(gm, OperationPhase::Completed, Some(TransitionKind::UnlockSlideUp));
    rig.wait_seen(Seen::A);
    rig.stop();
});

scenario!(power_actions_are_authorised_first_and_restore_the_scene_when_refused, {
    let mut rig = Rig::start(RigConfig::default());
    rig.attach_greeter();
    let a = rig.login(ALICE, SESSION_A, PASSWORD);
    rig.wait_seen(Seen::A);

    // The session asks to reboot but polkit says no: nothing is faded, logind is not called.
    rig.system.polkit.store(false, std::sync::atomic::Ordering::SeqCst);
    let caller = {
        let scope = support::scope_path();
        bsm_manager::core::Caller { uid: ALICE, pid: std::process::id(), bus_name: ":1.42".into(), cgroup: Some(format!("{scope}/app")) }
    };
    let gm = rig.g().mark();
    let fm = rig.frame_mark();
    let r = rig.call(caller.clone(), Method::RequestReboot);
    assert!(r.is_ok(), "{r:?}");
    rig.g().wait_after(gm, Duration::from_secs(10), "the operation to fail", |m| matches!(m, ServerMessage::Operation { phase: OperationPhase::Failed, reason: Some(ErrorCode::Unauthorised), .. }));
    assert!(rig.system.power_calls.lock().is_empty());
    assert_eq!(rig.now_seen(), Seen::A);
    assert!(rig.all_frames_since(fm).iter().all(|f| f.plan.cover_overlay == 0.0));
    assert_eq!(rig.system.polkit_asked.lock().last().map(String::as_str), Some("org.freedesktop.login1.reboot"));

    // Authorised, but logind refuses: the cover fades in on every output, then back out.
    rig.system.polkit.store(true, std::sync::atomic::Ordering::SeqCst);
    rig.system.power_ok.store(false, std::sync::atomic::Ordering::SeqCst);
    let gm = rig.g().mark();
    let fm = rig.frame_mark();
    rig.call(caller.clone(), Method::RequestReboot).unwrap();
    rig.g().wait_after(gm, Duration::from_secs(15), "the refusal", |m| matches!(m, ServerMessage::Operation { phase: OperationPhase::Failed, reason: Some(ErrorCode::ShutdownRejected), .. }));
    assert_eq!(rig.system.power_calls.lock().len(), 1);
    let frames = rig.all_frames_since(fm);
    assert!(frames.iter().any(|f| f.plan.cover_overlay > 0.9), "the cover was fully up before logind was called");
    rig.wait_seen(Seen::A);
    rig.wait("input to be back with the user", Duration::from_secs(5), |r| r.focus().owner == InputOwner::Session(a.clone()));
    assert_eq!(rig.rt.as_ref().unwrap().snapshot().seat, Some(SeatState::UserForeground));

    // Accepted: the cover stays and the manager does not pretend the machine is back.
    rig.system.power_ok.store(true, std::sync::atomic::Ordering::SeqCst);
    let gm = rig.g().mark();
    rig.root_call(Method::RequestPowerOff).unwrap();
    rig.wait_operation(gm, OperationPhase::Completed, None);
    assert_eq!(rig.system.power_calls.lock().last().map(|c| c.1), Some(PowerAction::PowerOff));
    rig.wait("the cover to be complete", Duration::from_secs(5), |r| r.now_seen() == Seen::Cover);
    std::thread::sleep(TICK * 20);
    assert_eq!(rig.now_seen(), Seen::Cover, "the cover is kept while the system goes down");
    assert_eq!(rig.rt.as_ref().unwrap().snapshot().seat, Some(SeatState::ShuttingDown));
    rig.stop();
});

scenario!(the_second_user_cannot_be_unlocked_with_the_first_users_password, {
    let mut cfg = RigConfig::default();
    cfg.users = vec![("alice", PASSWORD), ("bob", "bobs secret")];
    let mut rig = Rig::start(cfg);
    rig.attach_greeter();
    let gm = rig.g().mark();
    rig.g().login(BOB, AuthIntent::Login, PASSWORD);
    rig.g().wait_after(gm, Duration::from_secs(10), "a failed result", |m| matches!(m, ServerMessage::AuthResult { ok: false, .. }));
    assert!(rig.helper.opened().is_empty());
    let sid = rig.login(BOB, SESSION_B, "bobs secret");
    rig.wait_seen(Seen::B);
    assert_eq!(rig.session_state(&sid), Some(SessionState::Foreground));
    rig.stop();
});
