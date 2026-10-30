//! Headless renders of the player and the document viewer, against the real
//! helper: a film plays, pauses and seeks; a pane does not play by itself; a
//! PDF scrolls, zooms and finds. The PNGs go to `BINGUX_PREVIEW_SHOTS` when it
//! is set, so they can be looked at.
//!
//! The tests need `bingux-preview-host` built (`cargo build` in
//! `packages/bingux-preview-host`: it is found beside `target/<profile>/deps`)
//! and `gst-launch-1.0` for the film.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bingux_preview_core::PreviewSource;
use freya::prelude::*;
use freya_core::elements::image::Image;

use super::{Mode, PreviewHost, PreviewNav, host_available};
use crate::snapshot::Snapshot;
use crate::style::*;

const POPUP: (f32, f32) = (1100.0, 820.0);
const PHONE: (f32, f32) = (390.0, 800.0);
const PANE: (f32, f32) = (380.0, 600.0);
/// How long a test waits for the helper to start and decode.
const PATIENCE: Duration = Duration::from_secs(25);

static DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = std::env::temp_dir().join(format!("bingux-preview-render-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a fixture directory");
    dir
});

/// A three-second 320x180 clip with a ball and a tone: WebM, VP8 and Vorbis.
static CLIP: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = DIRECTORY.join("Holiday clip.webm");
    let status = Command::new("gst-launch-1.0")
        .arg("-q")
        .args(["videotestsrc", "pattern=ball", "num-buffers=90", "!", "video/x-raw,width=320,height=180,framerate=30/1", "!", "videoconvert", "!"])
        .args(["vp8enc", "deadline=1", "keyframe-max-dist=15", "!", "webmmux", "name=mux", "!", "filesink"])
        .arg(format!("location={}", path.display()))
        .args(["audiotestsrc", "num-buffers=140", "samplesperbuffer=1024", "!", "audioconvert", "!", "vorbisenc", "!", "mux."])
        .status()
        .expect("gst-launch-1.0 makes the test film");
    assert!(status.success(), "gst-launch-1.0 failed to make the test film");
    path
});

static SONG: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = DIRECTORY.join("Tone.wav");
    std::fs::write(&path, bingux_preview_core::testing::wav_tone(3)).expect("write the song");
    path
});

static REPORT: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = DIRECTORY.join("Quarterly report.pdf");
    std::fs::write(&path, bingux_preview_core::testing::three_page_pdf()).expect("write the pdf");
    path
});

fn junk_film() -> PathBuf {
    let path = DIRECTORY.join("Broken.webm");
    std::fs::write(&path, (0..5000u32).map(|n| (n.wrapping_mul(2654435761) >> 13) as u8).collect::<Vec<u8>>()).expect("write junk");
    path
}

fn require_host() {
    assert!(host_available(), "build packages/bingux-preview-host first: the helper is not beside {:?}", std::env::current_exe());
    // The tests run side by side in one process, and each starts a helper: the
    // default of two would end one test's helper to make room for another's.
    super::allow_helpers(32);
}

fn open(path: &Path, mode: Mode, size: (f32, f32)) -> Snapshot {
    require_host();
    let source = PreviewSource::path(path.to_owned());
    Snapshot::new(move || Stage { source: source.clone(), mode }.into_element(), size)
}

/// A window with the theme provided, as an application has it, around a preview.
#[derive(PartialEq)]
struct Stage {
    source: PreviewSource,
    mode: Mode,
}

impl Component for Stage {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::theme::provide_tokens();
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg)])
            .child(crate::shell::OverlayHost::new(PreviewHost::new(Some(self.source.clone()), self.mode, PreviewNav::new(|_| {}))))
    }
}

fn shot_to(shot: &mut Snapshot, name: &str) {
    if let Some(dir) = std::env::var_os("BINGUX_PREVIEW_SHOTS") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("a shots directory");
        shot.save(dir.join(format!("{name}.png")));
    }
}

/// The text of every label in the window.
fn labels(shot: &mut Snapshot) -> Vec<String> {
    shot.runner().find_many(|_, element| match Label::try_downcast(element) {
        Some(label) => Some(label.text.to_string()),
        None => Paragraph::try_downcast(element).map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect()),
    })
}

/// How many pictures of `width` pixels or less are drawn (a video frame, a page).
fn pictures(shot: &mut Snapshot) -> Vec<(i32, i32)> {
    shot.runner().find_many(|_, element| Image::try_downcast(element).map(|image| (image.image_handle.image.width(), image.image_handle.image.height())))
}

/// Settle until `done`, up to [`PATIENCE`]; panics with `what` when it never is.
fn until(shot: &mut Snapshot, what: &str, done: impl Fn(&mut Snapshot) -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        shot.settle(Duration::from_millis(150));
        if done(shot) {
            return;
        }
    }
    panic!("{what}: never happened; labels {:?}, pictures {:?}", labels(shot), pictures(shot));
}

/// The clock label that shows the position, "0:01" and so on: the first label
/// of that shape.
fn position(shot: &mut Snapshot) -> Option<String> {
    labels(shot).into_iter().find(|text| text.len() <= 7 && text.contains(':') && text.chars().all(|c| c.is_ascii_digit() || c == ':'))
}

fn seconds(text: &str) -> u32 {
    text.split(':').fold(0, |total, part| total * 60 + part.parse::<u32>().unwrap_or(0))
}

#[test]
fn a_film_in_a_popup_plays_pauses_and_seeks() {
    let mut shot = open(&CLIP, Mode::Popup, POPUP);
    until(&mut shot, "the first picture", |shot| !pictures(shot).is_empty());
    until(&mut shot, "the bar's clock", |shot| position(shot).is_some());
    // A popup plays at once: the clock moves.
    until(&mut shot, "playback", |shot| position(shot).is_some_and(|text| seconds(&text) >= 1));
    shot_to(&mut shot, "player-popup-playing");

    // K pauses (Space closes a popup): the clock stops.
    shot.key(Key::Character("k".into()));
    shot.settle(Duration::from_millis(700));
    let paused_at = position(&mut shot).expect("a clock");
    shot.settle(Duration::from_millis(1200));
    assert_eq!(position(&mut shot).as_deref(), Some(paused_at.as_str()), "paused: the clock does not move");
    shot_to(&mut shot, "player-popup-paused");

    // 0 goes to the start, Shift+Right seeks forward five seconds, which is
    // past the end of a three second film: it lands on the end.
    shot.key(Key::Character("0".into()));
    until(&mut shot, "the seek to the start", |shot| position(shot).is_some_and(|text| seconds(&text) == 0));
    shot.key_with(Key::Named(NamedKey::ArrowRight), Modifiers::SHIFT);
    until(&mut shot, "the seek to the end", |shot| position(shot).is_some_and(|text| seconds(&text) >= 2));
    shot_to(&mut shot, "player-popup-seeked");
}

#[test]
fn a_docked_pane_shows_the_first_frame_and_waits_to_be_asked() {
    let mut shot = open(&CLIP, Mode::Pane, PANE);
    until(&mut shot, "the poster frame", |shot| !pictures(shot).is_empty());
    until(&mut shot, "the compact bar", |shot| position(shot).is_some());
    shot.settle(Duration::from_millis(1800));
    assert_eq!(position(&mut shot).as_deref(), Some("0:00"), "a pane never plays by itself");
    shot_to(&mut shot, "player-pane-poster");
    // A click on the picture plays; it is the same stage press as in a popup.
    shot.click(PANE.0 / 2.0, 200.0);
    until(&mut shot, "playback after a click", |shot| position(shot).is_some_and(|text| seconds(&text) >= 1));
    shot_to(&mut shot, "player-pane-playing");
}

#[test]
fn the_phone_sheet_fits_the_player() {
    let mut shot = open(&CLIP, Mode::Popup, PHONE);
    until(&mut shot, "the first picture", |shot| !pictures(shot).is_empty());
    until(&mut shot, "the bar", |shot| position(shot).is_some());
    shot.settle(Duration::from_millis(600));
    let sizes = pictures(&mut shot);
    assert!(sizes.iter().all(|(w, _)| *w as f32 <= PHONE.0 * 2.0), "the frame fits the sheet: {sizes:?}");
    shot_to(&mut shot, "player-phone");
}

#[test]
fn a_song_is_its_card_with_the_same_bar_and_plays() {
    let mut shot = open(&SONG, Mode::Popup, POPUP);
    until(&mut shot, "the bar", |shot| position(shot).is_some());
    assert!(pictures(&mut shot).iter().all(|(w, _)| *w < 300), "a song has no video frame: {:?}", pictures(&mut shot));
    until(&mut shot, "playback", |shot| position(shot).is_some_and(|text| seconds(&text) >= 1));
    assert!(labels(&mut shot).iter().any(|text| text == "Tone.wav"), "the card names the file: {:?}", labels(&mut shot));
    shot_to(&mut shot, "player-audio");
}

#[test]
fn a_file_the_player_cannot_open_is_its_card_with_the_reason() {
    let mut shot = open(&junk_film(), Mode::Popup, POPUP);
    until(&mut shot, "the error", |shot| labels(shot).iter().any(|text| text.starts_with("Playback failed") || text.contains("cannot be played") || text.contains("damaged") || text.contains("no decoder")));
    assert!(pictures(&mut shot).iter().all(|(w, _)| *w < 300), "no video frame");
    assert!(position(&mut shot).is_none(), "no bar over a failure");
    shot_to(&mut shot, "player-error");
}
