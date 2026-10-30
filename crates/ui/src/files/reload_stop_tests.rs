//! The toolbar's Reload button as Stop, in a headless Files window with a
//! folder that reads slowly on purpose.
//!
//! A folder whose `.hidden` is a pipe blocks the read of the folder for as
//! long as nobody writes to the pipe, as a dead network mount would (the
//! loader's own tests use the same trick): a read that takes as long as the
//! test wants, with no timing to race. Releasing the pipe lets the abandoned
//! read go; removing it lets the next read finish.

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use freya::prelude::*;
use freya_core::elements::label::Label;
use freya_core::elements::paragraph::Paragraph;

use super::{Browse, TabMemory, Where, browse};
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (1000.0, 600.0);
const REFRESH: &str = "Refresh (F5)";
const STOP: &str = "Stop (Esc)";

/// A folder with two files, and optionally the pipe that stalls its read.
struct Slow {
    root: PathBuf,
    pipe: PathBuf,
}

impl Slow {
    fn new(tag: &str, stalled: bool) -> Self {
        // A short path: the address bar's crumbs of a deep one overflow in
        // this window, which is not what these tests are about.
        let root = PathBuf::from("/dev/shm").join(format!("rs{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a folder in the temporary directory");
        std::fs::write(root.join("alpha.txt"), "a").expect("a file");
        std::fs::write(root.join("beta.txt"), "b").expect("a file");
        let slow = Slow { pipe: root.join(".hidden"), root };
        if stalled {
            slow.stall();
        }
        slow
    }

    /// Make the next read of the folder block.
    fn stall(&self) {
        let path = std::ffi::CString::new(self.pipe.as_os_str().as_bytes()).expect("a path");
        // SAFETY: `path` is a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0, "a pipe named .hidden");
    }

    /// Let the read that is stuck on the pipe go, and take the pipe away, so
    /// the next read of the folder is a quick one.
    fn free(&self) {
        if let Ok(pipe) = std::fs::OpenOptions::new().write(true).open(&self.pipe) {
            drop(pipe);
        }
        let _ = std::fs::remove_file(&self.pipe);
    }
}

impl Drop for Slow {
    fn drop(&mut self) {
        self.free();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(PartialEq)]
struct Window {
    memory: TabMemory,
}

impl Component for Window {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let memory = self.memory.clone();
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .child(OverlayHost::new(
                rect().tw(&tokens, &[w_full(), h_flex(), flex_col()]).child(browse(Browse::new(memory))),
            ))
    }
}

fn open(path: &Path) -> Snapshot {
    let memory = TabMemory::new(Where::Folder(path.to_path_buf()));
    Snapshot::new(move || Window { memory: memory.clone() }.into_element(), SIZE)
}

fn rest(shot: &mut Snapshot, ms: u64) {
    shot.settle(Duration::from_millis(ms));
}

/// The name and box of the toolbar's Reload / Stop button.
fn reload_button(shot: &mut Snapshot) -> (String, torin::prelude::Area) {
    let mut found: Vec<_> = shot
        .icon_buttons()
        .into_iter()
        .filter(|button| button.name == REFRESH || button.name == STOP)
        .collect();
    assert_eq!(found.len(), 1, "one Reload / Stop button: {found:?}");
    let button = found.remove(0);
    (button.name, button.area)
}

/// Whether a label or paragraph of the window has `text`. Nothing is clicked:
/// a second click on a row is a double click, which opens the file.
fn shows(shot: &mut Snapshot, text: &str) -> bool {
    let labels = shot
        .runner()
        .find_many(|_, element| Label::try_downcast(element).map(|label| label.text.to_string()));
    let paragraphs = shot.runner().find_many(|_, element| {
        Paragraph::try_downcast(element).map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect::<String>())
    });
    labels.iter().chain(&paragraphs).any(|found| found.contains(text))
}

fn key(shot: &mut Snapshot, key: NamedKey) {
    shot.key(Key::Named(key));
}

#[test]
fn escape_while_a_re_read_is_going_stops_it_and_keeps_the_listing() {
    let slow = Slow::new("escape-keeps", false);
    let mut shot = open(&slow.root);
    rest(&mut shot, 1200);
    assert!(shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"), "the folder is listed");
    let (name, box_before) = reload_button(&mut shot);
    assert_eq!(name, REFRESH);

    // From now on reading the folder stalls. F5 asks for a read.
    slow.stall();
    rest(&mut shot, 400);
    key(&mut shot, NamedKey::F5);
    rest(&mut shot, 60);
    let (name, _) = reload_button(&mut shot);
    assert_eq!(name, REFRESH, "a read that has only just begun is not offered for stopping");
    rest(&mut shot, 400);
    let (name, area) = reload_button(&mut shot);
    assert_eq!(name, STOP, "the read has gone on for a moment");
    assert_eq!(area, box_before, "the button keeps its box");

    key(&mut shot, NamedKey::Escape);
    rest(&mut shot, 100);
    assert!(shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"), "the old listing stays on show");
    // The Stop state is held for a moment, then the button is Reload again:
    // nothing is reading any more.
    rest(&mut shot, 700);
    let (name, area) = reload_button(&mut shot);
    assert_eq!(name, REFRESH, "the stopped read is over");
    assert_eq!(area, box_before);
    assert!(shows(&mut shot, "alpha.txt"));

    // And F5 reads the folder again, now that nothing stalls it.
    slow.free();
    key(&mut shot, NamedKey::F5);
    rest(&mut shot, 500);
    assert_eq!(reload_button(&mut shot).0, REFRESH);
    assert!(shows(&mut shot, "alpha.txt"));
}

#[test]
fn the_stop_button_stops_a_read_as_escape_does() {
    let slow = Slow::new("click-stops", false);
    let mut shot = open(&slow.root);
    rest(&mut shot, 1200);
    assert!(shows(&mut shot, "alpha.txt"));
    slow.stall();
    rest(&mut shot, 400);
    key(&mut shot, NamedKey::F5);
    rest(&mut shot, 500);
    let (name, area) = reload_button(&mut shot);
    assert_eq!(name, STOP);
    shot.click(area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0);
    rest(&mut shot, 1000);
    let (name, _) = reload_button(&mut shot);
    assert_eq!(name, REFRESH, "pressing Stop ended the read and the button went back to Reload");
    assert!(shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"));
}

#[test]
fn a_first_load_stopped_before_anything_came_in_says_so_and_reads_again_on_request() {
    let slow = Slow::new("first-load", true);
    let mut shot = open(&slow.root);
    rest(&mut shot, 600);
    assert_eq!(reload_button(&mut shot).0, STOP, "the first load has gone on for a moment");
    assert!(!shows(&mut shot, "alpha.txt"), "nothing has been read");

    key(&mut shot, NamedKey::Escape);
    rest(&mut shot, 200);
    assert!(shows(&mut shot, "Loading stopped"), "the empty folder says the load was stopped");
    assert!(!shows(&mut shot, "Folder is empty"));
    rest(&mut shot, 600);
    assert_eq!(reload_button(&mut shot).0, REFRESH);

    slow.free();
    key(&mut shot, NamedKey::F5);
    rest(&mut shot, 800);
    assert!(shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"), "the folder is read after all");
    assert!(!shows(&mut shot, "Loading stopped"));
}

#[test]
fn a_folder_that_opens_at_once_never_shows_stop() {
    let slow = Slow::new("fast", false);
    let mut shot = open(&slow.root);
    for _ in 0..60 {
        rest(&mut shot, 16);
        let found = shot
            .icon_buttons()
            .into_iter()
            .filter(|button| button.name == STOP)
            .count();
        assert_eq!(found, 0, "no flash of Stop while a quick folder loads");
    }
    assert!(shows(&mut shot, "alpha.txt"));
    assert_eq!(reload_button(&mut shot).0, REFRESH);
}

#[test]
fn escape_with_no_read_going_does_nothing_to_the_listing() {
    let slow = Slow::new("idle-escape", false);
    let mut shot = open(&slow.root);
    rest(&mut shot, 1200);
    assert!(shows(&mut shot, "alpha.txt"));
    key(&mut shot, NamedKey::Escape);
    rest(&mut shot, 200);
    assert!(shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"));
    assert_eq!(reload_button(&mut shot).0, REFRESH);
}
