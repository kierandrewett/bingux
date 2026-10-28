//! The file view keeps its scroll position, with real wheel and key input in
//! the headless runner.
//!
//! Regression for a list that went back to the top a moment after every
//! wheel notch, key press or scrollbar drag. The view reset the scroll from an
//! effect on the search text; `scroll_to_y` reads the controller's bounds, the
//! scroll view rewrites them on every layout, so the effect re-ran after each
//! layout and put the offset back at 0. Every check below idles for several
//! seconds of frames after the input, which is where the old code lost it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bingux_files::{Entry, Kind, Listing, Selection};
use freya::prelude::*;

use super::scroll_memory::remember;
use super::view::FileView;
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;

const FILES: usize = 3000;
const FOLDER: &str = "/scroll-test";
const SIZE: (f32, f32) = (900.0, 600.0);
/// A list row and three grid sizes.
const LEVELS: [u16; 4] = [16, 32, 64, 128];

#[derive(PartialEq)]
struct Scene {
    icon_px: u16,
    owner: u64,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        let icon_px = self.icon_px;
        let level = use_state(move || icon_px);
        let selection = use_state(Selection::default);
        let (listing, order) = use_hook(|| {
            let mut listing = Listing::new(FOLDER);
            for index in 0..FILES {
                let name = format!("f{index:05}");
                listing.entries.push(Entry::named(
                    PathBuf::from(FOLDER).join(&name),
                    name,
                    Kind::File,
                    false,
                    false,
                ));
            }
            listing.complete = true;
            (Arc::new(listing), Arc::new((0..FILES).collect::<Vec<usize>>()))
        });
        OverlayHost::new(
            FileView::new(listing, order, selection)
                .icon_px(level)
                .thumbnails(false)
                .autofocus(true)
                .remember_for(self.owner),
        )
    }
}

fn open(icon_px: u16, owner: u64) -> Snapshot {
    let mut shot = Snapshot::new(move || Scene { icon_px, owner }.into(), SIZE);
    shot.settle(Duration::from_millis(400));
    shot
}

/// The indices of the files whose names are drawn, lowest first.
fn on_screen(shot: &mut Snapshot) -> Vec<usize> {
    let mut seen: Vec<usize> = shot.runner().find_many(|_, element| {
        let label = Label::try_downcast(element)?;
        label.text.strip_prefix('f')?.parse().ok()
    });
    seen.sort_unstable();
    seen
}

fn first(shot: &mut Snapshot) -> usize {
    on_screen(shot).first().copied().expect("some rows are drawn")
}

/// Run `seconds` of idle frames and check the first row does not move.
fn stays(shot: &mut Snapshot, expected: usize, what: &str) {
    for _ in 0..(4 * 4) {
        shot.settle(Duration::from_millis(250));
        assert_eq!(first(shot), expected, "{what}: the offset was taken back while idle");
    }
}

#[test]
fn a_wheel_scroll_stays_in_a_list_and_in_a_grid() {
    for px in LEVELS {
        // Each level is a view of its own: the view remembers where a folder
        // was left, and the next level must not inherit it.
        let mut shot = open(px, u64::from(px));
        assert_eq!(first(&mut shot), 0, "{px} px: opens at the top");
        shot.scroll(450.0, 300.0, -1500.0);
        shot.settle(Duration::from_millis(600));
        let moved = first(&mut shot);
        assert!(moved > 0, "{px} px: the wheel scrolled the content");
        stays(&mut shot, moved, &format!("{px} px after the wheel"));
        // And again: a second notch continues from where the first stopped.
        shot.scroll(450.0, 300.0, -1500.0);
        shot.settle(Duration::from_millis(600));
        let again = first(&mut shot);
        assert!(again > moved, "{px} px: a second scroll continues ({again} after {moved})");
        stays(&mut shot, again, &format!("{px} px after a second wheel"));
    }
}

#[test]
fn page_down_and_end_stay_where_they_reveal() {
    for px in LEVELS {
        let mut shot = open(px, u64::from(px));
        shot.click(450.0, 200.0);
        let before = first(&mut shot);
        for _ in 0..3 {
            shot.key(Key::Named(NamedKey::PageDown));
        }
        shot.settle(Duration::from_millis(600));
        let paged = first(&mut shot);
        assert!(paged > before, "{px} px: Page Down revealed rows below ({paged} after {before})");
        stays(&mut shot, paged, &format!("{px} px after Page Down"));

        shot.key(Key::Named(NamedKey::End));
        shot.settle(Duration::from_millis(600));
        assert_eq!(
            on_screen(&mut shot).last().copied(),
            Some(FILES - 1),
            "{px} px: End shows the last file"
        );
        let end = first(&mut shot);
        stays(&mut shot, end, &format!("{px} px after End"));
    }
}

#[test]
fn a_folder_opens_where_it_was_left_and_each_tab_has_its_own_place() {
    for px in [16, 64] {
        // About row 500 of a list; a grid is further down in lines but
        // further still in rows.
        remember(9, Path::new(FOLDER), -20_000, vec![]);
        let mut shot = open(px, 9);
        let restored = first(&mut shot);
        assert!(restored > 100, "{px} px: the folder opens where owner 9 left it ({restored})");
        stays(&mut shot, restored, &format!("{px} px restored"));
        drop(shot);

        let mut other = open(px, 10);
        assert_eq!(first(&mut other), 0, "{px} px: another tab on the folder starts at the top");
    }
}
