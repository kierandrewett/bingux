//! The arrow keys step the previewed file by exactly one, in the docked pane
//! and in Quick Look, over text, pictures and documents.
//!
//! The whole file view runs headless: a real list of real files, the real
//! preview wiring of `files/preview.rs`, and real clicks and key presses. A
//! person clicks a file, perhaps clicks into the pane, and presses Up or
//! Down; each press must move the selection one file, never skip one (the
//! list and the pane both stepping) and never stand still (neither does).
//!
//! The document rows need `bingux-preview-host` (see `preview::render_tests`).

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use bingux_files::{Entry, Kind, Listing, Selection};
use freya::engine::prelude::{EncodedImageFormat, raster_n32_premul};
use freya::prelude::*;

use super::view::FileView;
use crate::preview::{allow_helpers, host_available};
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (1200.0, 700.0);

/// One of each kind of viewer, twice over, in list order.
const NAMES: [&str; 6] = ["a.txt", "b.png", "c.pdf", "d.txt", "e.png", "f.pdf"];

static DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = std::env::temp_dir().join(format!("bingux-files-preview-steps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a fixture directory");
    for name in NAMES {
        let bytes = match name.rsplit('.').next() {
            Some("png") => picture(),
            Some("pdf") => bingux_preview_core::testing::three_page_pdf(),
            _ => (1..=200).map(|n| format!("{name} line {n}\n")).collect::<String>().into_bytes(),
        };
        std::fs::write(dir.join(name), bytes).expect("write a fixture");
    }
    dir
});

fn picture() -> Vec<u8> {
    let mut surface = raster_n32_premul((64, 48)).expect("a surface");
    surface.canvas().clear(Color::from_rgb(40, 110, 200));
    #[allow(deprecated)]
    let png = surface.image_snapshot().encode_to_data(EncodedImageFormat::PNG).expect("a PNG");
    png.as_bytes().to_vec()
}

/// The scene's selection, handed out so the test can read where it is.
#[derive(Clone, Default)]
struct Probe(Rc<Cell<Option<State<Selection>>>>);

impl PartialEq for Probe {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(PartialEq)]
struct Scene {
    pane: bool,
    owner: u64,
    probe: Probe,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        let level = use_state(|| 32u16);
        let selection = use_state(Selection::default);
        let pane = use_state({
            let open = self.pane;
            move || open
        });
        self.probe.0.set(Some(selection));
        let (listing, order) = use_hook(|| {
            let mut listing = Listing::new(DIRECTORY.clone());
            for name in NAMES {
                listing.entries.push(Entry::named(DIRECTORY.join(name), name.to_owned(), Kind::File, false, false));
            }
            listing.complete = true;
            (Arc::new(listing), Arc::new((0..NAMES.len()).collect::<Vec<usize>>()))
        });
        OverlayHost::new(
            FileView::new(listing, order, selection)
                .icon_px(level)
                .thumbnails(false)
                .autofocus(true)
                .preview_pane(pane)
                .remember_for(self.owner),
        )
    }
}

static NEXT_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7_000);

struct Stage {
    shot: Snapshot,
    probe: Probe,
}

fn stage(pane: bool) -> Stage {
    assert!(host_available(), "build services/preview-host first: the helper is not beside {:?}", std::env::current_exe());
    allow_helpers(32);
    let probe = Probe::default();
    let owner = NEXT_OWNER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scene = probe.clone();
    let mut shot = Snapshot::new(move || Scene { pane, owner, probe: scene.clone() }.into_element(), SIZE);
    shot.settle(Duration::from_millis(900));
    Stage { shot, probe }
}

impl Stage {
    /// The name of the file the selection's cursor is on.
    fn cursor(&mut self) -> Option<String> {
        let state = self.probe.0.get().expect("the scene rendered");
        self.shot
            .runner()
            .run_in(|| state.peek().cursor().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned()))
    }

    fn labels(&mut self) -> Vec<String> {
        self.shot.runner().find_many(|_, element| match Label::try_downcast(element) {
            Some(label) => Some(label.text.to_string()),
            None => Paragraph::try_downcast(element).map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect()),
        })
    }

    /// The list row of `name`: the centre of the label the list drew. The
    /// preview's title is a label too, so take the leftmost.
    fn row(&mut self, name: &str) -> (f32, f32) {
        let mut hits: Vec<(f32, f32)> = self.shot.runner().find_many(|node, element| {
            let text = match Label::try_downcast(element) {
                Some(label) => label.text.to_string(),
                None => Paragraph::try_downcast(element)?.spans.iter().map(|span| span.text.to_string()).collect(),
            };
            let centre = node.layout().area.center();
            (text == name).then_some((centre.x, centre.y))
        });
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        *hits.first().unwrap_or_else(|| panic!("no row {name:?}: {:?}", self.labels()))
    }

    fn click_row(&mut self, name: &str) {
        let (x, y) = self.row(name);
        self.shot.click(x, y);
        self.shot.settle(Duration::from_millis(250));
        assert_eq!(self.cursor().as_deref(), Some(name), "a click on a row selects it");
    }

    fn press(&mut self, named: NamedKey) {
        self.shot.key(Key::Named(named));
        self.shot.settle(Duration::from_millis(300));
    }

    /// Wait until the preview's title says `name`.
    fn shows(&mut self, name: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            self.shot.settle(Duration::from_millis(150));
            let titles = self.labels().iter().filter(|text| text.as_str() == name).count();
            // The list has the name once; the preview's title is a second one.
            if titles >= 2 {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "the preview never showed {name}: {:?}", self.labels());
        }
    }

    /// Down five times and Up five times: each press moves one file and the
    /// preview follows. The last Up at the top stays put.
    fn walk(&mut self, from: usize, what: &str) {
        assert_eq!(self.cursor().as_deref(), Some(NAMES[from]), "{what}: starting point");
        for at in from + 1..NAMES.len() {
            self.press(NamedKey::ArrowDown);
            assert_eq!(self.cursor().as_deref(), Some(NAMES[at]), "{what}: Down from {} must land on {}", NAMES[at - 1], NAMES[at]);
            self.shows(NAMES[at]);
        }
        self.press(NamedKey::ArrowDown);
        assert_eq!(self.cursor().as_deref(), Some(NAMES[NAMES.len() - 1]), "{what}: Down stays on the last file");
        for at in (0..NAMES.len() - 1).rev() {
            self.press(NamedKey::ArrowUp);
            assert_eq!(self.cursor().as_deref(), Some(NAMES[at]), "{what}: Up from {} must land on {}", NAMES[at + 1], NAMES[at]);
            self.shows(NAMES[at]);
        }
        self.press(NamedKey::ArrowUp);
        assert_eq!(self.cursor().as_deref(), Some(NAMES[0]), "{what}: Up stays on the first file");
    }
}

#[test]
fn in_the_docked_pane_each_press_steps_one_file_after_a_click_in_the_list() {
    let mut stage = stage(true);
    stage.click_row(NAMES[0]);
    stage.shows(NAMES[0]);
    stage.walk(0, "after a list click");
}

#[test]
fn in_the_docked_pane_each_press_steps_one_file_after_a_click_in_the_pane() {
    let mut stage = stage(true);
    stage.click_row(NAMES[0]);
    stage.shows(NAMES[0]);
    // Into the pane's body: the person clicked the preview.
    stage.shot.click(SIZE.0 - 140.0, 380.0);
    stage.shot.settle(Duration::from_millis(250));
    assert_eq!(stage.cursor().as_deref(), Some(NAMES[0]), "a click in the pane does not move the selection");
    stage.walk(0, "after a pane click");
}

#[test]
fn in_the_docked_pane_the_keys_come_back_to_the_list_and_still_step_one_file() {
    let mut stage = stage(true);
    stage.click_row(NAMES[2]);
    stage.shows(NAMES[2]);
    // Over a document, then back to the list by Escape, and by a click.
    stage.shot.click(SIZE.0 - 140.0, 380.0);
    stage.shot.settle(Duration::from_millis(250));
    stage.press(NamedKey::ArrowDown);
    assert_eq!(stage.cursor().as_deref(), Some(NAMES[3]));
    stage.press(NamedKey::Escape);
    stage.press(NamedKey::ArrowDown);
    assert_eq!(stage.cursor().as_deref(), Some(NAMES[4]), "after Escape the list steps, once");
    stage.click_row(NAMES[1]);
    stage.press(NamedKey::ArrowDown);
    assert_eq!(stage.cursor().as_deref(), Some(NAMES[2]), "after a click in the list the list steps, once");
    stage.press(NamedKey::ArrowUp);
    assert_eq!(stage.cursor().as_deref(), Some(NAMES[1]));
}

#[test]
fn in_quick_look_each_press_steps_one_file() {
    let mut stage = stage(false);
    stage.click_row(NAMES[0]);
    stage.press_space();
    stage.shows(NAMES[0]);
    stage.walk(0, "in Quick Look");
}

#[test]
fn in_quick_look_over_the_docked_pane_each_press_steps_one_file() {
    let mut stage = stage(true);
    stage.click_row(NAMES[0]);
    // The person clicked the pane, then opened Quick Look: the pane gives its keys up.
    stage.shot.click(SIZE.0 - 140.0, 380.0);
    stage.shot.settle(Duration::from_millis(250));
    stage.press_space();
    stage.walk(0, "in Quick Look over a docked pane");
}

#[test]
fn quick_look_hands_the_keyboard_back_to_the_list_when_it_closes() {
    let mut stage = stage(false);
    stage.click_row(NAMES[1]);
    stage.press_space();
    stage.shows(NAMES[1]);
    stage.press(NamedKey::ArrowDown);
    assert_eq!(stage.cursor().as_deref(), Some(NAMES[2]));
    stage.press(NamedKey::Escape);
    stage.shot.settle(Duration::from_millis(500));
    stage.press(NamedKey::ArrowDown);
    assert_eq!(stage.cursor().as_deref(), Some(NAMES[3]), "the list steps once after Quick Look closed");
}

impl Stage {
    fn press_space(&mut self) {
        self.shot.key(Key::Character(" ".to_owned()));
        self.shot.settle(Duration::from_millis(700));
    }
}
