//! What a person sees and does in a preview, headless: the arrow keys choose
//! the file and never scroll, a document opens whole, the zoom steps and says
//! so, and every previewer stays inside its chrome budget.
//!
//! The viewers that read a PDF or a song need `bingux-preview-host` (see
//! `render_tests`); a text file and a picture do not.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bingux_preview_core::PreviewSource;
use freya::engine::prelude::{EncodedImageFormat, raster_n32_premul};
use freya::prelude::*;
use freya_core::elements::image::Image;
use torin::prelude::Area;

use super::{Mode, PreviewHost, PreviewNav, allow_helpers, host_available};
use crate::snapshot::Snapshot;
use crate::style::*;

const POPUP: (f32, f32) = (1100.0, 820.0);
const PANE: (f32, f32) = (380.0, 640.0);
const PATIENCE: Duration = Duration::from_secs(25);

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

static DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = std::env::temp_dir().join(format!("bingux-preview-behaviour-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a fixture directory");
    dir
});

fn fixture(name: &str, bytes: &[u8]) -> PathBuf {
    let path = DIRECTORY.join(name);
    std::fs::write(&path, bytes).expect("write a fixture");
    path
}

fn long_text() -> PathBuf {
    fixture("long.txt", (1..=400).map(|n| format!("line number {n}\n")).collect::<String>().as_bytes())
}

fn picture() -> PathBuf {
    let mut surface = raster_n32_premul((64, 48)).expect("a surface");
    surface.canvas().clear(Color::from_rgb(40, 110, 200));
    #[allow(deprecated)]
    let png = surface.image_snapshot().encode_to_data(EncodedImageFormat::PNG).expect("a PNG");
    fixture("blue.png", png.as_bytes())
}

fn letter_pdf() -> PathBuf {
    fixture("report.pdf", &bingux_preview_core::testing::three_page_pdf())
}

fn wide_pdf() -> PathBuf {
    fixture("wide.pdf", &bingux_preview_core::testing::pdf_sized((1190, 500), &[&[("Wide page one", 72, 400)], &[("Wide page two", 72, 400)]]))
}

fn tall_pdf() -> PathBuf {
    fixture("tall.pdf", &bingux_preview_core::testing::pdf_sized((400, 1600), &[&[("Tall page one", 72, 1500)], &[("Tall page two", 72, 1500)]]))
}

fn song() -> PathBuf {
    fixture("tone.wav", &bingux_preview_core::testing::wav_tone(3))
}

fn folder() -> PathBuf {
    let dir = DIRECTORY.join("a folder");
    std::fs::create_dir_all(&dir).expect("a folder");
    std::fs::write(dir.join("one.txt"), b"1").expect("a file");
    dir
}

fn blob() -> PathBuf {
    fixture("blob.dat", &[0, 1, 2, 3, 0, 0, 255, 254])
}

fn require_host() {
    assert!(host_available(), "build packages/bingux-preview-host first: the helper is not beside {:?}", std::env::current_exe());
    allow_helpers(32);
}

// ---------------------------------------------------------------------------
// The stage: a preview whose owner writes down what it was asked
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<String>>>);

impl PartialEq for Log {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Log {
    fn note(&self, text: impl Into<String>) {
        self.0.borrow_mut().push(text.into());
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

/// A preview in `mode`, with every handler of a file view's. `keys`: for a pane,
/// whether it holds the keyboard from the start.
#[derive(PartialEq)]
struct Stage {
    source: PreviewSource,
    mode: Mode,
    log: Log,
    grid: bool,
    keys: Option<bool>,
}

impl Component for Stage {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::theme::provide_tokens();
        let held = use_state(|| self.keys.unwrap_or(false));
        let log = self.log.clone();
        let mut nav = PreviewNav::new({
            let log = log.clone();
            move |_| log.note("close")
        })
        .dockable(true)
        .grid(self.grid)
        .on_step({
            let log = log.clone();
            move |step| log.note(format!("{step:?}"))
        })
        .on_open({
            let log = log.clone();
            move |_| log.note("open")
        })
        .on_properties({
            let log = log.clone();
            move |_| log.note("properties")
        })
        .on_mode({
            let log = log.clone();
            move |mode: Mode| log.note(format!("mode {mode:?}"))
        });
        if self.keys.is_some() {
            nav = nav.pane_keys(held).on_release({
                let log = log.clone();
                move |()| log.note("release")
            });
        }
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg)])
            .child(crate::shell::OverlayHost::new(PreviewHost::new(Some(self.source.clone()), self.mode, nav)))
    }
}

fn stage(path: &Path, mode: Mode, size: (f32, f32), log: &Log, keys: Option<bool>, grid: bool) -> Snapshot {
    let (source, log) = (PreviewSource::path(path.to_owned()), log.clone());
    Snapshot::new(move || Stage { source: source.clone(), mode, log: log.clone(), grid, keys }.into_element(), size)
}

fn with_host(path: &Path, mode: Mode, size: (f32, f32), log: &Log, keys: Option<bool>) -> Snapshot {
    require_host();
    stage(path, mode, size, log, keys, false)
}

// ---------------------------------------------------------------------------
// Looking
// ---------------------------------------------------------------------------

fn labels(shot: &mut Snapshot) -> Vec<String> {
    shot.runner().find_many(|_, element| match Label::try_downcast(element) {
        Some(label) => Some(label.text.to_string()),
        None => Paragraph::try_downcast(element).map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect()),
    })
}

/// Where the label whose whole text is `text` sits.
fn area_of(shot: &mut Snapshot, text: &str) -> Option<Area> {
    shot.runner().find_many(|node, element| {
        let plain: String = match Label::try_downcast(element) {
            Some(label) => label.text.to_string(),
            None => Paragraph::try_downcast(element)?.spans.iter().map(|span| span.text.to_string()).collect(),
        };
        (plain == text).then(|| node.layout().area)
    })
    .first()
    .copied()
}

/// Every picture drawn: where, and how many pixels it has.
fn pictures(shot: &mut Snapshot) -> Vec<Area> {
    shot.runner().find_many(|node, element| Image::try_downcast(element).map(|_| node.layout().area))
}

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

/// The zoom menu's label: "Fit page", "Fit width" or "125%".
fn zoom_label(shot: &mut Snapshot) -> Option<String> {
    labels(shot).into_iter().find(|text| text == "Fit page" || text == "Fit width" || (text.ends_with('%') && text[..text.len() - 1].chars().all(|c| c.is_ascii_digit())))
}

fn percent(label: &str) -> u32 {
    label.trim_end_matches('%').parse().expect("a percentage")
}

/// The page field: the first label that is a bare number.
fn page_number(shot: &mut Snapshot) -> Option<String> {
    labels(shot).into_iter().find(|text| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()))
}

fn key(shot: &mut Snapshot, named: NamedKey) {
    shot.key(Key::Named(named));
    shot.settle(Duration::from_millis(120));
}

fn arrows(shot: &mut Snapshot) {
    for named in [NamedKey::ArrowDown, NamedKey::ArrowDown, NamedKey::ArrowUp, NamedKey::ArrowRight, NamedKey::ArrowLeft] {
        key(shot, named);
    }
}

const ARROWS_AS_STEPS: [&str; 5] = ["Next", "Next", "Previous", "Next", "Previous"];

/// The first page (or frame) drawn, once there is one: a picture bigger than any icon.
fn first_picture(shot: &mut Snapshot) -> Option<Area> {
    pictures(shot).into_iter().find(|area| area.width() >= 100.0 && area.height() >= 100.0)
}

// ---------------------------------------------------------------------------
// Arrow keys
// ---------------------------------------------------------------------------

#[test]
fn the_arrows_step_the_file_and_never_scroll_a_text() {
    let log = Log::default();
    let mut shot = stage(&long_text(), Mode::Popup, POPUP, &log, None, false);
    until(&mut shot, "the first lines", |shot| area_of(shot, "line number 1").is_some());
    let before = area_of(&mut shot, "line number 1").expect("the first line");
    arrows(&mut shot);
    assert_eq!(log.take(), ARROWS_AS_STEPS, "the four arrows are Previous and Next in a list");
    shot.settle(Duration::from_millis(300));
    let after = area_of(&mut shot, "line number 1").expect("the first line is still on show");
    assert_eq!(before.min_y(), after.min_y(), "the text did not move");
}

#[test]
fn in_a_grid_up_and_down_step_a_line() {
    let log = Log::default();
    let mut shot = stage(&long_text(), Mode::Popup, POPUP, &log, None, true);
    until(&mut shot, "the first lines", |shot| area_of(shot, "line number 1").is_some());
    arrows(&mut shot);
    assert_eq!(log.take(), ["Down", "Down", "Up", "Next", "Previous"]);
}

#[test]
fn the_arrows_step_the_file_over_a_picture_and_a_folder_and_a_card() {
    for path in [picture(), folder(), blob()] {
        let log = Log::default();
        let mut shot = stage(&path, Mode::Popup, POPUP, &log, None, false);
        shot.settle(Duration::from_millis(900));
        arrows(&mut shot);
        assert_eq!(log.take(), ARROWS_AS_STEPS, "{path:?}");
    }
}

#[test]
fn the_arrows_step_the_file_over_a_document_and_never_scroll_it() {
    let log = Log::default();
    let mut shot = with_host(&letter_pdf(), Mode::Popup, POPUP, &log, None);
    until(&mut shot, "the first page", |shot| first_picture(shot).is_some());
    shot.settle(Duration::from_millis(400));
    let before = first_picture(&mut shot).expect("a page");
    arrows(&mut shot);
    assert_eq!(log.take(), ARROWS_AS_STEPS);
    shot.settle(Duration::from_millis(400));
    assert_eq!(first_picture(&mut shot).expect("a page").min_y(), before.min_y(), "the page did not move");
    assert_eq!(page_number(&mut shot).as_deref(), Some("1"));

    // Page Down is how a document scrolls.
    key(&mut shot, NamedKey::PageDown);
    shot.settle(Duration::from_millis(500));
    let scrolled = first_picture(&mut shot).map(|area| area.min_y());
    assert!(scrolled.is_none_or(|y| y < before.min_y()), "Page Down scrolled the document: {scrolled:?} against {}", before.min_y());
    assert!(log.take().is_empty(), "Page Down is not a step");
}

#[test]
fn the_arrows_step_the_file_over_a_song_and_do_not_seek_it() {
    let log = Log::default();
    let mut shot = with_host(&song(), Mode::Pane, PANE, &log, Some(true));
    until(&mut shot, "the bar", |shot| labels(shot).iter().any(|text| text == "0:00"));
    arrows(&mut shot);
    assert_eq!(log.take(), ARROWS_AS_STEPS);
    shot.settle(Duration::from_millis(700));
    assert!(labels(&mut shot).iter().any(|text| text == "0:00"), "a pane does not play, and an arrow does not seek: {:?}", labels(&mut shot));
}

// ---------------------------------------------------------------------------
// A pane's keyboard
// ---------------------------------------------------------------------------

#[test]
fn a_pane_that_was_not_clicked_leaves_every_key_to_the_list() {
    let log = Log::default();
    let mut shot = stage(&long_text(), Mode::Pane, PANE, &log, Some(false), false);
    until(&mut shot, "the first lines", |shot| area_of(shot, "line number 1").is_some());
    arrows(&mut shot);
    key(&mut shot, NamedKey::Escape);
    assert!(log.take().is_empty(), "the list has the keyboard: it steps the file itself");
}

#[test]
fn a_pane_that_was_clicked_steps_the_file_and_never_scrolls_its_text() {
    let log = Log::default();
    let mut shot = stage(&long_text(), Mode::Pane, PANE, &log, Some(true), false);
    until(&mut shot, "the first lines", |shot| area_of(shot, "line number 1").is_some());
    let before = area_of(&mut shot, "line number 1").expect("the first line");
    // The person clicked into the text: the scroll view under it has the focus.
    shot.click(PANE.0 / 2.0, 300.0);
    shot.settle(Duration::from_millis(200));
    log.take();
    arrows(&mut shot);
    assert_eq!(log.take(), ARROWS_AS_STEPS);
    let after = area_of(&mut shot, "line number 1").expect("the first line is still on show");
    assert_eq!(before.min_y(), after.min_y(), "the arrows did not scroll the text");
}

#[test]
fn escape_gives_a_pane_that_has_the_keyboard_back_to_the_list() {
    let log = Log::default();
    let mut shot = stage(&long_text(), Mode::Pane, PANE, &log, Some(true), false);
    until(&mut shot, "the first lines", |shot| area_of(shot, "line number 1").is_some());
    key(&mut shot, NamedKey::Escape);
    assert_eq!(log.take(), ["release"], "Escape hands the keyboard back; it does not close the pane");
}

#[test]
fn space_and_escape_close_a_popup_over_every_viewer() {
    for path in [long_text(), picture(), folder(), blob()] {
        for close in [Key::Character(" ".into()), Key::Named(NamedKey::Escape)] {
            let log = Log::default();
            let mut shot = stage(&path, Mode::Popup, POPUP, &log, None, false);
            shot.settle(Duration::from_millis(700));
            shot.key(close.clone());
            shot.settle(Duration::from_millis(100));
            assert_eq!(log.take(), ["close"], "{path:?} {close:?}");
        }
    }
}

#[test]
fn space_closes_a_film_or_song_popup_too() {
    let log = Log::default();
    let mut shot = with_host(&song(), Mode::Popup, POPUP, &log, None);
    until(&mut shot, "the bar", |shot| labels(shot).iter().any(|text| text.contains(':')));
    shot.key(Key::Character(" ".into()));
    shot.settle(Duration::from_millis(100));
    assert_eq!(log.take(), ["close"]);
}

// ---------------------------------------------------------------------------
// A document opens whole, and zooms by steps
// ---------------------------------------------------------------------------

/// The first page, drawn: whole inside the window below the toolbar, and in the
/// middle of it.
fn assert_whole_and_centred(shot: &mut Snapshot, what: &str, size: (f32, f32)) {
    let page = first_picture(shot).unwrap_or_else(|| panic!("{what}: no page is drawn"));
    let bar = area_of(shot, "of 3").or_else(|| area_of(shot, "of 2")).expect("the page count");
    assert!(page.min_y() >= bar.max_y(), "{what}: the page starts under the toolbar: page {page:?}, bar {bar:?}");
    assert!(page.max_y() <= size.1 + 0.5, "{what}: the whole page is inside the window: {page:?}");
    assert!(page.min_x() >= 0.0 && page.max_x() <= size.0 + 0.5, "{what}: the page is not wider than the window: {page:?}");
    let (left, right) = (page.min_x(), size.0 - page.max_x());
    assert!((left - right).abs() <= 2.0, "{what}: the page is centred across the window: {left} left, {right} right");
    // Large: the page uses most of the room it has.
    let room = (size.1 - bar.max_y()).min(size.0);
    assert!(page.height() >= room * 0.6 || page.width() >= size.0 * 0.6, "{what}: the page is large: {page:?}");
}

#[test]
fn a_document_opens_on_the_whole_first_page_in_a_popup_and_a_pane() {
    for (what, path) in [("letter", letter_pdf()), ("wide", wide_pdf()), ("tall", tall_pdf())] {
        for (mode, size) in [(Mode::Popup, POPUP), (Mode::Pane, PANE)] {
            let log = Log::default();
            let mut shot = with_host(&path, mode, size, &log, None);
            until(&mut shot, "the first page", |shot| first_picture(shot).is_some());
            shot.settle(Duration::from_millis(500));
            assert_eq!(zoom_label(&mut shot).as_deref(), Some("Fit page"), "{what} {mode:?}: opens fitted to the page");
            let page = first_picture(&mut shot).expect("page");
            assert!(page.min_y() > 0.0, "{what} {mode:?}: {page:?}");
            assert_whole_and_centred(&mut shot, &format!("{what} {mode:?}"), size);
        }
    }
}

#[test]
fn the_zoom_steps_and_the_label_says_where_it_is() {
    let log = Log::default();
    let mut shot = with_host(&letter_pdf(), Mode::Popup, POPUP, &log, None);
    until(&mut shot, "the first page", |shot| first_picture(shot).is_some());
    shot.settle(Duration::from_millis(500));
    assert_eq!(zoom_label(&mut shot).as_deref(), Some("Fit page"));

    let mut seen = Vec::new();
    for _ in 0..3 {
        shot.key(Key::Character("+".into()));
        shot.settle(Duration::from_millis(500));
        seen.push(percent(&zoom_label(&mut shot).expect("a zoom label after +")));
    }
    assert!(seen.windows(2).all(|pair| pair[1] > pair[0]), "each + goes up: {seen:?}");
    for step in &seen {
        assert!([10, 25, 50, 75, 100, 125, 150, 200, 300, 400, 500, 600, 800].contains(step), "{step} is one of the steps");
    }

    shot.key(Key::Character("-".into()));
    shot.settle(Duration::from_millis(500));
    assert_eq!(percent(&zoom_label(&mut shot).unwrap()), seen[1], "- goes back one step");
    shot.key(Key::Character("1".into()));
    shot.settle(Duration::from_millis(500));
    assert_eq!(zoom_label(&mut shot).as_deref(), Some("100%"));
    shot.key(Key::Character("0".into()));
    shot.settle(Duration::from_millis(500));
    assert_eq!(zoom_label(&mut shot).as_deref(), Some("Fit width"));
    shot.key(Key::Character("9".into()));
    shot.settle(Duration::from_millis(500));
    assert_eq!(zoom_label(&mut shot).as_deref(), Some("Fit page"));
    assert!(log.take().is_empty(), "the zoom keys are not steps or closes");
}

#[test]
fn the_zoom_buttons_and_the_zoom_menu_work_and_stay_in_one_place() {
    let log = Log::default();
    let mut shot = with_host(&letter_pdf(), Mode::Popup, POPUP, &log, None);
    until(&mut shot, "the first page", |shot| first_picture(shot).is_some());
    shot.settle(Duration::from_millis(500));
    let out = shot.runner().find_many(|node, element| Label::try_downcast(element).filter(|label| label.text == "Fit page").map(|_| node.layout().area));
    let menu_before = out.first().copied().expect("the menu label");

    // The menu: Fit page, then 150 %, from its rows.
    assert!(shot.click_text("Fit page"));
    shot.settle(Duration::from_millis(500));
    assert!(shot.click_text("150%"), "the menu lists 150%");
    shot.settle(Duration::from_millis(500));
    assert_eq!(zoom_label(&mut shot).as_deref(), Some("150%"));
    let menu_after = area_of(&mut shot, "150%").expect("the label");
    assert!((menu_after.center().x - menu_before.center().x).abs() <= 1.0, "the label keeps its slot as the text changes: {menu_before:?} {menu_after:?}");
}

// ---------------------------------------------------------------------------
// The chrome budget
// ---------------------------------------------------------------------------

/// The controls of a frame, by component: buttons, menus, sliders, fields,
/// switches.
fn controls(shot: &mut Snapshot) -> Vec<String> {
    const CONTROLS: [&str; 8] = ["IconButton", "IconMenuButton", "MenuButton", "ActionButton", "RowButton", "TrackBar", "TextField", "Checkbox"];
    let tree = crate::inspector::snapshot::Snapshot::capture_headless(shot.runner(), 1.0);
    tree.nodes.iter().filter_map(|node| node.a11y.component.clone()).filter(|name| CONTROLS.contains(&name.as_str())).collect()
}

/// The title line holds at most this many buttons: Open, Info, the mode
/// switch, Close.
const HEADER: usize = 4;

#[test]
fn a_previewer_stays_inside_its_chrome_budget() {
    // (what, path, content controls at rest). A picture shows its zoom pill only
    // under the pointer; a text, a folder and an archive show none; the generic
    // card offers Open; a document has the page field, zoom out, in, the zoom
    // menu and find; a song has play, the seek bar, mute, volume and speed.
    require_host();
    let cases: Vec<(&str, PathBuf, usize, Duration)> = vec![
        ("text", long_text(), 0, Duration::from_millis(800)),
        ("picture", picture(), 0, Duration::from_millis(900)),
        ("folder", folder(), 0, Duration::from_millis(800)),
        ("card", blob(), 1, Duration::from_millis(800)),
    ];
    for (what, path, content, wait) in cases {
        let log = Log::default();
        let mut shot = stage(&path, Mode::Popup, POPUP, &log, None, false);
        shot.settle(wait);
        let found = controls(&mut shot);
        assert!(found.len() <= HEADER + content, "{what}: {} controls at rest, budget {}: {found:?}", found.len(), HEADER + content);
    }
    for (what, path, content, ready) in [("document", letter_pdf(), 5usize, "of 3"), ("song", song(), 6, "0:00")] {
        let log = Log::default();
        let mut shot = with_host(&path, Mode::Popup, POPUP, &log, None);
        until(&mut shot, "the viewer's own bar", |shot| area_of(shot, ready).is_some() || labels(shot).iter().any(|text| text.starts_with("0:0")));
        shot.settle(Duration::from_millis(600));
        let found = controls(&mut shot);
        assert!(found.len() <= HEADER + content, "{what}: {} controls at rest, budget {}: {found:?}", found.len(), HEADER + content);
    }
}

#[test]
fn there_is_no_footer_and_no_second_title() {
    let log = Log::default();
    let mut shot = stage(&long_text(), Mode::Popup, POPUP, &log, None, false);
    until(&mut shot, "the first lines", |shot| area_of(shot, "line number 1").is_some());
    let names = labels(&mut shot).into_iter().filter(|text| text == "long.txt").count();
    assert_eq!(names, 1, "the file's name is on the screen once: {:?}", labels(&mut shot));
    assert!(labels(&mut shot).iter().all(|text| !text.contains("Modified") && !text.contains("Plain text")), "no footer facts and no language bar: {:?}", labels(&mut shot));
}
