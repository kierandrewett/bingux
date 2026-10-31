//! The omnibox's card opening, growing and closing: it grows out of the field along one curve, from
//! the field's own box, and nothing under it moves.
//!
//! Every frame of every motion checks that the field keeps its place and size, that the card keeps
//! the field's left edge and width, that the text and boxes of the page under the card keep their
//! places (the card is laid over them, never between them), that the card never leaves the window,
//! and that rows do not come and go while it moves.

use freya::prelude::*;

use super::*;
use crate::controls::{OmniDraft, OmniItem, OmniPart, OmniRow, Omnibox};
use crate::icons::{Icon, icon};
use crate::motion::MOVE_MS;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::BODY;

const DESKTOP: (f32, f32) = (900.0, 700.0);
const PHONE: (f32, f32) = (390.0, 800.0);
const SHORT: (f32, f32) = (900.0, 260.0);
const EPS: f32 = 0.01;
const ABOVE: &str = "Before the field";
const BELOW: &str = "After the field";

thread_local! {
    /// The width the field's column has, so a test can resize it.
    static WIDTH: std::cell::Cell<Option<State<f32>>> = const { std::cell::Cell::new(None) };
    /// How many rows the card offers, so a test can stream more in.
    static ROWS: std::cell::Cell<Option<State<usize>>> = const { std::cell::Cell::new(None) };
}

#[derive(Clone, PartialEq)]
struct Scene {
    rows: usize,
    width: f32,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let draft = OmniDraft::use_new();
        let width = use_state(|| self.width);
        let rows = use_state(|| self.rows);
        WIDTH.set(Some(width));
        ROWS.set(Some(rows));
        let mut items = vec![OmniItem::Strip {
            element: rect().into(),
            height: 30.0,
        }];
        for number in 0..*rows.read() {
            let name = format!("Result {number}");
            items.push(OmniItem::Row(OmniRow {
                lead: icon(Icon::Folder, IconSize::Control, Paint::Muted.color(&tokens)).into(),
                parts: vec![OmniPart::plain(name.clone())],
                detail: Some("in a folder".into()),
                chip: None,
                on_press: EventHandler::new(|_: ()| {}),
                fill: None,
                name,
            }));
        }
        WindowHost::new(
            rect()
                .expanded()
                .tw(&tokens, &[bg(Paint::Bg), p(S4), flex_col(), gap(S2)])
                .child(BODY.label(&tokens, ABOVE.to_string()))
                .child(
                    rect().width(Size::px(*width.read())).child(
                        Omnibox::new(draft, |_: String| {}, |_: ()| {})
                            .items(items)
                            .placeholder("Type"),
                    ),
                )
                .child(BODY.label(&tokens, BELOW.to_string()))
                .child(BODY.label(&tokens, "Another line under it".to_string())),
        )
    }
}

fn y_of(frame: &Frame, text: &str) -> f32 {
    frame
        .texts()
        .iter()
        .find(|look| look.text == text)
        .unwrap_or_else(|| panic!("no text {text:?}"))
        .area
        .min_y()
}

fn card_height(frame: &Frame) -> f32 {
    frame.area("OmniboxCard").height()
}

fn type_in(rig: &mut Rig, text: &str) {
    rig.click("Omnibox");
    for c in text.chars() {
        rig.key_char(c);
    }
}

fn every_frame(frames: &[Frame], findings: &mut Findings) {
    let first = &frames[0];
    let field = first.area("Omnibox");
    for (at, frame) in frames.iter().enumerate() {
        let here = frame.area("Omnibox");
        findings.check(
            (here.min_x() - field.min_x()).abs() <= EPS
                && (here.min_y() - field.min_y()).abs() <= EPS
                && (here.width() - field.width()).abs() <= EPS
                && (here.height() - field.height()).abs() <= EPS,
            || format!("frame {at}: the field's place in the layout moved or changed size"),
        );
        let card = frame.area("OmniboxCard");
        findings.check(
            (card.min_x() - field.min_x()).abs() <= EPS
                && (card.min_y() - field.min_y()).abs() <= EPS
                && (card.width() - field.width()).abs() <= EPS,
            || {
                format!(
                    "frame {at}: the card ({:.1},{:.1} {:.1} wide) is not at the field's corner ({:.1},{:.1} {:.1} wide)",
                    card.min_x(),
                    card.min_y(),
                    card.width(),
                    field.min_x(),
                    field.min_y(),
                    field.width()
                )
            },
        );
        findings.check(card.max_y() <= frame.window.1 + EPS, || {
            format!(
                "frame {at}: the card ends at {:.1}, below the window's {:.1}",
                card.max_y(),
                frame.window.1
            )
        });
        findings.check(card.height() + EPS >= field.height(), || {
            format!(
                "frame {at}: the card is shorter ({:.1}) than the field ({:.1})",
                card.height(),
                field.height()
            )
        });
    }
    let above: Vec<f32> = frames.iter().map(|f| y_of(f, ABOVE)).collect();
    let below: Vec<f32> = frames.iter().map(|f| y_of(f, BELOW)).collect();
    findings.extend([constant("the line above the field", &above, EPS)]);
    findings.extend([constant(
        "the line under the field (the card is over it, not between)",
        &below,
        EPS,
    )]);
    findings.extend(elision_flips(frames, 0).into_iter().map(Err));
}

/// How many rows are drawn in a frame.
fn rows_drawn(frame: &Frame) -> usize {
    frame.all("OmniboxRow").len()
}

#[test]
fn typing_grows_the_card_out_of_the_field_along_one_curve_and_moves_nothing_else() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(
            env,
            window,
            Scene {
                rows: 6,
                width: window.0 - 32.0,
            },
        );
        let resting = card_height(rig.now());
        // Editing alone shows no card: the card is the field.
        rig.click("Omnibox");
        rig.steps(4);
        findings.check((card_height(rig.now()) - resting).abs() <= EPS, || {
            "a card showed before anything was typed".into()
        });
        type_in(&mut rig, "re");
        rig.mark();
        let from = rig.frames.len() - 1;
        rig.steps(24);
        let frames = &rig.frames[from..];
        every_frame(frames, findings);
        let heights: Vec<f32> = frames.iter().map(card_height).collect();
        findings.extend([monotone_up("the card", &heights, EPS)]);
        let full = *heights.last().expect("a frame");
        findings.check(full > resting + 100.0, || {
            format!("the card ended {full:.1} tall, the field is {resting:.1}")
        });
        findings.extend([within("the card", &heights, resting, full, EPS)]);
        findings.extend([no_jump("the card", &heights, (full - resting) * 0.6)]);
        let (start, rest) = (start_index(&heights, EPS), rest_index(&heights, EPS));
        let wanted = crate::shell::panel::MOTION_MS.div_ceil(STEP_MS) as usize;
        let took = rest + 1 - start;
        if !env.reduced {
            findings.check((wanted - 1..=wanted + 3).contains(&took), || {
                format!(
                    "the card took {took} frames, {wanted} are {} ms: {}",
                    crate::shell::panel::MOTION_MS,
                    show(&heights)
                )
            });
        } else {
            findings.check(
                heights
                    .iter()
                    .skip(1)
                    .all(|height| (height - full).abs() <= EPS),
                || {
                    format!(
                        "with reduced motion the card is at its height at once: {}",
                        show(&heights)
                    )
                },
            );
        }
        // Rows never come and go while it grows: they are all there from the first frame that has a list.
        let drawn: Vec<f32> = frames
            .iter()
            .skip_while(|f| rows_drawn(f) == 0)
            .map(|f| rows_drawn(f) as f32)
            .collect();
        findings.extend([constant("the rows drawn while the card grows", &drawn, 0.5)]);
        let _ = MOVE_MS;
    });
}

#[test]
fn escape_closes_the_card_back_into_the_field_and_the_field_keeps_the_keyboard() {
    Env::matrix(&[DESKTOP, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(
            env,
            window,
            Scene {
                rows: 6,
                width: window.0 - 32.0,
            },
        );
        let resting = card_height(rig.now());
        type_in(&mut rig, "re");
        rig.steps(24);
        let open = card_height(rig.now());
        findings.check(open > resting + 100.0, || "the card did not open".into());
        rig.key(Key::Named(NamedKey::Escape));
        let from = rig.frames.len() - 1;
        rig.steps(24);
        let frames = &rig.frames[from..];
        every_frame(frames, findings);
        let heights: Vec<f32> = frames.iter().map(card_height).collect();
        findings.extend([monotone_down("the card", &heights, EPS)]);
        findings.extend([ends_at("the card", &heights, resting, EPS)]);
        findings.extend([no_jump("the card", &heights, (open - resting) * 0.6)]);
        // The rows stay drawn while it closes, so what closes is what was open (no blank card).
        let drawn: Vec<f32> = frames
            .iter()
            .take_while(|f| card_height(f) > resting + 4.0)
            .map(|f| rows_drawn(f) as f32)
            .collect();
        findings.extend([constant(
            "the rows drawn while the card closes",
            &drawn,
            0.5,
        )]);
        // Typing again opens it again: the field still had the keyboard.
        rig.key_char('p');
        rig.steps(24);
        findings.check(card_height(rig.now()) > resting + 100.0, || {
            "the field lost the keyboard on the first Escape".into()
        });
    });
}

#[test]
fn closing_in_the_middle_of_opening_goes_back_from_where_it_is() {
    Env::matrix(&[DESKTOP], &Env::ALL, |env, window, findings| {
        if env.reduced {
            return;
        }
        let mut rig = Rig::new(
            env,
            window,
            Scene {
                rows: 8,
                width: 600.0,
            },
        );
        let resting = card_height(rig.now());
        type_in(&mut rig, "r");
        rig.steps(3);
        let seen = card_height(rig.now());
        findings.check(seen > resting + 4.0, || {
            format!("the card has not begun after 5 frames: {seen:.1} of field {resting:.1}")
        });
        rig.key(Key::Named(NamedKey::Escape));
        let from = rig.frames.len() - 1;
        rig.steps(24);
        let frames = &rig.frames[from..];
        every_frame(frames, findings);
        let heights: Vec<f32> = frames.iter().map(card_height).collect();
        findings.check((heights[0] - seen).abs() <= EPS, || {
            format!(
                "the close began at {:.1}, not at the {seen:.1} that was on screen",
                heights[0]
            )
        });
        findings.extend([monotone_down("the card", &heights, EPS)]);
        findings.extend([ends_at("the card", &heights, resting, EPS)]);
        findings.extend([no_jump("the card", &heights, 200.0)]);
        // And the other way: opening again part way down starts from there.
        rig.steps(0);
        rig.key_char('e');
        rig.steps(3);
        let part = card_height(rig.now());
        rig.key(Key::Named(NamedKey::Escape));
        rig.key_char('x');
        let from = rig.frames.len() - 1;
        rig.steps(24);
        let heights: Vec<f32> = rig.frames[from..].iter().map(card_height).collect();
        findings.check(heights[0] <= part + EPS + 40.0, || {
            format!(
                "reopening began at {:.1}, above the {part:.1} it was at",
                heights[0]
            )
        });
        findings.extend([no_jump("the card", &heights, 200.0)]);
    });
}

#[test]
fn rows_streaming_in_make_the_card_grow_a_little_at_a_time_and_never_jump() {
    Env::matrix(&[DESKTOP], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(
            env,
            window,
            Scene {
                rows: 1,
                width: 600.0,
            },
        );
        type_in(&mut rig, "re");
        rig.steps(24);
        let mut heights = vec![card_height(rig.now())];
        let from = rig.frames.len() - 1;
        for more in 2..=8 {
            rig.shot.runner().run_in(|| {
                let mut rows = ROWS.get().expect("the scene's rows");
                rows.set(more);
            });
            rig.steps(3);
        }
        rig.steps(24);
        let frames = &rig.frames[from..];
        every_frame(frames, findings);
        heights.extend(frames.iter().map(card_height));
        findings.extend([monotone_up("the card", &heights, EPS)]);
        let row = frames
            .last()
            .and_then(|f| {
                f.all("OmniboxRow")
                    .first()
                    .map(|n| n.area.expect("area").height())
            })
            .unwrap_or(36.0);
        findings.extend([no_jump("the card", &heights, row * 1.1)]);
        findings.extend([ends_at(
            "the card",
            &heights,
            *heights.last().expect("a frame"),
            EPS,
        )]);
        // The first row is where it was, however many come after.
        let first: Vec<f32> = frames
            .iter()
            .filter_map(|f| {
                f.all("OmniboxRow")
                    .first()
                    .map(|n| n.area.expect("area").min_y())
            })
            .collect();
        findings.extend([constant("the first row", &first, EPS)]);
    });
}

#[test]
fn the_card_never_leaves_the_window_and_scrolls_what_does_not_fit() {
    Env::matrix(&[SHORT, PHONE], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(
            env,
            window,
            Scene {
                rows: 40,
                width: window.0 - 32.0,
            },
        );
        type_in(&mut rig, "re");
        rig.steps(30);
        every_frame(&rig.frames[1..], findings);
        let card = rig.now().area("OmniboxCard");
        findings.check(card.max_y() <= window.1 + EPS, || {
            format!(
                "the card ends at {:.1} in a window {:.1} tall",
                card.max_y(),
                window.1
            )
        });
        let row = rig
            .now()
            .all("OmniboxRow")
            .first()
            .map(|n| n.area.expect("area").height())
            .unwrap_or(36.0);
        let list_height = rig.now().area("OmniboxList").height();
        findings.check(list_height < 40.0 * row, || {
            format!("the list is {list_height:.1} tall for 40 rows of {row:.1}: it did not scroll")
        });
        // The wheel over it scrolls the list, not the page.
        let y_before = y_of(rig.now(), ABOVE);
        let list = rig.now().area("OmniboxList");
        rig.wheel(list.min_x() + 20.0, list.min_y() + 20.0, -120.0);
        rig.steps(6);
        findings.check((y_of(rig.now(), ABOVE) - y_before).abs() <= EPS, || {
            "the page scrolled under the card".into()
        });
    });
}

#[test]
fn the_card_follows_the_field_when_the_window_is_resized_while_it_is_open() {
    Env::matrix(&[DESKTOP], &Env::ALL, |env, window, findings| {
        let mut rig = Rig::new(
            env,
            window,
            Scene {
                rows: 6,
                width: 700.0,
            },
        );
        type_in(&mut rig, "re");
        rig.steps(24);
        let wide = rig.now().area("OmniboxCard").width();
        rig.shot.runner().run_in(|| {
            let mut width = WIDTH.get().expect("the scene's width");
            width.set(420.0);
        });
        rig.steps(8);
        let narrow = rig.now().area("OmniboxCard");
        let field = rig.now().area("Omnibox");
        findings.check(
            (narrow.width() - 420.0).abs() <= 1.0 && wide > 600.0,
            || {
                format!(
                    "the card is {:.1} wide after the field went from {wide:.1} to 420",
                    narrow.width()
                )
            },
        );
        findings.check(
            (narrow.min_x() - field.min_x()).abs() <= EPS
                && (narrow.width() - field.width()).abs() <= EPS,
            || "the card is not at the field's edge and width after the resize".into(),
        );
        // The window gets shorter than the card: it is cut to the room that is left.
        rig.shot.runner().run_in(|| {
            let window = Platform::get().current_window();
            let mut size = window.root_size;
            size.set(torin::prelude::Size2D::new(
                420.0 * env.scale as f32,
                200.0 * env.scale as f32,
            ));
        });
        rig.steps(30);
        let card = rig.now().area("OmniboxCard");
        findings.check(card.max_y() <= 200.0 + 1.0, || {
            format!(
                "the card ends at {:.1} in a window that is 200 tall",
                card.max_y()
            )
        });
    });
}
