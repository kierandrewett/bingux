//! Reload and Stop in a headless Files window with a folder that reads slowly
//! on purpose.
//!
//! A window's header has no Reload button: F5 (and the view menu) reads the
//! folder again. Stop joins the header's start only while a read has gone on
//! for a moment, so it can be pressed, and leaves when the read ends.
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
        let slow = Slow {
            pipe: root.join(".hidden"),
            root,
        };
        if stalled {
            slow.stall();
        }
        slow
    }

    /// Make the next read of the folder block.
    fn stall(&self) {
        let path = std::ffi::CString::new(self.pipe.as_os_str().as_bytes()).expect("a path");
        // SAFETY: `path` is a valid NUL-terminated path.
        assert_eq!(
            unsafe { libc::mkfifo(path.as_ptr(), 0o600) },
            0,
            "a pipe named .hidden"
        );
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
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_col()])
                    .child(browse(Browse::new(memory))),
            ))
    }
}

fn open(path: &Path) -> Snapshot {
    let memory = TabMemory::new(Where::Folder(path.to_path_buf()));
    Snapshot::new(
        move || {
            Window {
                memory: memory.clone(),
            }
            .into_element()
        },
        SIZE,
    )
}

fn rest(shot: &mut Snapshot, ms: u64) {
    shot.settle(Duration::from_millis(ms));
}

/// The header's Stop button, if it is on show. There is never more than one.
fn stop_button(shot: &mut Snapshot) -> Option<torin::prelude::Area> {
    let mut found: Vec<_> = shot
        .icon_buttons()
        .into_iter()
        .filter(|button| button.name == STOP)
        .collect();
    assert!(found.len() <= 1, "at most one Stop button: {found:?}");
    found.pop().map(|button| button.area)
}

/// The box of the header's Back button, which Stop follows.
fn back_button(shot: &mut Snapshot) -> torin::prelude::Area {
    shot.icon_buttons()
        .into_iter()
        .find(|button| button.name == "Back")
        .map(|button| button.area)
        .expect("a Back button")
}

/// Whether a label or paragraph of the window has `text`. Nothing is clicked:
/// a second click on a row is a double click, which opens the file.
fn shows(shot: &mut Snapshot, text: &str) -> bool {
    let labels = shot
        .runner()
        .find_many(|_, element| Label::try_downcast(element).map(|label| label.text.to_string()));
    let paragraphs = shot.runner().find_many(|_, element| {
        Paragraph::try_downcast(element).map(|paragraph| {
            paragraph
                .spans
                .iter()
                .map(|span| span.text.to_string())
                .collect::<String>()
        })
    });
    labels
        .iter()
        .chain(&paragraphs)
        .any(|found| found.contains(text))
}

fn key(shot: &mut Snapshot, key: NamedKey) {
    shot.key(Key::Named(key));
}

#[test]
fn escape_while_a_re_read_is_going_stops_it_and_keeps_the_listing() {
    let slow = Slow::new("escape-keeps", false);
    let mut shot = open(&slow.root);
    rest(&mut shot, 1200);
    assert!(
        shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"),
        "the folder is listed"
    );
    assert_eq!(stop_button(&mut shot), None, "nothing is reading");

    // From now on reading the folder stalls. F5 asks for a read.
    slow.stall();
    rest(&mut shot, 400);
    key(&mut shot, NamedKey::F5);
    rest(&mut shot, 60);
    assert_eq!(
        stop_button(&mut shot),
        None,
        "a read that has only just begun is not offered for stopping"
    );
    rest(&mut shot, 400);
    let stop = stop_button(&mut shot).expect("the read has gone on for a moment: Stop is on show");
    let back = back_button(&mut shot);
    assert!(
        stop.min_x() > back.min_x(),
        "Stop sits after Back and Forward, at the header's start"
    );

    key(&mut shot, NamedKey::Escape);
    rest(&mut shot, 100);
    assert!(
        shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"),
        "the old listing stays on show"
    );
    // The Stop state is held for a moment, then it goes: nothing is reading
    // any more.
    rest(&mut shot, 700);
    assert_eq!(stop_button(&mut shot), None, "the stopped read is over");
    assert!(shows(&mut shot, "alpha.txt"));

    // And F5 reads the folder again, now that nothing stalls it.
    slow.free();
    key(&mut shot, NamedKey::F5);
    rest(&mut shot, 500);
    assert_eq!(stop_button(&mut shot), None);
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
    let area = stop_button(&mut shot).expect("Stop is on show");
    shot.click(
        area.min_x() + area.width() / 2.0,
        area.min_y() + area.height() / 2.0,
    );
    rest(&mut shot, 1000);
    assert_eq!(
        stop_button(&mut shot),
        None,
        "pressing Stop ended the read and the button went"
    );
    assert!(shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"));
}

#[test]
fn a_first_load_stopped_before_anything_came_in_says_so_and_reads_again_on_request() {
    let slow = Slow::new("first-load", true);
    let mut shot = open(&slow.root);
    rest(&mut shot, 600);
    assert!(
        stop_button(&mut shot).is_some(),
        "the first load has gone on for a moment"
    );
    assert!(!shows(&mut shot, "alpha.txt"), "nothing has been read");

    key(&mut shot, NamedKey::Escape);
    rest(&mut shot, 200);
    assert!(
        shows(&mut shot, "Loading stopped"),
        "the empty folder says the load was stopped"
    );
    assert!(!shows(&mut shot, "Folder is empty"));
    rest(&mut shot, 600);
    assert_eq!(stop_button(&mut shot), None);

    slow.free();
    key(&mut shot, NamedKey::F5);
    rest(&mut shot, 800);
    assert!(
        shows(&mut shot, "alpha.txt") && shows(&mut shot, "beta.txt"),
        "the folder is read after all"
    );
    assert!(!shows(&mut shot, "Loading stopped"));
}

#[test]
fn a_folder_that_opens_at_once_never_shows_stop() {
    let slow = Slow::new("fast", false);
    let mut shot = open(&slow.root);
    for _ in 0..60 {
        rest(&mut shot, 16);
        assert_eq!(
            stop_button(&mut shot),
            None,
            "no flash of Stop while a quick folder loads"
        );
    }
    assert!(shows(&mut shot, "alpha.txt"));
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
    assert_eq!(stop_button(&mut shot), None);
}
