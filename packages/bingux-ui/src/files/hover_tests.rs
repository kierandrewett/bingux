//! The detail card in the headless runner, frame by frame, with real pointer
//! and wheel input over a list of three thousand files.
//!
//! The card used to stay on screen, with the text of a row that had scrolled
//! away, while the list moved under the pointer, and to pop up later for a row
//! the pointer had passed. These tests watch every frame.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bingux_files::{Entry, Kind, Listing, Selection};
use freya::prelude::*;

use super::hover_card::renders;
use super::view::FileView;
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const FILES: usize = 3000;
const FOLDER: &str = "/hover-test";
const SIZE: (f32, f32) = (900.0, 600.0);
const FRAME: Duration = Duration::from_millis(16);
/// The pointer rests here: over a row, left of the card that opens beside it.
const POINTER: (f32, f32) = (450.0, 150.0);

#[derive(PartialEq)]
struct Scene {
    owner: u64,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        let level = use_state(|| 16_u16);
        let selection = use_state(Selection::default);
        let (listing, order) = use_hook(|| {
            let mut listing = Listing::new(FOLDER);
            for index in 0..FILES {
                let name = format!("f{index:05}.txt");
                let mut entry = Entry::named(
                    PathBuf::from(FOLDER).join(&name),
                    name,
                    Kind::File,
                    false,
                    false,
                );
                entry.sized = true;
                entry.size = 100 + index as u64;
                entry.modified =
                    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + index as u64 * 3600));
                listing.entries.push(entry);
            }
            listing.complete = true;
            (Arc::new(listing), Arc::new((0..FILES).collect::<Vec<usize>>()))
        });
        WindowHost::new(OverlayHost::new(
            FileView::new(listing, order, selection)
                .icon_px(level)
                .thumbnails(false)
                .autofocus(true)
                .remember_for(self.owner),
        ))
    }
}

fn open(owner: u64) -> Snapshot {
    let mut shot = Snapshot::new(move || Scene { owner }.into(), SIZE);
    shot.settle(Duration::from_millis(400));
    shot
}

fn frame(shot: &mut Snapshot) {
    shot.runner().poll_n(FRAME, 1);
}

/// Every label that is a file name or a card line, with where it is drawn.
fn labels(shot: &mut Snapshot) -> Vec<(String, Area)> {
    shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        Some((label.text.to_string(), node.layout().area))
    })
}

/// The name on the card, if one is up: the file name drawn straight above the
/// "Modified" line, at its left edge. A row's name is somewhere else.
fn card(shot: &mut Snapshot) -> Option<String> {
    let all = labels(shot);
    let (_, modified) = all.iter().find(|(text, _)| text.starts_with("Modified "))?;
    all.iter()
        .filter(|(text, area)| {
            text.ends_with(".txt")
                && (area.min_x() - modified.min_x()).abs() < 0.5
                && area.min_y() < modified.min_y()
        })
        .max_by(|a, b| a.1.min_y().total_cmp(&b.1.min_y()))
        .map(|(text, _)| text.clone())
}

/// The file names drawn as rows, top first.
fn rows(shot: &mut Snapshot) -> Vec<(String, Area)> {
    let mut rows: Vec<(String, Area)> = labels(shot)
        .into_iter()
        .filter(|(text, area)| text.ends_with(".txt") && area.min_x() < POINTER.0 - 20.0)
        .collect();
    rows.sort_by(|a, b| a.1.min_y().total_cmp(&b.1.min_y()));
    rows
}

/// The distance from one row to the next.
fn row_pitch(shot: &mut Snapshot) -> f32 {
    let rows = rows(shot);
    rows[1].1.min_y() - rows[0].1.min_y()
}

/// The name of the row at height `y`: the one whose name is nearest.
fn row_at(shot: &mut Snapshot, y: f32) -> String {
    let middle = |area: &Area| area.min_y() + area.height() / 2.0;
    rows(shot)
        .into_iter()
        .min_by(|a, b| (middle(&a.1) - y).abs().total_cmp(&(middle(&b.1) - y).abs()))
        .map(|(text, _)| text)
        .expect("a row is under the pointer")
}

/// Steps until a card is up, and returns the frames it took.
fn until_card(shot: &mut Snapshot, within: usize) -> usize {
    for frames in 1..=within {
        frame(shot);
        if card(shot).is_some() {
            return frames;
        }
    }
    panic!("no card within {within} frames");
}

#[test]
fn a_scroll_takes_the_card_down_at_once_and_keeps_it_down_until_the_content_is_still() {
    let mut shot = open(1);
    shot.hover(POINTER.0, POINTER.1);
    let waited = until_card(&mut shot, 90);
    assert!(waited * 16 >= 600, "the card waits for the pointer to rest ({waited} frames)");
    let before = card(&mut shot).expect("a card is up");
    assert_eq!(before, row_at(&mut shot, POINTER.1), "the card is for the row under the pointer");

    // The first notch: the card is gone within two frames.
    shot.scroll(POINTER.0, POINTER.1, -300.0);
    frame(&mut shot);
    frame(&mut shot);
    assert_eq!(card(&mut shot), None, "the card went with the first notch");

    // A second of notches, the pointer never moving: not one frame draws a card.
    for notch in 0..20 {
        shot.scroll(POINTER.0, POINTER.1, -300.0);
        for tick in 0..3 {
            frame(&mut shot);
            assert_eq!(
                card(&mut shot),
                None,
                "notch {notch}, frame {tick}: a card is drawn while the list scrolls"
            );
        }
    }

    // Still: not at once, and not before the content has been still a while.
    let mut quiet = 0;
    let appeared = loop {
        frame(&mut shot);
        quiet += 1;
        if let Some(name) = card(&mut shot) {
            break name;
        }
        assert!(quiet < 120, "the card never came back");
    };
    assert!(quiet * 16 >= 400, "the card came back after only {} ms of stillness", quiet * 16);
    assert_ne!(appeared, before, "the card is for the row now under the pointer, not the old one");
    assert_eq!(appeared, row_at(&mut shot, POINTER.1));
}

#[test]
fn moving_to_the_next_row_swaps_the_card_in_place_with_no_frame_without_one() {
    let mut shot = open(2);
    shot.hover(POINTER.0, POINTER.1);
    until_card(&mut shot, 90);
    let first = card(&mut shot).expect("a card is up");

    // Down one row, and another: every frame draws a card, ending on the new row.
    let row = row_pitch(&mut shot);
    let mut seen = vec![first.clone()];
    for step in 1..=2 {
        shot.hover(POINTER.0, POINTER.1 + row * step as f32);
        for _ in 0..6 {
            frame(&mut shot);
            let name = card(&mut shot).expect("no frame is without a card while it moves between rows");
            if seen.last() != Some(&name) {
                seen.push(name);
            }
        }
    }
    assert_eq!(seen.len(), 3, "the card followed the pointer over two rows: {seen:?}");
    assert_ne!(seen[0], seen[1]);
    assert_ne!(seen[1], seen[2]);
    assert_eq!(seen[2], row_at(&mut shot, POINTER.1 + 2.0 * row));
}

#[test]
fn a_scroll_with_the_card_armed_does_not_draw_the_view_or_the_list_again() {
    let mut shot = open(3);
    shot.hover(POINTER.0, POINTER.1);
    until_card(&mut shot, 90);
    shot.settle(Duration::from_millis(200));
    let before = renders::counts();
    // Sixty ticks of scrolling, with a card that was up and a pointer that rests.
    for _ in 0..60 {
        shot.scroll(POINTER.0, POINTER.1, -64.0);
        frame(&mut shot);
        assert_eq!(card(&mut shot), None);
    }
    let after = renders::counts();
    assert!(
        after.0 - before.0 <= 3,
        "the view drew {} times for 60 scroll ticks",
        after.0 - before.0
    );
    assert!(
        after.1 - before.1 <= 3,
        "the list drew {} times for 60 scroll ticks",
        after.1 - before.1
    );
}

#[test]
fn a_scroll_that_sends_no_pointer_event_still_leaves_the_card_for_the_row_under_the_pointer() {
    // Freya moves the hover after the layout of a wheel notch and not after a
    // key's scroll or a glide: the row an event last named is not the one the
    // pointer ends on.
    let mut shot = open(4);
    shot.hover(POINTER.0, POINTER.1);
    until_card(&mut shot, 90);
    let before = card(&mut shot).expect("a card is up");
    for _ in 0..3 {
        shot.key(Key::Named(NamedKey::PageDown));
    }
    // The scroll took it down.
    for _ in 0..3 {
        frame(&mut shot);
    }
    assert_eq!(card(&mut shot), None, "the card stays up over rows that moved");
    let mut still = 0;
    let back = loop {
        frame(&mut shot);
        still += 1;
        if let Some(name) = card(&mut shot) {
            break name;
        }
        assert!(still < 150, "the card never came back");
    };
    assert_ne!(back, before, "the list moved, so the row under the pointer did");
    assert_eq!(back, row_at(&mut shot, POINTER.1), "the card is for the row at the pointer");
}
