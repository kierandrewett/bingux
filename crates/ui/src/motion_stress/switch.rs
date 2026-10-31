//! The switch, frame by frame: the thumb travels along its track and nothing else moves.
//!
//! Every test runs at scale 1.0 and 2.0 in the light and the dark scheme, on a desktop window
//! (the 36 x 20 switch) and on a phone (the 48 x 28 one). A travel is [`TRAVEL_MS`] long.

use freya::prelude::*;

use super::*;
use crate::contrast::ratio_opaque;
use crate::group::Group;
use crate::rows::{row, switch_row};
use crate::style::*;
use crate::surface::use_tokens;

const DESKTOP: (f32, f32) = (900.0, 420.0);
const PHONE: (f32, f32) = (390.0, 800.0);
const WINDOWS: [(f32, f32); 2] = [DESKTOP, PHONE];
/// The switch's thumb travel, from `controls/switch.rs`.
const TRAVEL_MS: u64 = 100;
/// The thumb's inset from the track end, from `controls/switch.rs`.
const INSET: f32 = 2.0;
/// The most frames a thumb may take to start moving after the input: the animation starts the
/// frame after the state changes, and its first tick counts for nothing.
const LATENCY: usize = 5;
/// A position tolerance in logical pixels.
const EPS: f32 = 0.01;

/// A card of three rows, the middle one a switch. The key `x` flips it from outside the control,
/// as a keyboard shortcut or another control would.
#[derive(Clone, PartialEq)]
pub(super) struct Scene {
    pub start_on: bool,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let start = self.start_on;
        let mut on = use_state(move || start);
        WindowHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg), p(S4)])
                .on_global_key_down(move |event: Event<KeyboardEventData>| {
                    if event.key == Key::Character("x".into()) {
                        let next = !*on.peek();
                        on.set(next);
                    }
                })
                .child(Group::titled("Switch").rows(vec![
                    row("Before").value("A value").into(),
                    switch_row("Wi-Fi", on(), move |next| on.set(next)).into(),
                    row("After").value("Another").into(),
                ])),
        )
    }
}

/// The track's and the thumb's left edge and the thumb's width, in a frame.
struct Look {
    track: Area,
    thumb: Area,
    track_fill: Color,
    thumb_fill: Color,
}

fn look(frame: &Frame) -> Look {
    let track = frame.nth("Switch", 0);
    let thumb = frame.nth("SwitchThumb", 0);
    Look {
        track: track.area.expect("the track has an area"),
        thumb: thumb.area.expect("the thumb has an area"),
        track_fill: Frame::fill(track).expect("the track paints a fill"),
        thumb_fill: Frame::fill(thumb).expect("the thumb paints a fill"),
    }
}

/// Where the thumb's left edge rests, off and on, for a track at `track` and a thumb `width` wide.
fn ends(track: Area, width: f32) -> (f32, f32) {
    (track.min_x() + INSET, track.max_x() - INSET - width)
}

fn thumb_xs(frames: &[Frame]) -> Vec<f32> {
    frames
        .iter()
        .map(|frame| look(frame).thumb.min_x())
        .collect()
}

/// What must hold on every frame of every motion of the switch, whatever input made it.
fn every_frame(frames: &[Frame], findings: &mut Findings) {
    let first = look(&frames[0]);
    let (off, on) = ends(first.track, first.thumb.width());
    let thumb_rest = first.thumb.width();
    for (at, frame) in frames.iter().enumerate() {
        let now = look(frame);
        // The track is a box the motion does not own.
        findings.check(now.track == first.track, || {
            format!(
                "frame {at}: the track moved or resized: {:?} -> {:?}",
                first.track, now.track
            )
        });
        // The thumb stays inside the track, whole.
        findings.check(now.thumb.min_x() >= now.track.min_x() + INSET - EPS, || {
            format!(
                "frame {at}: the thumb left the track at the start: {:?} in {:?}",
                now.thumb, now.track
            )
        });
        findings.check(now.thumb.max_x() <= now.track.max_x() - INSET + EPS, || {
            format!(
                "frame {at}: the thumb left the track at the end: {:?} in {:?}",
                now.thumb, now.track
            )
        });
        // One centre line: the thumb is centred on the track vertically.
        let (a, b) = (
            now.thumb.min_y() + now.thumb.height() / 2.0,
            now.track.min_y() + now.track.height() / 2.0,
        );
        findings.check((a - b).abs() <= 0.5, || {
            format!("frame {at}: the thumb's centre is {a:.2}, the track's {b:.2}")
        });
        // The thumb is never more than the pressed width and never narrower than at rest.
        findings.check(
            now.thumb.width() >= thumb_rest - EPS && now.thumb.width() <= thumb_rest + 2.0 + EPS,
            || {
                format!(
                    "frame {at}: the thumb is {:.2} wide (rest {thumb_rest:.2})",
                    now.thumb.width()
                )
            },
        );
        // No overshoot: the left edge stays between where it rests off and where it rests on.
        let x = now.thumb.min_x();
        findings.check(x >= off - EPS && x <= on + EPS, || {
            format!("frame {at}: the thumb overshoots, x {x:.2} outside {off:.2}..{on:.2}")
        });
    }
    // Nothing else of the window moves: every box but the thumb stays where it was.
    findings.extend(
        unmoved(frames, EPS, |path| path.ends_with("SwitchThumb#0"))
            .into_iter()
            .map(Err),
    );
    // And no text changes whether it is cut.
    findings.extend(elision_flips(frames, 0).into_iter().map(Err));
}

fn toggle_by_click(rig: &mut Rig) {
    rig.click("Switch");
}

#[test]
fn a_toggle_moves_the_thumb_along_the_track_and_nothing_else() {
    Env::matrix(&WINDOWS, &Env::ALL, |env, window, findings| {
        for start_on in [false, true] {
            {
                let mut rig = Rig::new(env, window, Scene { start_on });
                let (width, track) = (look(rig.now()).thumb.width(), look(rig.now()).track);
                let (off, on) = ends(track, width);
                let (from, to) = if start_on { (on, off) } else { (off, on) };
                toggle_by_click(&mut rig);
                rig.steps(24);
                every_frame(&rig.frames, findings);
                let xs = thumb_xs(&rig.frames);
                findings.extend([monotone_between_ends("thumb x", &xs, EPS)]);
                findings.extend([ends_at("thumb x", &xs, to, EPS)]);
                findings.extend([no_jump("thumb x", &xs, (on - off) * 0.55)]);
                // It starts from where it rests, not from a jump.
                findings.check((xs[0] - from).abs() <= EPS, || {
                    format!("the thumb began at {:.2}, not {from:.2}", xs[0])
                });
                // It starts soon, and takes the specified time at 16 ms a frame.
                let start = start_index(&xs, EPS);
                findings.check(start <= LATENCY, || {
                    format!("the thumb began to move on frame {start}, after {LATENCY}")
                });
                let rest = rest_index(&xs, EPS);
                let frames = rest + 1 - start;
                let wanted = TRAVEL_MS.div_ceil(STEP_MS) as usize;
                findings.check((wanted - 1..=wanted + 1).contains(&frames), || {
                    format!(
                        "the travel took {frames} frames ({} ms), {TRAVEL_MS} ms is {wanted}: {}",
                        frames as u64 * STEP_MS,
                        show(&xs)
                    )
                });
                // The colours go from the start to the end along one path.
                let tracks: Vec<Color> = rig.frames.iter().map(|f| look(f).track_fill).collect();
                findings.extend([colour_path("track colour", &tracks, 1.0)]);
                let thumbs: Vec<Color> = rig.frames.iter().map(|f| look(f).thumb_fill).collect();
                findings.extend([colour_path("thumb colour", &thumbs, 1.0)]);
            }
        }
    });
}

#[test]
fn spamming_the_switch_never_overshoots_and_ends_on_the_last_state() {
    Env::matrix(&WINDOWS, &Env::ALL, |env, window, findings| {
        for gap in [0usize, 1, 2, 3, 5] {
            let mut rig = Rig::new(env, window, Scene { start_on: false });
            let (width, track) = (look(rig.now()).thumb.width(), look(rig.now()).track);
            let (off, on) = ends(track, width);
            let clicks = 9;
            for _ in 0..clicks {
                toggle_by_click(&mut rig);
                rig.steps(gap);
            }
            rig.steps(30);
            every_frame(&rig.frames, findings);
            let xs = thumb_xs(&rig.frames);
            // Nine flips from off is on. The thumb ends there, exactly.
            findings.extend([ends_at(
                &format!("thumb x after {clicks} clicks, {gap} frames apart"),
                &xs,
                on,
                EPS,
            )]);
            // A reversal continues from where the thumb is: no frame moves it by more than a
            // single frame of the travel does.
            findings.extend([no_jump("thumb x", &xs, (on - off) * 0.55)]);
            // The final frame is the frame an undisturbed run settles on.
            let mut calm = Rig::new(env, window, Scene { start_on: true });
            calm.steps(3);
            let diff = signature_diff(rig.now(), calm.now(), 4);
            findings.check(diff.is_empty(), || {
                format!("{gap} frames apart: the settled frame differs from a calm one: {diff:?}")
            });
        }
    });
}

#[test]
fn turning_a_travel_around_starts_from_where_the_thumb_is() {
    Env::matrix(&WINDOWS, &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { start_on: false });
        let (width, track) = (look(rig.now()).thumb.width(), look(rig.now()).track);
        let (off, on) = ends(track, width);
        toggle_by_click(&mut rig);
        // Run until the thumb is about half way, then turn it round.
        let mut guard = 0;
        while look(rig.now()).thumb.min_x() < off + (on - off) * 0.5 {
            rig.step();
            guard += 1;
            assert!(guard < 40, "the thumb never got half way");
        }
        let before = look(rig.now()).thumb.min_x();
        let turn = rig.frames.len();
        toggle_by_click(&mut rig);
        rig.steps(24);
        every_frame(&rig.frames, findings);
        let xs = thumb_xs(&rig.frames);
        // After the turn the thumb only ever goes back, from where it was.
        let after = &xs[turn - 1..];
        findings.extend([monotone_down("thumb x after the turn", after, EPS)]);
        findings.check(
            after[1] <= before + EPS && after[1] >= before - (on - off) * 0.55,
            || {
                format!(
                    "the thumb jumped when turned: {before:.2} -> {:.2}",
                    after[1]
                )
            },
        );
        findings.extend([ends_at("thumb x", &xs, off, EPS)]);
    });
}

#[test]
fn the_pointer_moving_over_the_switch_while_it_slides_changes_nothing_but_its_fill() {
    Env::matrix(&WINDOWS, &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { start_on: false });
        let (width, track) = (look(rig.now()).thumb.width(), look(rig.now()).track);
        let (_, on) = ends(track, width);
        let area = track;
        let (cx, cy) = (
            area.min_x() + area.width() / 2.0,
            area.min_y() + area.height() / 2.0,
        );
        toggle_by_click(&mut rig);
        // In and out of the switch on every frame of the travel.
        for step in 0..20 {
            if step % 2 == 0 {
                rig.hover_at(cx + area.width() * 3.0, cy);
            } else {
                rig.hover_at(cx, cy);
            }
            rig.step();
        }
        rig.steps(10);
        every_frame(&rig.frames, findings);
        let xs = thumb_xs(&rig.frames);
        findings.extend([monotone_up(
            "thumb x while the pointer comes and goes",
            &xs,
            EPS,
        )]);
        findings.extend([ends_at("thumb x", &xs, on, EPS)]);
    });
}

#[test]
fn keys_and_the_pointer_interleaved_agree_with_the_count_of_toggles() {
    Env::matrix(&WINDOWS, &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { start_on: false });
        let (width, track) = (look(rig.now()).thumb.width(), look(rig.now()).track);
        let (off, on) = ends(track, width);
        // Tab until the switch has focus (the page has no other stop), Space, a click,
        // Enter, the outside key, a click.
        // The switch has the focus once a ring is made under it.
        let focused = |rig: &Rig| {
            // The ring is a sibling of the switch's face, inside the focus wrapper around it.
            let frame = rig.now();
            let wrapper = frame
                .nth("Switch", 0)
                .parent
                .expect("the switch has a wrapper");
            frame
                .below(wrapper)
                .iter()
                .any(|node| node.a11y.component.as_deref() == Some("FocusRing"))
        };
        for _ in 0..4 {
            rig.key(Key::Named(NamedKey::Tab));
            if focused(&rig) {
                break;
            }
        }
        findings.check(focused(&rig), || "Tab never reached the switch".to_owned());
        let mut flips = 0;
        for step in 0..5 {
            match step {
                0 => rig.key_char(' '),
                1 => toggle_by_click(&mut rig),
                2 => rig.key(Key::Named(NamedKey::Enter)),
                3 => rig.key_char('x'),
                _ => toggle_by_click(&mut rig),
            }
            flips += 1;
            rig.steps(2);
        }
        rig.steps(30);
        every_frame(&rig.frames, findings);
        let xs = thumb_xs(&rig.frames);
        let expect = if flips % 2 == 1 { on } else { off };
        findings.extend([ends_at(
            &format!("thumb x after {flips} flips"),
            &xs,
            expect,
            EPS,
        )]);
    });
}

#[test]
fn reduced_motion_flips_the_thumb_at_once_and_never_shows_an_in_between() {
    Env::matrix(&WINDOWS, &Env::REDUCED, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { start_on: false });
        let (width, track) = (look(rig.now()).thumb.width(), look(rig.now()).track);
        let (off, on) = ends(track, width);
        toggle_by_click(&mut rig);
        rig.steps(12);
        every_frame(&rig.frames, findings);
        let xs = thumb_xs(&rig.frames);
        for (at, x) in xs.iter().enumerate() {
            findings.check((x - off).abs() <= EPS || (x - on).abs() <= EPS, || {
                format!("frame {at}: reduced motion drew the thumb at {x:.2}, between {off:.2} and {on:.2}")
            });
        }
        let start = start_index(&xs, EPS);
        findings.check(start <= LATENCY, || {
            format!("the thumb moved on frame {start}, after {LATENCY}")
        });
        findings.extend([ends_at("thumb x", &xs, on, EPS)]);
    });
}

/// The two states' thumbs and tracks as rendered, for a window in `env`.
fn rendered_pairs(env: Env) -> [(Color, Color); 2] {
    let off = Rig::new(env, DESKTOP, Scene { start_on: false });
    let on = Rig::new(env, DESKTOP, Scene { start_on: true });
    let pair = |rig: &Rig| {
        let now = look(rig.now());
        (now.thumb_fill, now.track_fill)
    };
    [pair(&off), pair(&on)]
}

/// Whether `thumb` is lighter than `track`.
fn lighter(thumb: Color, track: Color) -> bool {
    crate::contrast::luminance(thumb) > crate::contrast::luminance(track)
}

#[test]
fn the_thumb_keeps_one_look_in_both_states_in_both_schemes() {
    // The on thumb and the off thumb are one object that moves: when the off thumb is lighter than
    // its track (the dark scheme) the on thumb is lighter than its own, not a dark navy ink on a
    // light accent. Both are read from the pixels' source: the fills the renderer was given.
    Env::each(&[Env::ALL[0], Env::ALL[1]], |env, findings| {
        let [(off_thumb, off_track), (on_thumb, on_track)] = rendered_pairs(env);
        if env.dark {
            findings.check(lighter(off_thumb, off_track) == lighter(on_thumb, on_track), || {
                format!(
                    "the off thumb {} is {} than its track {}, the on thumb {} is {} than its track {}",
                    hex(off_thumb),
                    if lighter(off_thumb, off_track) {
                        "lighter"
                    } else {
                        "darker"
                    },
                    hex(off_track),
                    hex(on_thumb),
                    if lighter(on_thumb, on_track) {
                        "lighter"
                    } else {
                        "darker"
                    },
                    hex(on_track)
                )
            });
        }
        // The on thumb is the colour of the slider's knob, in every scheme: the library's one thumb
        // on an accent fill, not an ink derived from the accent.
        findings.check(on_thumb == Paint::Knob.color(&env.tokens()), || {
            format!(
                "the on thumb is {}, the knob colour is {}",
                hex(on_thumb),
                hex(Paint::Knob.color(&env.tokens()))
            )
        });
        // Each state's thumb reads against its track at least as well as against the page.
        for (name, thumb, track) in [("off", off_thumb, off_track), ("on", on_thumb, on_track)] {
            findings.check(ratio_opaque(thumb, track) >= 2.0, || {
                format!(
                    "the {name} thumb {} is only {:.2}:1 on its track {}",
                    hex(thumb),
                    ratio_opaque(thumb, track),
                    hex(track)
                )
            });
        }
    });
}

#[test]
fn the_on_thumb_is_the_same_colour_on_every_accent() {
    let mut findings = Findings::default();
    for tokens in crate::contrast_matrix::all_tokens() {
        let thumb = crate::controls::switch::thumb_paint(true).color(&tokens);
        let knob = Paint::Knob.color(&tokens);
        findings.check(thumb == knob, || {
            format!(
                "{} {}: the on thumb is {}, the knob {}",
                tokens.accent_name,
                tokens.scheme.is_dark(),
                hex(thumb),
                hex(knob)
            )
        });
    }
    findings.assert_clean();
}

#[test]
fn a_switch_row_keeps_its_height_and_its_label_through_a_toggle() {
    Env::matrix(&WINDOWS, &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(env, window, Scene { start_on: false });
        rig.click("Switch");
        rig.steps(24);
        let first = &rig.frames[0];
        let rows: Vec<Area> = first
            .all("Row")
            .iter()
            .filter_map(|node| node.area)
            .collect();
        for (at, frame) in rig.frames.iter().enumerate() {
            let now: Vec<Area> = frame
                .all("Row")
                .iter()
                .filter_map(|node| node.area)
                .collect();
            findings.check(now == rows, || {
                format!("frame {at}: the rows moved: {rows:?} -> {now:?}")
            });
        }
    });
}
