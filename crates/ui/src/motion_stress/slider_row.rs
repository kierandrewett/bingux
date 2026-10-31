//! The slider row of a settings page as its container changes width and its value changes text:
//! the label is whole when the row has room for it, the value shares the slider's line when it
//! fits, and a new value moves nothing but its own digits.

use freya::prelude::*;

use super::*;
use crate::group::Group;
use crate::rows::{row, slider_row, switch_row};
use crate::style::*;
use crate::surface::use_tokens;

/// A window wide enough for the regular density: the row is one line.
const WINDOW: (f32, f32) = (1000.0, 520.0);
/// The widths of the page column a window of 740 to 1000 px gives a page beside the sidebar, up to
/// the 720 px cap.
const COLUMNS: [f32; 6] = [360.0, 420.0, 480.0, 560.0, 640.0, 720.0];
const TITLE: &str = "Delay before repeat";
const OTHER: &str = "Repeat speed";
const EPS: f32 = 0.01;
/// The least a track needs beside its label to be a slider, in logical pixels.
const MIN_TRACK: f32 = 120.0;

/// Key `v` swaps the readout between a short and a long text, `w` the container's width.
#[derive(Clone, PartialEq)]
struct Scene {
    column: f32,
    other_column: f32,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut long = use_state(|| false);
        let mut wide = use_state(|| false);
        let mut level = use_state(|| 40.0_f64);
        let readout = |long: bool| {
            if long {
                "1500 ms".to_owned()
            } else {
                "5 ms".to_owned()
            }
        };
        let nested: Vec<Element> = vec![
            slider_row(TITLE, level(), move |next| level.set(next))
                .value_text(readout(long()))
                .into(),
            slider_row(OTHER, 60.0, |_| {}).value_text("30 ms").into(),
            row("Try it").value("A field").into(),
        ];
        let column = if wide() {
            self.other_column
        } else {
            self.column
        };
        WindowHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg), p(S4)])
                .on_global_key_down(move |event: Event<KeyboardEventData>| match &event.key {
                    Key::Character(c) if c == "v" => {
                        let next = !*long.peek();
                        long.set(next);
                    }
                    Key::Character(c) if c == "w" => {
                        let next = !*wide.peek();
                        wide.set(next);
                    }
                    _ => {}
                })
                .child(
                    rect()
                        .width(Size::px(column))
                        .child(Group::titled("Key repeat").rows(vec![
                            slider_row("Volume", 70.0, |_| {}).value_text("70%").into(),
                            switch_row("Repeat keys", true, |_| {}).nested(true, nested).into(),
                        ])),
                ),
        )
    }
}

fn text<'a>(texts: &'a [TextLook], wanted: &str) -> &'a TextLook {
    texts
        .iter()
        .find(|look| look.text == wanted)
        .unwrap_or_else(|| {
            panic!(
                "no text {wanted:?} on screen; texts: {:?}",
                texts.iter().map(|t| &t.text).collect::<Vec<_>>()
            )
        })
}

fn vertical_overlap(a: Area, b: Area) -> bool {
    a.min_y() < b.max_y() && b.min_y() < a.max_y()
}

/// The row of `title`: the nearest `LevelRow` above its text, as an area.
fn row_of(frame: &Frame, title: &str) -> Area {
    let look = text(&frame.texts(), title).clone();
    let mut up = frame.node(look.id).parent;
    while let Some(id) = up {
        let node = frame.node(id);
        if node.a11y.component.as_deref() == Some("LevelRow") {
            return node.area.expect("the row has an area");
        }
        up = node.parent;
    }
    panic!("{title:?} is not in a level row");
}

/// What the row must show whatever the width: the label whole when it has room, the value on the
/// slider's line when everything fits on it.
fn row_invariants(frame: &Frame, at: &str, title: &str, readout: &str, findings: &mut Findings) {
    let texts = frame.texts();
    let (label, value) = (text(&texts, title), text(&texts, readout));
    let area = row_of(frame, title);
    // The row's content: its padding is the gutter on each side; the nested rows sit one more in.
    let content = area.width() - 2.0 * 18.0;
    let gaps = 2.0 * 16.0;
    // The room the label needs for a whole line, with a real track and the value beside it.
    let needs = label.intrinsic + value.intrinsic + MIN_TRACK + gaps;
    let tracks = frame.all("SliderTrack");
    let track = tracks
        .iter()
        .filter_map(|node| node.area)
        .find(|track| vertical_overlap(*track, label.area))
        .or_else(|| {
            tracks
                .iter()
                .filter_map(|node| node.area)
                .find(|track| track.min_y() >= area.min_y() && track.max_y() <= area.max_y())
        })
        .expect("the row has a track");
    if content - 18.0 >= needs {
        findings.check(label.lines == 1 && !label.elided, || {
            format!(
                "{at}: {title:?} is {} on {} line(s) in a row {:.0} wide (its text needs {:.0}, the track and the value {:.0} more)",
                if label.elided { "elided" } else { "whole" },
                label.lines,
                area.width(),
                label.intrinsic,
                value.intrinsic + MIN_TRACK + gaps
            )
        });
        findings.check(vertical_overlap(value.area, track), || {
            format!(
                "{at}: the value {readout:?} at y {:.0}..{:.0} is on a line of its own under the track at y {:.0}..{:.0}",
                value.area.min_y(),
                value.area.max_y(),
                track.min_y(),
                track.max_y()
            )
        });
    }
    // The value is never cut, in any width.
    findings.check(!value.elided, || {
        format!("{at}: the value {readout:?} is cut by an ellipsis")
    });
}

#[test]
fn the_label_is_whole_and_the_value_shares_the_slider_line_wherever_they_fit() {
    Env::each(&Env::ALL, |env, findings| {
        for column in COLUMNS {
            let rig = Rig::new(
                env,
                WINDOW,
                Scene {
                    column,
                    other_column: column,
                },
            );
            let at = format!("page column {column:.0}");
            row_invariants(rig.now(), &at, TITLE, "5 ms", findings);
            row_invariants(rig.now(), &at, OTHER, "30 ms", findings);
        }
    });
}

#[test]
fn resizing_the_column_keeps_every_label_whole_that_fits_on_every_frame() {
    Env::each(&Env::ALL, |env, findings| {
        for pair in COLUMNS.windows(2) {
            let mut rig = Rig::new(
                env,
                WINDOW,
                Scene {
                    column: pair[0],
                    other_column: pair[1],
                },
            );
            rig.key_char('w');
            rig.steps(6);
            rig.key_char('w');
            rig.steps(6);
            for (at, frame) in rig.frames.iter().enumerate() {
                let width = row_of(frame, TITLE).width() + 2.0 * 16.0 + 16.0;
                let tag = format!(
                    "columns {:.0}/{:.0}, frame {at} (row {width:.0})",
                    pair[0], pair[1]
                );
                row_invariants(frame, &tag, TITLE, "5 ms", findings);
            }
            // The rows never jump in height while the column is resized.
            let heights: Vec<f32> = rig
                .frames
                .iter()
                .map(|frame| row_of(frame, TITLE).height())
                .collect();
            findings.extend([constant(
                "row height while the column resizes",
                &heights,
                0.5,
            )]);
        }
    });
}

#[test]
fn a_longer_value_moves_nothing_but_its_own_digits() {
    Env::each(&Env::ALL, |env, findings| {
        for column in [420.0, 560.0, 720.0] {
            let mut rig = Rig::new(
                env,
                WINDOW,
                Scene {
                    column,
                    other_column: column,
                },
            );
            let before = rig
                .now()
                .all("SliderTrack")
                .iter()
                .filter_map(|n| n.area)
                .collect::<Vec<_>>();
            let label_before = text(&rig.now().texts(), TITLE).area;
            rig.key_char('v');
            rig.steps(6);
            let after = rig
                .now()
                .all("SliderTrack")
                .iter()
                .filter_map(|n| n.area)
                .collect::<Vec<_>>();
            findings.check(before == after, || {
                format!("column {column:.0}: a longer readout moved or resized a track: {before:?} -> {after:?}")
            });
            let label_after = text(&rig.now().texts(), TITLE).area;
            findings.check(label_before == label_after, || {
                format!("column {column:.0}: a longer readout moved the label: {label_before:?} -> {label_after:?}")
            });
            findings.extend(
                unmoved(&rig.frames, EPS, |path| {
                    path.contains("ValueText") || path.contains("Label")
                })
                .into_iter()
                .map(Err),
            );
        }
    });
}

#[test]
fn the_sliders_of_one_card_start_at_one_x() {
    Env::each(&Env::ALL, |env, findings| {
        for column in COLUMNS {
            let rig = Rig::new(
                env,
                WINDOW,
                Scene {
                    column,
                    other_column: column,
                },
            );
            let starts: Vec<f32> = rig
                .now()
                .all("SliderTrack")
                .iter()
                .filter_map(|n| n.area)
                .map(|a| a.min_x())
                .collect();
            // The two nested rows share a slot; the third (not nested) starts at its own.
            findings.check(
                starts.len() >= 3 && (starts[1] - starts[2]).abs() <= 0.5,
                || format!("page column {column:.0}: the nested sliders start at {starts:?}"),
            );
        }
    });
}
