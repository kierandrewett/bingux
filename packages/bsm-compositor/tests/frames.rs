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
