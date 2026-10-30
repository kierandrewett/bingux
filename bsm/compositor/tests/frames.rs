//! Frame proofs: pixels captured from the headless compositor while real clients
//! drive it. Every test writes PNG frames and a trajectory CSV under
//! `$BSM_TEST_ARTIFACTS` (default: the system temp dir, `bsm-compositor-artifacts`).

mod common;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use bsm_compositor::{CapturedFrame, FocusTarget, Harness, HarnessConfig};
use bsm_proto::scene::{CoverColor, SceneCommand, SceneEvent, TransitionPhaseEvent};
use bsm_proto::state::{InputOwner, TransitionKind};
use bsm_proto::transition::{ease_out_cubic, ease_unlock, progress};
use common::*;

const W: u32 = 64;
const H: u32 = 48;
const SID: &str = "s00000001";
const GEN: u64 = 1;
const COVER: Rgba = [13, 26, 38, 255];
const BAND: Rgba = [200, 60, 60, 255];
const GREETER: Rgba = [40, 60, 200, 255];
const SESSION: Rgba = [30, 180, 60, 255];

fn greeter_fill() -> Fill {
    // The top quarter is a distinct band so that translation is visible, not just a clip.
    Box::new(|_, y, _, h| if y < h / 4 { BAND } else { GREETER })
}

fn greeter_px(y: u32) -> Rgba {
    if y < H / 4 { BAND } else { GREETER }
}

struct Rig {
    h: Harness,
    greeter: Option<Client>,
    host: Option<Client>,
    art: PathBuf,
    csv: String,
    next_op: u64,
}

impl Rig {
    fn new(test: &str, outputs: usize, session_fill: Fill) -> Rig {
        let art = artifacts(test);
        let mut cfg = HarnessConfig::with_outputs(outputs, W, H);
        cfg.cover = CoverColor { r: 13.0 / 255.0, g: 26.0 / 255.0, b: 38.0 / 255.0 };
        cfg.frame_dump_dir = Some(art.join("frames"));
        let mut h = Harness::start(cfg).expect("harness");
        let greeter = connect_greeter(&mut h, 1, greeter_fill());
        let host = bring_up_session(&mut h, SID, GEN, session_fill);
        h.sync();
        Rig { h, greeter: Some(greeter), host: Some(host), art, csv: String::from("test,frame,time_ms,output,t,greeter_y,session_opacity,cover_overlay,sample\n"), next_op: 1 }
    }

    fn host(&mut self) -> &mut Client {
        self.host.as_mut().expect("host connected")
    }

    fn greeter(&mut self) -> &mut Client {
        self.greeter.as_mut().expect("greeter connected")
    }

    fn key() -> (String, u64) {
        (SID.to_string(), GEN)
    }

    fn step(&mut self, ms: u64) -> Vec<CapturedFrame> {
        self.h.step(Duration::from_millis(ms))
    }

    fn begin(&mut self, kind: TransitionKind, source: bool, target: bool, ms: u32, reduce: bool) -> u64 {
        let op = self.next_op;
        self.next_op += 1;
        self.h.send(SceneCommand::BeginTransition {
            operation: op,
            kind,
            source: source.then(Self::key),
            target: target.then(Self::key),
            duration_ms: ms,
            reduce_motion: reduce,
            power: None,
        });
        op
    }

    fn phases(&mut self, op: u64) -> Vec<TransitionPhaseEvent> {
        self.h.sync();
        self.h
            .events()
            .iter()
            .filter_map(|e| match e {
                SceneEvent::TransitionPhase { operation, phase } if *operation == op => Some(*phase),
                _ => None,
            })
            .collect()
    }

    /// Greeter only on screen (greeter scene committed through the show command).
    fn show_greeter(&mut self) {
        self.h.send(SceneCommand::ShowGreeterOnly);
        let f = self.step(0);
        assert_eq!(frame_of(&f, "Virtual-1").pixel(3, 30), GREETER);
    }

    /// Make the session the committed foreground with input, as a finished new login.
    fn make_foreground(&mut self) {
        self.show_greeter();
        let op = self.begin(TransitionKind::NewLoginFade, false, true, 0, true);
        self.step(16);
        self.step(16);
        assert_eq!(self.phases(op).last(), Some(&TransitionPhaseEvent::Completed));
        self.h.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into())));
        self.step(16);
        assert_eq!(self.h.focus().owner, InputOwner::Session(SID.into()));
    }

    fn log(&mut self, test: &str, f: &CapturedFrame, t: f32, sample: Rgba) {
        let (gy, so) = (
            f.plan.greeter.map(|g| g.offset_y).unwrap_or(f32::NAN),
            f.plan.session.as_ref().map(|s| s.opacity).unwrap_or(0.0),
        );
        writeln!(
            self.csv,
            "{test},{},{},{},{t:.4},{gy:.4},{so:.4},{:.4},{:?}",
            f.seq, f.time_ms, f.output, f.plan.cover_overlay, sample
        )
        .unwrap();
    }

    fn save_csv(&self, name: &str) {
        std::fs::write(self.art.join(name), &self.csv).unwrap();
    }
}

fn lerp(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]), 255]
}

fn assert_all(f: &CapturedFrame, want: Rgba, what: &str) {
    for y in 0..f.height {
        for x in 0..f.width {
            assert!(near(f.pixel(x, y), want, 1), "{what}: pixel ({x},{y}) of {} is {:?}, wanted {want:?}", f.output, f.pixel(x, y));
        }
    }
}

#[test]
fn new_login_fades_the_desktop_in_above_a_constant_greeter_on_two_outputs() {
    // Left half transparent: whatever is visible there is what lies beneath the session.
    let fill: Fill = Box::new(|x, _, w, _| if x < w / 2 { [0, 0, 0, 0] } else { SESSION });
    let mut r = Rig::new("new_login", 2, fill);
    r.show_greeter();
    let op = r.begin(TransitionKind::NewLoginFade, false, true, 420, false);
    let mut frames = Vec::new();
    let mut t0 = None;
    for i in 0..40 {
        let f = r.step(if i == 0 { 0 } else { 30 });
        t0.get_or_insert(f[0].time_ms);
        let elapsed = f[0].time_ms - t0.unwrap();
        let t = progress(elapsed, 420, false);
        let o = ease_out_cubic(t);
        if f[0].plan.transition.is_none() {
            frames.push(f);
            break;
        }
        for out in &f {
            // Greeter beneath: constant, full opacity, no offset, whatever the session does.
            assert_eq!(out.pixel(5, 30), GREETER, "greeter pixels stay at full opacity (frame {i})");
            assert_eq!(out.pixel(5, 3), BAND);
            // Session above: opacity follows the easing exactly.
            let want = lerp(GREETER, SESSION, o);
            assert!(near(out.pixel(50, 30), want, 3), "frame {i}: {:?} vs {want:?}", out.pixel(50, 30));
            assert!(out.plan.session_above_greeter);
            let sample = out.pixel(50, 30);
            r.log("new_login", out, t, sample);
        }
        assert_eq!(f[0].rgba, f[1].rgba, "both outputs advance in lockstep (frame {i})");
        frames.push(f);
    }
    let phases = r.phases(op);
    assert_eq!(
        phases,
        vec![
            TransitionPhaseEvent::Prepared,
            TransitionPhaseEvent::Animating,
            TransitionPhaseEvent::Committed,
            TransitionPhaseEvent::Completed
        ]
    );
    // Opacity rises monotonically from 0 to 1.
    let ops: Vec<f32> = frames
        .iter()
        .filter_map(|f| f[0].plan.session.as_ref().map(|s| s.opacity))
        .collect();
    assert_eq!(ops[0], 0.0);
    assert!(ops.windows(2).all(|w| w[1] >= w[0]));
    assert!((ops.last().unwrap() - 1.0).abs() < 1e-6);
    // After the commit the greeter is no longer composed: the transparent half is the cover.
    let after = r.step(16);
    assert!(near(frame_of(&after, "Virtual-1").pixel(5, 30), COVER, 1));
    assert!(near(frame_of(&after, "Virtual-1").pixel(50, 30), SESSION, 1));
    r.save_csv("trajectory.csv");
}

#[test]
fn unlock_slides_the_greeter_up_over_only_the_authorised_session() {
    let mut r = Rig::new("unlock", 2, solid(SESSION));
    r.make_foreground();
    // Lock: protect first, then the lock scene.
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    let lock = r.begin(TransitionKind::Lock, true, false, 0, true);
    r.step(16);
    r.step(16);
    assert_eq!(r.phases(lock).last(), Some(&TransitionPhaseEvent::Completed));
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Greeter));
    let f = r.step(16);
    assert_eq!(frame_of(&f, "Virtual-1").pixel(3, 30), GREETER, "locked: greeter over the cover");
    assert!(frame_of(&f, "Virtual-1").rgba.chunks_exact(4).all(|p| p != SESSION), "no session pixel while locked");

    // Unlock.
    let op = r.begin(TransitionKind::UnlockSlideUp, false, true, 520, false);
    let mut t0 = None;
    let mut last_shift = 0;
    let mut refused_mid_flight = false;
    for i in 0..60 {
        let f = r.step(if i == 0 { 0 } else { 16 });
        if f[0].plan.transition.is_none() {
            break;
        }
        t0.get_or_insert(f[0].time_ms);
        let t = progress(f[0].time_ms - t0.unwrap(), 520, false);
        let e = ease_unlock(t);
        let shift = (e * H as f32).round() as i32;
        assert!(shift >= last_shift, "the greeter only ever moves up");
        last_shift = shift;
        for out in &f {
            assert!((out.plan.greeter.unwrap().offset_y + e).abs() < 1e-6);
            for y in 0..H as i32 {
                for x in [0u32, 17, 63] {
                    let got = out.pixel(x, y as u32);
                    let want = if y < H as i32 - shift { greeter_px((y + shift) as u32) } else { SESSION };
                    assert!(
                        near(got, want, 1),
                        "frame {i} (t={t:.3}, shift {shift}) {} ({x},{y}): {got:?} vs {want:?}",
                        out.output
                    );
                    assert!(!near(got, COVER, 2), "no empty (cover) pixel at any intermediate position");
                }
            }
            r.log("unlock", out, t, out.pixel(0, 0));
        }
        assert_eq!(f[0].rgba, f[1].rgba, "two outputs stay in sync (frame {i})");
        if i == 5 {
            // The session must not receive input before the transition committed.
            r.h.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into())));
            refused_mid_flight = true;
        }
    }
    assert!(refused_mid_flight);
    assert_eq!(r.h.focus().owner, InputOwner::Greeter, "input stays with the greeter until Committed");
    let phases = r.phases(op);
    assert_eq!(phases.last(), Some(&TransitionPhaseEvent::Completed));
    assert!(phases.contains(&TransitionPhaseEvent::Committed));
    let done = r.step(16);
    assert_all(frame_of(&done, "Virtual-1"), SESSION, "after unlock");
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into())));
    r.step(16);
    assert_eq!(r.h.focus().owner, InputOwner::Session(SID.into()));
    r.save_csv("trajectory.csv");
}

#[test]
fn reduced_motion_commits_in_one_secure_frame() {
    let mut r = Rig::new("reduced_motion", 2, solid(SESSION));
    r.make_foreground();
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    let lock = r.begin(TransitionKind::Lock, true, false, 240, true);
    let f = r.step(16);
    assert!(near(frame_of(&f, "Virtual-1").pixel(3, 30), GREETER, 1), "lock lands in its first frame");
    r.step(16);
    assert_eq!(r.phases(lock).last(), Some(&TransitionPhaseEvent::Completed));

    let op = r.begin(TransitionKind::UnlockSlideUp, false, true, 520, true);
    let f = r.step(16);
    for out in &f {
        assert_all(out, SESSION, "reduced motion unlock: the very first frame is the final scene");
    }
    let ph = r.phases(op);
    assert_eq!(ph, vec![TransitionPhaseEvent::Prepared, TransitionPhaseEvent::Animating, TransitionPhaseEvent::Committed]);
    r.step(16);
    assert_eq!(r.phases(op).last(), Some(&TransitionPhaseEvent::Completed));
}

#[test]
fn protect_removes_pixels_and_input_before_the_reply_and_input_handover_is_refused() {
    let mut r = Rig::new("protect", 1, solid(SESSION));
    r.make_foreground();
    // The session has the keyboard.
    r.h.key(30, true);
    r.h.sync();
    r.host().pump();
    r.host().roundtrip().unwrap();
    assert!(r.host().app.kb.contains(&KbEvent::Enter));
    assert!(r.host().app.kb.contains(&KbEvent::Key(30, true)));
    let f = r.step(16);
    assert_all(frame_of(&f, "Virtual-1"), SESSION, "foreground");

    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    r.h.sync();
    // Input is revoked at once, before any frame.
    assert_eq!(r.h.focus(), bsm_compositor::FocusSnapshot { owner: InputOwner::None, keyboard: None, pointer: None });
    assert!(
        !r.h.events().iter().any(|e| matches!(e, SceneEvent::Protected { .. })),
        "Protected is only sent after a frame without the session was rendered"
    );
    r.h.key(31, true);
    r.h.sync();
    r.host().roundtrip().unwrap();
    assert!(!r.host().app.kb.contains(&KbEvent::Key(31, true)), "a protected session receives no key events");
    assert!(r.host().app.kb.contains(&KbEvent::Leave));
    assert!(r.host().app.input_state.contains(&0), "the bridge is told input_state(0)");

    let f = r.step(16);
    assert_all(frame_of(&f, "Virtual-1"), COVER, "after protect: opaque cover, never the desktop");
    assert!(r.h.wait_event(WAIT, |e| matches!(e, SceneEvent::Protected { session, generation: 1 } if session == SID)).is_some());

    // Handing input to a protected session is refused: the owner reported is the real one.
    r.h.take_events();
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into())));
    r.step(16);
    assert!(r.h.events().iter().any(|e| matches!(e, SceneEvent::InputOwnerChanged(InputOwner::None))));
    assert_eq!(r.h.focus().owner, InputOwner::None);
}

#[test]
fn failed_transition_returns_to_cover_plus_greeter_and_hides_the_target() {
    let mut r = Rig::new("failed_transition", 2, solid(SESSION));
    r.show_greeter();
    let op = r.begin(TransitionKind::NewLoginFade, false, true, 420, false);
    let f = r.step(0);
    assert_eq!(frame_of(&f, "Virtual-1").pixel(3, 30), GREETER);
    let f = r.step(200);
    assert!(!near(frame_of(&f, "Virtual-1").pixel(3, 30), GREETER, 5), "mid fade the session is partly visible");
    // The bridge dies in the middle of the fade.
    r.host = None;
    r.h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionGone { .. })).expect("SessionGone");
    let f = r.step(16);
    for out in &f {
        assert!(out.plan.session.is_none());
        assert_eq!(out.pixel(3, 30), GREETER, "back to the greeter scene");
        assert_eq!(out.pixel(3, 3), BAND);
        assert!(out.rgba.chunks_exact(4).all(|p| p != SESSION && !near([p[0], p[1], p[2], p[3]], SESSION, 40) || p == [40, 60, 200, 255]), "no session pixel");
    }
    assert_eq!(r.phases(op).last(), Some(&TransitionPhaseEvent::Failed));
    let f = r.step(16);
    assert_eq!(frame_of(&f, "Virtual-1").pixel(3, 30), GREETER, "and it stays there");
    // The session cannot be revealed again: it has no host any more.
    let op2 = r.begin(TransitionKind::NewLoginFade, false, true, 0, true);
    r.step(16);
    assert_eq!(r.phases(op2).last(), Some(&TransitionPhaseEvent::Failed));
}

#[test]
fn cancel_before_commit_restores_the_previous_scene() {
    let mut r = Rig::new("cancel", 1, solid(SESSION));
    r.make_foreground();
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    let lock = r.begin(TransitionKind::Lock, true, false, 0, true);
    r.step(16);
    r.step(16);
    assert_eq!(r.phases(lock).last(), Some(&TransitionPhaseEvent::Completed));
    let op = r.begin(TransitionKind::UnlockSlideUp, false, true, 520, false);
    r.step(0);
    let f = r.step(100);
    assert!(!near(frame_of(&f, "Virtual-1").pixel(3, 3), BAND, 1), "the greeter moved");
    r.h.send(SceneCommand::CancelTransition { operation: op });
    let f = r.step(16);
    let out = frame_of(&f, "Virtual-1");
    assert_eq!(out.pixel(3, 3), BAND, "greeter back in place");
    assert_eq!(out.pixel(3, 30), GREETER);
    assert!(out.rgba.chunks_exact(4).all(|p| p != SESSION), "the session is hidden again");
    assert_eq!(r.phases(op).last(), Some(&TransitionPhaseEvent::Cancelled));
}

#[test]
fn greeter_crash_while_protected_shows_the_opaque_cover_until_a_greeter_returns() {
    let mut r = Rig::new("greeter_crash", 2, solid(SESSION));
    r.make_foreground();
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    let lock = r.begin(TransitionKind::Lock, true, false, 0, true);
    r.step(16);
    r.step(16);
    assert_eq!(r.phases(lock).last(), Some(&TransitionPhaseEvent::Completed));
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Greeter));
    r.step(16);
    // The greeter dies.
    r.greeter = None;
    r.h.wait_event(WAIT, |e| matches!(e, SceneEvent::GreeterGone { generation: 1 })).expect("GreeterGone");
    let f = r.step(16);
    for out in &f {
        assert_all(out, COVER, "greeter crashed: opaque cover, no desktop, no stale greeter");
    }
    // A new greeter connects with a new generation: the lock scene returns.
    let _g = connect_greeter(&mut r.h, 2, greeter_fill());
    let f = r.step(16);
    assert_eq!(frame_of(&f, "Virtual-1").pixel(3, 30), GREETER);
    assert!(f.iter().all(|o| o.rgba.chunks_exact(4).all(|p| p != SESSION)));
}

#[test]
fn output_changes_never_expose_a_protected_session() {
    let mut r = Rig::new("output_change", 2, solid(SESSION));
    r.make_foreground();
    let f = r.step(16);
    assert_all(frame_of(&f, "Virtual-2"), SESSION, "both outputs show the desktop");
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    let lock = r.begin(TransitionKind::Lock, true, false, 0, true);
    r.step(16);
    r.step(16);
    assert_eq!(r.phases(lock).last(), Some(&TransitionPhaseEvent::Completed));
    // Unplug the second monitor, plug in a third, reshape the first, all while locked.
    let mut outs = r.h.outputs.clone();
    outs.remove(1);
    outs.push(bsm_proto::scene::OutputInfo { name: "Virtual-3".into(), x: 128, y: 0, width: 80, height: 60, scale_120: 120, refresh_mhz: 60_000, transform: 0 });
    outs[0].width = 72;
    r.h.set_outputs(outs);
    for _ in 0..3 {
        let f = r.step(16);
        for out in &f {
            assert!(out.rgba.chunks_exact(4).all(|p| p != SESSION), "no protected pixel on {}", out.output);
        }
    }
    // The bridge learned about it.
    r.host().roundtrip().unwrap();
    let names: Vec<_> = r.host().app.host_outputs.iter().map(|o| o.0.clone()).collect();
    assert_eq!(names, vec!["Virtual-1".to_string(), "Virtual-3".to_string()]);
    assert_eq!(r.host().app.removed_outputs, vec!["Virtual-2".to_string()]);
}

#[test]
fn a_foreground_session_with_an_unproven_new_output_is_not_composed() {
    let mut r = Rig::new("hotplug_foreground", 1, solid(SESSION));
    r.make_foreground();
    let f = r.step(16);
    assert_all(frame_of(&f, "Virtual-1"), SESSION, "foreground");
    let mut outs = r.h.outputs.clone();
    outs.push(bsm_proto::scene::OutputInfo { name: "Virtual-2".into(), x: 64, y: 0, width: W, height: H, scale_120: 120, refresh_mhz: 60_000, transform: 0 });
    r.h.set_outputs(outs);
    let f = r.step(16);
    for out in &f {
        assert_all(out, COVER, "a session with an output that has no frame yet is never composed (no stale pixels)");
    }
    // The bridge creates the surface for the new output, acks and paints it.
    r.host().roundtrip().unwrap();
    r.host().host_add_surface("Virtual-2");
    r.host().host_ack_only(1);
    let f = r.step(16);
    assert_all(frame_of(&f, "Virtual-1"), COVER, "acked but no frame yet: still not composed");
    // Paint the new surface (and repaint the old one so its evidence is fresh too).
    r.host().host_paint(&solid(SESSION));
    let f = r.step(16);
    assert_all(frame_of(&f, "Virtual-1"), SESSION, "composed once every output has a frame");
    assert_all(frame_of(&f, "Virtual-2"), SESSION, "composed once every output has a frame");
}

#[test]
fn input_goes_only_to_the_owner() {
    let mut r = Rig::new("input_owner", 1, solid(SESSION));
    r.show_greeter();
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Greeter));
    r.step(16);
    r.h.key(30, true);
    r.h.sync();
    r.greeter().roundtrip().unwrap();
    r.host().roundtrip().unwrap();
    assert!(r.greeter().app.kb.contains(&KbEvent::Key(30, true)), "greeter owns the keyboard");
    assert!(r.host().app.kb.is_empty(), "the hosted session saw nothing: {:?}", r.host().app.kb);
    assert_eq!(r.h.focus().keyboard, Some(FocusTarget::Greeter));
    r.h.pointer_to(10.0, 10.0);
    r.h.sync();
    r.greeter().roundtrip().unwrap();
    assert!(r.greeter().app.ptr.contains(&PtrEvent::Enter));
    assert!(r.host().app.ptr.is_empty());

    // A session cannot take input while it is not on screen, even if asked.
    r.h.take_events();
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into())));
    r.step(16);
    assert_eq!(r.h.focus().owner, InputOwner::Greeter);

    // After a committed reveal, it moves, and the greeter gets a leave.
    let op = r.begin(TransitionKind::NewLoginFade, false, true, 0, true);
    r.step(16);
    r.step(16);
    assert_eq!(r.phases(op).last(), Some(&TransitionPhaseEvent::Completed));
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into())));
    r.step(16);
    r.h.key(31, true);
    r.h.pointer_to(12.0, 12.0);
    r.h.pointer_button(0x110, true);
    r.h.sync();
    r.greeter().roundtrip().unwrap();
    r.host().roundtrip().unwrap();
    assert!(r.host().app.kb.contains(&KbEvent::Key(31, true)));
    assert!(r.host().app.ptr.contains(&PtrEvent::Button(0x110, true)));
    assert!(!r.greeter().app.kb.contains(&KbEvent::Key(31, true)), "the greeter no longer sees keys");
    assert!(r.greeter().app.kb.contains(&KbEvent::Leave));
    assert!(r.host().app.input_state.contains(&1));
    assert_eq!(r.h.focus().keyboard, Some(FocusTarget::Session(SID.into())));
}

#[test]
fn power_fade_holds_the_cover_and_a_refused_power_action_fades_back() {
    let mut r = Rig::new("power_fade", 2, solid(SESSION));
    r.make_foreground();
    let op = r.begin(TransitionKind::PowerFade, true, false, 360, false);
    let mut t0 = None;
    for i in 0..40 {
        let f = r.step(if i == 0 { 0 } else { 30 });
        if f[0].plan.transition.is_none() {
            break;
        }
        t0.get_or_insert(f[0].time_ms);
        let t = progress(f[0].time_ms - t0.unwrap(), 360, false);
        let e = bsm_proto::transition::ease_in_out_cubic(t);
        let want = lerp(SESSION, COVER, e);
        for out in &f {
            assert!(near(out.pixel(30, 20), want, 3), "frame {i}: {:?} vs {want:?}", out.pixel(30, 20));
        }
    }
    let f = r.step(16);
    for out in &f {
        assert_all(out, COVER, "committed power fade: the cover on every output");
    }
    assert!(r.phases(op).contains(&TransitionPhaseEvent::Committed));
    // Held: nothing changes however long we wait.
    let f = r.step(5000);
    assert_all(frame_of(&f, "Virtual-2"), COVER, "the cover is kept");

    // logind refused: the cover fades back out onto the scene authorised before.
    r.h.send(SceneCommand::CancelTransition { operation: op });
    let mut prev = 1.0f32;
    let mut last = None;
    for i in 0..40 {
        let f = r.step(if i == 0 { 0 } else { 30 });
        if f[0].plan.transition.is_none() {
            last = Some(f);
            break;
        }
        let a = f[0].plan.cover_overlay;
        assert!(a <= prev + 1e-6, "the cover only recedes");
        prev = a;
    }
    let last = last.expect("fade back finished");
    assert_all(frame_of(&last, "Virtual-1"), SESSION, "the foreground desktop is back");
    assert_eq!(r.phases(op).last(), Some(&TransitionPhaseEvent::Cancelled));
}

#[test]
fn greeter_keyboard_works_whatever_order_owner_and_connection_arrive_in() {
    // The manager may name the greeter as owner before the greeter process connects, and
    // Quickshell asks for on-demand rather than exclusive keyboard interactivity.
    for mode in [
        wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand,
        wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
    ] {
        let mut h = Harness::start(HarnessConfig::with_outputs(1, W, H)).unwrap();
        h.send(SceneCommand::ShowGreeterOnly);
        h.send(SceneCommand::SetInputOwner(InputOwner::Greeter));
        h.step(Duration::from_millis(16));
        h.send(SceneCommand::RegisterGreeter { generation: 1, uid: my_uid(), pid: std::process::id() });
        let mut c = Client::connect(&h.greeter_socket).unwrap();
        c.kb_mode = mode;
        h.wait_event(WAIT, |e| matches!(e, SceneEvent::GreeterConnected { .. })).unwrap();
        c.greeter_map(greeter_fill());
        h.step(Duration::from_millis(16));
        assert_eq!(h.focus().keyboard, Some(FocusTarget::Greeter), "{mode:?}: keyboard focus on the greeter");
        h.key(30, true);
        h.key(30, false);
        h.sync();
        c.roundtrip().unwrap();
        assert!(c.app.kb.contains(&KbEvent::Enter), "{mode:?}");
        assert!(c.app.kb.contains(&KbEvent::Key(30, true)), "{mode:?}: key reached the greeter: {:?}", c.app.kb);
    }
}

// ---- review F1: acknowledgement is tied to a frame composed after the change ----

fn protected_events(r: &mut Rig) -> usize {
    r.h.sync();
    r.h.events().iter().filter(|e| matches!(e, SceneEvent::Protected { .. })).count()
}

#[test]
fn f1a_a_flip_queued_before_protect_does_not_acknowledge_it() {
    let mut r = Rig::new("f1a", 2, solid(SESSION));
    r.make_foreground();
    r.h.take_events();
    r.h.fake_flips(true);
    let n = r.step(16); // frame N, with the session, in flight
    assert_all(frame_of(&n, "Virtual-1"), SESSION, "frame N shows the session");
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    r.h.sync();
    r.h.complete_flip(); // the flip of frame N lands: the screen shows the session
    assert_eq!(protected_events(&mut r), 0, "frame N proves nothing about the protect");
    assert!(r.h.needs_redraw(), "the frame that does prove it is still owed (review F1b)");
    let cover = r.step(16);
    for out in &cover {
        assert_all(out, COVER, "the frame composed after the protect");
    }
    assert_eq!(protected_events(&mut r), 0, "composed is not shown yet");
    r.h.complete_flip();
    assert_eq!(protected_events(&mut r), 1, "acknowledged by the flip of the cover frame");
}

#[test]
fn f1c_a_lost_flip_acknowledges_nothing_and_is_retried() {
    let mut r = Rig::new("f1c", 1, solid(SESSION));
    r.make_foreground();
    r.h.take_events();
    r.h.fake_flips(true);
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    r.step(16);
    r.h.expire_flip(); // the 500 ms watchdog
    assert_eq!(protected_events(&mut r), 0, "a flip that never happened is not an acknowledgement");
    assert!(r.h.needs_redraw());
    r.step(16);
    r.h.complete_flip();
    assert_eq!(protected_events(&mut r), 1);
}

#[test]
fn f1d_render_errors_and_partial_failures_are_not_success() {
    let mut r = Rig::new("f1d", 2, solid(SESSION));
    r.make_foreground();
    r.h.take_events();
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    r.h.fail_renders(2);
    r.step(16);
    r.step(16);
    assert_eq!(protected_events(&mut r), 0, "failed renders acknowledge nothing");
    r.step(16);
    assert_eq!(protected_events(&mut r), 1, "the retry that reached the screen does");

    // One output fails while the other flips.
    let mut r = Rig::new("f1d_partial", 2, solid(SESSION));
    r.make_foreground();
    r.h.take_events();
    r.h.fake_flips(true);
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    r.h.partial_failure();
    r.step(16);
    r.h.complete_flip();
    assert_eq!(protected_events(&mut r), 0, "output B still shows the session");
    r.step(16);
    r.h.complete_flip();
    assert_eq!(protected_events(&mut r), 1);
}

#[test]
fn f1d_a_transition_that_cannot_be_shown_is_given_up_with_failed() {
    let mut r = Rig::new("f1d_abort", 1, solid(SESSION));
    r.show_greeter();
    r.h.take_events();
    let op = r.begin(TransitionKind::NewLoginFade, false, true, 420, false);
    r.h.fail_renders(5);
    for _ in 0..5 {
        r.step(16);
    }
    assert_eq!(r.phases(op).last(), Some(&TransitionPhaseEvent::Failed));
    assert!(!r.phases(op).contains(&TransitionPhaseEvent::Committed), "nothing was committed on failed frames");
}

#[test]
fn f1e_dpms_off_leaves_the_cover_on_the_scanout_and_protect_while_off_is_safe() {
    let mut r = Rig::new("f1e", 1, solid(SESSION));
    r.make_foreground();
    r.h.take_events();
    r.h.send(SceneCommand::SetDpms { on: false });
    let blank = r.step(16);
    assert_all(frame_of(&blank, "Virtual-1"), COVER, "the scanout buffer holds the cover when the panel is switched off");
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    let f = r.step(16);
    assert_all(frame_of(&f, "Virtual-1"), COVER, "still the cover");
    assert_eq!(protected_events(&mut r), 1, "nothing but the cover can be on the scanout buffer");
    // Wake: the first frame is the scene, which no longer contains the protected session.
    r.h.send(SceneCommand::SetDpms { on: true });
    let f = r.step(16);
    assert_all(frame_of(&f, "Virtual-1"), COVER, "waking never shows the old desktop");
}

// ---- review F2, F5, F6, F12: input follows the scene and nothing leaks across a hand-off ----

#[test]
fn f2_a_pointer_grab_does_not_survive_protect() {
    let mut r = Rig::new("f2", 1, solid(SESSION));
    r.make_foreground();
    r.h.pointer_to(10.0, 10.0);
    r.h.pointer_button(0x110, true); // implicit grab in the session
    r.h.sync();
    r.host().roundtrip().unwrap();
    assert!(r.host().app.ptr.contains(&PtrEvent::Button(0x110, true)));
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    r.h.sync();
    r.host().roundtrip().unwrap();
    let before = r.host().app.ptr.len();
    r.h.pointer_to(20.0, 20.0);
    r.h.pointer_to(30.0, 25.0);
    r.h.pointer_button(0x110, false);
    r.h.sync();
    r.host().roundtrip().unwrap();
    let after: Vec<_> = r.host().app.ptr[before..].to_vec();
    assert!(!after.contains(&PtrEvent::Motion), "no motion after protect: {after:?}");
    assert!(!after.contains(&PtrEvent::Button(0x110, false)), "the physical release is swallowed: {after:?}");
    let releases = r.host().app.ptr.iter().filter(|e| **e == PtrEvent::Button(0x110, false)).count();
    assert_eq!(releases, 1, "the session got exactly one release, at the hand-off");
    assert!(r.host().app.ptr.contains(&PtrEvent::Leave));
}

#[test]
fn f12_keys_held_at_hand_off_are_released_not_listed_in_enter() {
    let mut r = Rig::new("f12", 1, solid(SESSION));
    r.show_greeter();
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Greeter));
    r.step(16);
    r.h.key(28, true); // Return, still down when the desktop is revealed
    r.h.sync();
    let op = r.begin(TransitionKind::NewLoginFade, false, true, 0, true);
    r.step(16);
    r.step(16);
    assert_eq!(r.phases(op).last(), Some(&TransitionPhaseEvent::Completed));
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into())));
    r.step(16);
    r.h.key(28, false); // the physical release arrives after the hand-off
    r.h.key(30, true);
    r.h.sync();
    r.greeter().roundtrip().unwrap();
    r.host().roundtrip().unwrap();
    assert!(r.greeter().app.kb.contains(&KbEvent::Key(28, false)), "the greeter got the release of its key");
    let enters = r.host().app.enter_keys.clone();
    assert_eq!(enters.len(), 1);
    assert!(enters[0].is_empty(), "no held key in the new owner's enter: {enters:?}");
    assert!(!r.host().app.kb.contains(&KbEvent::Key(28, false)), "a release without a press is swallowed");
    assert!(r.host().app.kb.contains(&KbEvent::Key(30, true)));
}

#[test]
fn f5_a_lock_revokes_input_at_once_not_a_frame_later() {
    let mut r = Rig::new("f5", 1, solid(SESSION));
    r.make_foreground();
    r.h.fake_flips(true);
    r.step(16); // a frame is in flight
    // Lock without a separate Protect: the engine protects, input must follow immediately.
    let _ = r.begin(TransitionKind::Lock, true, false, 240, false);
    r.h.sync();
    assert_eq!(r.h.focus().owner, InputOwner::None);
    assert_eq!(r.h.focus().keyboard, None);
}

#[test]
fn f6_the_greeter_gets_input_only_when_it_is_what_is_shown() {
    let mut r = Rig::new("f6", 1, solid(SESSION));
    r.make_foreground();
    r.h.take_events();
    // The desktop is the foreground: a (replayed or buggy) greeter hand-over is refused.
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Greeter));
    r.step(16);
    assert_eq!(r.h.focus().owner, InputOwner::Session(SID.into()));
    r.h.key(30, true);
    r.h.sync();
    r.greeter().roundtrip().unwrap();
    assert!(r.greeter().app.kb.is_empty(), "the greeter saw no keystroke of the user's desktop");

    // A request that only came early is granted once the scene is the greeter's.
    r.h.send(SceneCommand::Protect { session: SID.into(), generation: GEN });
    r.h.send(SceneCommand::SetInputOwner(InputOwner::Greeter));
    r.h.sync();
    assert_eq!(r.h.focus().owner, InputOwner::None, "the cover is shown, nobody types");
    r.h.send(SceneCommand::ShowGreeterOnly);
    r.h.sync();
    assert_eq!(r.h.focus().owner, InputOwner::Greeter, "kept request granted when the greeter is shown");
}

#[test]
fn f6_a_session_with_an_unproven_output_loses_input_and_gets_it_back_when_proven() {
    let mut r = Rig::new("f6_unproven", 1, solid(SESSION));
    r.make_foreground();
    let mut outs = r.h.outputs.clone();
    outs.push(bsm_proto::scene::OutputInfo { name: "Virtual-2".into(), x: 64, y: 0, width: W, height: H, scale_120: 120, refresh_mhz: 60_000, transform: 0 });
    r.h.set_outputs(outs);
    r.step(16);
    assert_eq!(r.h.focus().owner, InputOwner::None, "the cover is shown: no typing into an unseen desktop");
    r.host().roundtrip().unwrap();
    r.host().host_add_surface("Virtual-2");
    r.host().host_paint(&solid(SESSION));
    r.step(16);
    assert_eq!(r.h.focus().owner, InputOwner::Session(SID.into()), "restored when the desktop is on screen again");
}
