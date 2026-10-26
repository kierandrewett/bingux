//! Cursor, fractional scale, buffer release and presentation behaviour.

mod common;

use std::time::Duration;

use bsm_compositor::{Harness, HarnessConfig};
use bsm_proto::scene::{OutputInfo, SceneCommand};
use bsm_proto::state::{InputOwner, TransitionKind};
use common::*;

const GREETER: Rgba = [40, 60, 200, 255];
const BAND: Rgba = [200, 60, 60, 255];
const SESSION: Rgba = [30, 180, 60, 255];
const SID: &str = "s00000001";

fn band_fill() -> Fill {
    Box::new(|_, y, _, h| if y < h / 4 { BAND } else { GREETER })
}

fn reveal(h: &mut Harness) {
    h.send(SceneCommand::BeginTransition {
        operation: 1,
        kind: TransitionKind::NewLoginFade,
        source: None,
        target: Some((SID.into(), 1)),
        duration_ms: 0,
        reduce_motion: true,
        power: None,
    });
    h.step(Duration::from_millis(16));
    h.step(Duration::from_millis(16));
}

#[test]
fn the_cursor_is_drawn_for_the_owner_only() {
    let mut cfg = HarnessConfig::with_outputs(1, 64, 48);
    cfg.draw_cursor = true;
    let mut h = Harness::start(cfg).unwrap();
    let _g = connect_greeter(&mut h, 1, solid(GREETER));
    h.send(SceneCommand::ShowGreeterOnly);
    h.send(SceneCommand::SetInputOwner(InputOwner::Greeter));
    h.step(Duration::from_millis(16));
    h.pointer_to(20.0, 20.0);
    h.sync();
    let f = h.step(Duration::from_millis(16));
    let f = &f[0];
    assert_eq!(f.pixel(20, 20), [0, 0, 0, 255], "arrow tip outline at the hotspot");
    assert_eq!(f.pixel(21, 24), [255, 255, 255, 255], "arrow body");
    assert_eq!(f.pixel(40, 40), GREETER, "the rest is untouched");
    // Nobody owns the seat: no cursor, since nobody could be clicked.
    h.send(SceneCommand::SetInputOwner(InputOwner::None));
    let f = h.step(Duration::from_millis(16));
    assert_eq!(f[0].pixel(20, 20), GREETER);
}

#[test]
fn fractional_scale_output_keeps_sessions_one_to_one_and_scales_the_greeter() {
    let mut cfg = HarnessConfig::with_outputs(1, 96, 72);
    cfg.outputs[0].scale_120 = 180; // 1.5: logical 64x48
    let mut h = Harness::start(cfg).unwrap();
    let _g = connect_greeter(&mut h, 1, band_fill());
    let _s = bring_up_session(&mut h, SID, 1, solid(SESSION));
    h.send(SceneCommand::ShowGreeterOnly);
    let f = h.step(Duration::from_millis(16));
    let f = &f[0];
    // Greeter buffer is 64x48 logical; band is the top 12 logical rows = 18 physical rows.
    assert_eq!(f.pixel(5, 5), BAND);
    assert_eq!(f.pixel(5, 30), GREETER);
    assert_eq!(f.pixel(90, 70), GREETER, "the greeter fills the whole 96x72 output");
    reveal(&mut h);
    let f = h.step(Duration::from_millis(16));
    for y in 0..72 {
        for x in 0..96 {
            assert!(near(f[0].pixel(x, y), SESSION, 1), "session is composed 1:1 at ({x},{y}): {:?}", f[0].pixel(x, y));
        }
    }
}

#[test]
fn shm_buffers_are_released_when_replaced() {
    let mut h = Harness::start(HarnessConfig::with_outputs(1, 64, 48)).unwrap();
    let _g = connect_greeter(&mut h, 1, solid(GREETER));
    let mut s = bring_up_session(&mut h, SID, 1, solid(SESSION));
    h.send(SceneCommand::ShowGreeterOnly);
    h.step(Duration::from_millis(16));
    reveal(&mut h);
    for _ in 0..4 {
        s.host_paint(&solid(SESSION));
        h.step(Duration::from_millis(16));
    }
    s.roundtrip().unwrap();
    assert!(s.app.releases >= 3, "the bridge needs its slots back: {} releases", s.app.releases);
}

#[test]
fn frame_callbacks_and_presented_events_reach_a_visible_session() {
    let mut h = Harness::start(HarnessConfig::with_outputs(1, 64, 48)).unwrap();
    let _g = connect_greeter(&mut h, 1, solid(GREETER));
    let mut s = bring_up_session(&mut h, SID, 1, solid(SESSION));
    h.send(SceneCommand::ShowGreeterOnly);
    h.step(Duration::from_millis(16));
    reveal(&mut h);
    s.roundtrip().unwrap();
    let before = s.app.presented.len();
    s.host_paint(&solid([31, 181, 61, 255]));
    h.step(Duration::from_millis(16));
    s.roundtrip().unwrap();
    assert!(s.app.presented.len() > before, "a visible commit is reported as presented");
    let seqs: Vec<u32> = s.app.presented.iter().map(|p| p.1).collect();
    assert!(seqs.windows(2).all(|w| w[1] > w[0]), "sequence numbers increase: {seqs:?}");
    let _ = OutputInfo { name: String::new(), x: 0, y: 0, width: 0, height: 0, scale_120: 120, refresh_mhz: 0, transform: 0 };
}
