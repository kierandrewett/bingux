//! Every option row of an open dropdown shows its label.
//!
//! A phone sheet once opened with its fills drawn and every label missing. A
//! structural test could not see it (the labels were all in the tree), so this
//! one reads the frame the renderer produced: for each row on screen it takes
//! the row's own fill from the pixels beside the label, and asks that the
//! pixels *inside* the label's box differ from that fill by a readable
//! contrast, and that the ink the label was given does too. It also asks for
//! the label's text, its opacity and that no box above it is hidden.
//!
//! The lists are short and long (200 rows, scrolled end to end so that every
//! row is on screen once), at desktop (a popover) and phone (a sheet) density,
//! in both schemes, with and without icons, with the selection first or last,
//! and with a type-ahead query active.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use torin::prelude::Area;

use super::dropdown::dropdown;
use crate::component::ComponentName;
use crate::contrast::{AA_TEXT, ratio, ratio_opaque};
use crate::icons::Icon;
use crate::inspector::snapshot::{Kind, Node, Snapshot as Tree};
use crate::shell::panel::SHEET_FADE;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};
use crate::typography::{BODY, Ink};
use crate::window::WindowHost;

/// What a dropdown under test is made of.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Case {
    count: usize,
    icons: bool,
    /// Whether the selection is the last option, not the first.
    last: bool,
    dark: bool,
}

/// The window root: it provides the tokens of the case's scheme.
#[derive(PartialEq)]
struct Root(Case);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let scheme = if self.0.dark { Scheme::PreferDark } else { Scheme::PreferLight };
        let tokens = Tokens::new(DEFAULT_ACCENT, scheme, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(Host(self.0))
    }
}

#[derive(PartialEq)]
struct Host(Case);

/// The text of option `index`: distinct, of mixed length, with digits and a
/// word that starts with a letter a query can pick out.
fn label(index: usize) -> String {
    match index % 3 {
        0 => format!("Option {index:03}"),
        1 => format!("Option {index:03} with a longer name"),
        _ => format!("Setting {index:03} of the list"),
    }
}

fn options(count: usize) -> Vec<(String, String)> {
    (0..count).map(|index| (index.to_string(), label(index))).collect()
}

impl Component for Host {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let case = self.0;
        let selected = if case.last { case.count - 1 } else { 0 };
        let mut list = dropdown(options(case.count), selected.to_string(), |_: String| {})
            .width(Dim::DropdownMenu)
            .title("Choice");
        if case.icons {
            list = list.icons(vec![Icon::Bluetooth; case.count]);
        }
        rect().expanded().tw(&tokens, &[p(S4)]).child(list)
    }
}

/// The window root of the blank row.
#[derive(PartialEq)]
struct BlankRoot;

impl Component for BlankRoot {
    fn render(&self) -> impl IntoElement {
        let _ = crate::theme::provide_tokens();
        WindowHost::new(Blank)
    }
}

/// A row whose label is drawn in the colour of the row's own fill: what the
/// bug looked like. The check must flag it.
#[derive(PartialEq)]
struct Blank;

impl Component for Blank {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect().expanded().child(
            rect()
                .component("DropdownItem")
                .tw(&tokens, &[w_full(), h(Dim::MenuItem), bg(Paint::Text)])
                .child(BODY.in_ink(Ink::Text).label(&tokens, "Invisible")),
        )
    }
}

/// One frame as the renderer drew it.
struct Frame {
    bytes: Vec<u8>,
    row: usize,
    width: usize,
    height: usize,
}

impl Frame {
    fn of(shot: &mut Snapshot) -> Self {
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface =
            raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        Self {
            bytes,
            row,
            width: image.width() as usize,
            height: image.height() as usize,
        }
    }

    /// The pixel at a window point, opaque.
    fn pixel(&self, x: f32, y: f32) -> Color {
        let x = (x.round().max(0.0) as usize).min(self.width - 1);
        let y = (y.round().max(0.0) as usize).min(self.height - 1);
        let at = y * self.row + x * 4;
        // N32 on a little-endian machine is blue, green, red, alpha.
        colour_from_argb(255, self.bytes[at + 2], self.bytes[at + 1], self.bytes[at])
    }

    /// The highest contrast any pixel of `area` has against `fill`.
    fn best_contrast(&self, area: Area, fill: Color) -> f32 {
        let mut best = 1.0_f32;
        let (x0, x1) = (area.min_x().floor() as i32, area.max_x().ceil() as i32);
        let (y0, y1) = (area.min_y().floor() as i32, area.max_y().ceil() as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                best = best.max(ratio_opaque(self.pixel(x as f32, y as f32), fill));
            }
        }
        best
    }
}

/// The tree with its nodes by id, to walk up and down.
struct Frame2 {
    tree: Tree,
    by_id: HashMap<u64, usize>,
}

impl Frame2 {
    fn of(shot: &mut Snapshot) -> Self {
        let tree = Tree::capture_headless(shot.runner(), 1.0);
        let by_id = tree.nodes.iter().enumerate().map(|(at, node)| (node.id, at)).collect();
        Self { tree, by_id }
    }

    fn node(&self, id: u64) -> &Node {
        &self.tree.nodes[self.by_id[&id]]
    }

    /// The first text node under `id`.
    fn text_under(&self, id: u64) -> Option<&Node> {
        let node = self.node(id);
        if matches!(node.kind, Kind::Label | Kind::Paragraph) && node.text.is_some() {
            return Some(node);
        }
        node.children.iter().find_map(|child| self.text_under(*child))
    }

    /// The part of the window the node can be seen in: the window itself
    /// cut down by every ancestor that clips.
    fn visible(&self, id: u64, window: Area) -> Option<Area> {
        let mut seen = window;
        let mut up = self.node(id).parent;
        while let Some(parent) = up {
            let node = self.node(parent);
            if node.used.overflow == Some("clip")
                && let Some(area) = node.area
            {
                seen = seen.intersection(&area)?;
            }
            up = node.parent;
        }
        Some(seen)
    }

    /// The node's opacity with every ancestor's folded in.
    fn opacity(&self, id: u64) -> f32 {
        let mut product = 1.0;
        let mut at = Some(id);
        while let Some(id) = at {
            let node = self.node(id);
            product *= node.props.as_ref().and_then(|props| props.opacity).unwrap_or(1.0);
            at = node.parent;
        }
        product
    }

    /// Whether this node or an ancestor is hidden.
    fn hidden(&self, id: u64) -> bool {
        let mut at = Some(id);
        while let Some(id) = at {
            let node = self.node(id);
            if node.used.hidden {
                return true;
            }
            at = node.parent;
        }
        false
    }
}

/// What a row must show. `dim` rows are the type-ahead's non-matches: they
/// are drawn faint on purpose, but they are drawn.
struct Expect<'a> {
    labels: &'a [String],
    /// Whether a row is a dimmed non-match.
    dim: &'a dyn Fn(usize) -> bool,
    /// How much of the bottom of the viewport a fade covers.
    fade: f32,
    /// Whether the list may still be fading in: a row fainter than a settled
    /// one is then left to a later frame.
    fading_in: bool,
}

/// The faintest a dimmed row's label may be, against its fill.
const DIM_FLOOR: f32 = 1.8;
/// The least a pixel of a label must differ from its fill, whatever the ink:
/// text drawn at the nominal contrast reaches most of it in its strokes.
const PIXEL_FLOOR: f32 = 3.0;

/// Checks every `DropdownItem` fully on screen. Returns what it found wrong
/// and the indices (into `expect.labels`) of the rows it looked at.
fn check_rows(shot: &mut Snapshot, window: Area, expect: &Expect) -> (Vec<String>, BTreeSet<usize>) {
    let tree = Frame2::of(shot);
    let frame = Frame::of(shot);
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for node in tree.tree.nodes.iter().filter(|node| node.a11y.component.as_deref() == Some("DropdownItem")) {
        let Some(row) = node.area else { continue };
        let Some(text) = tree.text_under(node.id) else {
            problems.push(format!("a row at {row:?} has no text node"));
            continue;
        };
        let (Some(shown), Some(label)) = (tree.visible(node.id, window), text.area) else {
            continue;
        };
        let shown = Area::new(
            shown.origin,
            torin::prelude::Size2D::new(shown.width(), (shown.height() - expect.fade).max(0.0)),
        );
        // A row that is not wholly on screen is checked when it scrolls in.
        let inside = row.min_y() >= shown.min_y() - 0.5 && row.max_y() <= shown.max_y() + 0.5;
        if !inside {
            continue;
        }
        let name = text.text.clone().unwrap_or_default();
        let Some(index) = expect.labels.iter().position(|label| *label == name) else {
            problems.push(format!("a row at {row:?} shows {name:?}, which is no option"));
            continue;
        };
        seen.insert(index);
        let dim = (expect.dim)(index);
        let at = format!("row {index} ({name:?}) at {row:?}");
        if name.trim().is_empty() || label.width() < 4.0 || label.height() < 4.0 {
            problems.push(format!("{at}: the label is empty or has no size ({label:?})"));
            continue;
        }
        if tree.hidden(text.id) {
            problems.push(format!("{at}: the label or a box around it is hidden"));
            continue;
        }
        let opacity = tree.opacity(text.id);
        if expect.fading_in && !dim && opacity < 0.99 {
            seen.remove(&index);
            continue;
        }
        // A dimmed row is faint by the type-ahead's own rule (0.4), no more:
        // anything fainter is a row that has gone.
        let opacity_ok = if dim { (0.35..=0.45).contains(&opacity) } else { opacity >= 0.99 };
        if !opacity_ok {
            problems.push(format!("{at}: the label's opacity is {opacity:.2} (dim {dim})"));
        }
        // The row's fill, beside the label: the row is rounded, so a pixel
        // a few points in from its start edge at mid height.
        let fill = frame.pixel(row.min_x() + 3.0, row.min_y() + row.height() / 2.0);
        let ink = text.props.as_ref().and_then(|props| props.text_color);
        match ink {
            Some(ink) => {
                let ink = colour_from_argb(
                    (ink.a() as f32 * opacity) as u8,
                    ink.r(),
                    ink.g(),
                    ink.b(),
                );
                let nominal = ratio(ink, fill);
                let floor = if dim { DIM_FLOOR } else { AA_TEXT };
                if nominal < floor {
                    problems.push(format!(
                        "{at}: the ink is {nominal:.2}:1 against the row's fill {fill:?} (needs {floor})"
                    ));
                }
            }
            None => problems.push(format!("{at}: the label has no ink")),
        }
        let drawn = frame.best_contrast(label, fill);
        let floor = if dim { DIM_FLOOR } else { PIXEL_FLOOR };
        if drawn < floor {
            problems.push(format!(
                "{at}: the pixels in the label differ from the fill {fill:?} by only {drawn:.2}:1 (needs {floor}): nothing is drawn"
            ));
        }
    }
    (problems, seen)
}

/// How long a test waits for the list to open or close before it says it did not. The wait ends
/// the moment the list is there, so the cap costs nothing when the machine keeps up.
const WAIT: Duration = Duration::from_secs(30);

/// How many options the list shows now.
fn rows_on_screen(shot: &mut Snapshot) -> usize {
    Frame2::of(shot)
        .tree
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("DropdownItem"))
        .count()
}

/// Whether every row of the list is drawn at full strength: it has finished fading in.
fn rows_whole(shot: &mut Snapshot) -> bool {
    let tree = Frame2::of(shot);
    tree.tree
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("DropdownItem"))
        .all(|node| tree.text_under(node.id).is_none_or(|text| tree.opacity(text.id) >= 0.99))
}

struct Rig {
    shot: Snapshot,
    window: Area,
    phone: bool,
    case: Case,
    /// Whether a query has been typed: the list is then on its first match,
    /// not on the current choice.
    queried: bool,
}

impl Rig {
    fn new(case: Case, phone: bool) -> Self {
        let size = if phone { (390.0, 800.0) } else { (800.0, 700.0) };
        let mut shot = Snapshot::with_scale(move || Root(case).into_element(), size, 1.0);
        shot.settle(Duration::from_millis(300));
        let window = Area::new((0.0, 0.0).into(), size.into());
        Self { shot, window, phone, case, queried: false }
    }

    /// The button's centre: the dropdown component's box starts at it.
    fn button(&mut self) -> (f32, f32) {
        let tree = Frame2::of(&mut self.shot);
        let node = tree
            .tree
            .nodes
            .iter()
            .find(|node| node.a11y.role == "ComboBox")
            .expect("the dropdown's button");
        let area = node.area.expect("the button has an area");
        (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0)
    }

    /// Opens the list with a click on the button, or from the keyboard: Tab
    /// to the button and Enter.
    fn open(&mut self, by_key: bool) {
        if by_key {
            self.shot.key(Key::Named(NamedKey::Tab));
            self.shot.key(Key::Named(NamedKey::Enter));
        } else {
            let (x, y) = self.button();
            self.shot.click(x, y);
        }
        self.shot.settle(self.motion_wait());
        // The animation moves at most one tick cap a frame when the machine is busy, so a list
        // can take more frames to open than the clock says: wait until its rows are drawn whole.
        self.shot.settle_until(WAIT, rows_whole);
        let tree = Frame2::of(&mut self.shot);
        let named = |name: &str| {
            tree.tree.nodes.iter().filter(|node| node.a11y.component.as_deref() == Some(name)).count()
        };
        assert_eq!(named("DropdownItem"), self.case.count, "{:?}: the open list mounts every option", self.case);
        assert_eq!(
            named("Sheet"),
            usize::from(self.phone),
            "{:?}: a phone opens a sheet and a desktop a popover",
            self.case
        );
    }

    /// Long enough for the list to open or close: a phone's sheet takes
    /// `SLIDE_MS`, a desktop's popover far less, and the animation moves at
    /// most one tick cap a frame when the machine is busy.
    fn motion_wait(&self) -> Duration {
        Duration::from_millis(if self.phone { 1200 } else { 500 })
    }

    /// Escape: closes the list (or takes back its query) and lets it go.
    fn close(&mut self) {
        self.shot.key(Key::Named(NamedKey::Escape));
        self.shot.settle(self.motion_wait());
        self.shot.settle_until(WAIT, |shot| rows_on_screen(shot) == 0);
        let rows = rows_on_screen(&mut self.shot);
        assert_eq!(rows, 0, "{:?}: the list closed", self.case);
    }

    fn fade(&self) -> f32 {
        if self.phone { SHEET_FADE } else { 0.0 }
    }

    /// The scrolling list's box.
    fn list(&mut self) -> Area {
        let tree = Frame2::of(&mut self.shot);
        let rows: Vec<Area> = tree
            .tree
            .nodes
            .iter()
            .filter(|node| node.a11y.component.as_deref() == Some("DropdownItem"))
            .filter_map(|node| tree.visible(node.id, self.window))
            .collect();
        rows.into_iter()
            .min_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
            .expect("a visible list")
    }

    /// Checks the rows on screen now, then scrolls down a page at a time and
    /// checks again until the last row has been seen.
    fn check_every_row(&mut self, dim: &dyn Fn(usize) -> bool) -> Vec<String> {
        let labels: Vec<String> = (0..self.case.count).map(label).collect();
        let mut problems = Vec::new();
        let mut covered = BTreeSet::new();
        let list = self.list();
        // The list opens on the current choice: it is on screen, and drawn.
        let selected = if self.case.last { self.case.count - 1 } else { 0 };
        {
            let expect = Expect { labels: &labels, dim, fade: self.fade(), fading_in: false };
            let (found, seen) = check_rows(&mut self.shot, self.window, &expect);
            problems.extend(found);
            if !self.queried && !seen.contains(&selected) {
                problems.push(format!("the list opened without the choice (row {selected}) in view; it shows {seen:?}"));
            }
        }
        let row = self.shot.runner().run_in(|| Dim::MenuItem.px());
        let step = (list.height() - 2.0 * row - self.fade()).max(40.0);
        // Back to the top first: the list opens on the selection.
        for _ in 0..200 {
            self.shot.scroll(list.min_x() + 20.0, list.min_y() + 20.0, step);
            if self.at_top() {
                break;
            }
        }
        for _ in 0..400 {
            let expect = Expect { labels: &labels, dim, fade: self.fade(), fading_in: false };
            let (found, seen) = check_rows(&mut self.shot, self.window, &expect);
            problems.extend(found);
            covered.extend(seen);
            if covered.len() == self.case.count {
                break;
            }
            self.shot.scroll(list.min_x() + 20.0, list.min_y() + 20.0, -step);
            self.shot.settle(Duration::from_millis(32));
        }
        let missing: Vec<usize> = (0..self.case.count).filter(|index| !covered.contains(index)).collect();
        if !missing.is_empty() {
            problems.push(format!("rows {missing:?} never came on screen to be checked"));
        }
        problems.sort();
        problems.dedup();
        problems
    }

    /// Whether the first option is wholly on screen.
    fn at_top(&mut self) -> bool {
        let tree = Frame2::of(&mut self.shot);
        let first = label(0);
        tree.tree.nodes.iter().any(|node| {
            node.text.as_deref() == Some(first.as_str())
                && node.area.is_some_and(|area| {
                    tree.visible(node.id, self.window)
                        .is_some_and(|shown| area.min_y() >= shown.min_y() - 0.5 && area.max_y() <= shown.max_y() + 0.5)
                })
        })
    }
}

fn report(case: Case, phone: bool, what: &str, problems: &[String]) {
    assert!(
        problems.is_empty(),
        "{what} ({case:?}, phone {phone}): {} problems, the first:\n{}",
        problems.len(),
        problems.iter().take(6).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn dropdown_rows_show_their_labels_in_a_short_list() {
    for phone in [false, true] {
        for dark in [true, false] {
            for icons in [false, true] {
                let case = Case { count: 4, icons, last: false, dark };
                let mut rig = Rig::new(case, phone);
                rig.open(false);
                let problems = rig.check_every_row(&|_| false);
                report(case, phone, "short list", &problems);
            }
        }
    }
}

#[test]
fn dropdown_rows_show_their_labels_in_a_long_list() {
    for phone in [false, true] {
        for dark in [true, false] {
            for (icons, last) in [(false, false), (true, true)] {
                let case = Case { count: 200, icons, last, dark };
                let mut rig = Rig::new(case, phone);
                rig.open(false);
                let problems = rig.check_every_row(&|_| false);
                report(case, phone, "long list", &problems);
            }
        }
    }
}

#[test]
fn dropdown_rows_show_their_labels_while_a_query_is_typed() {
    // "Setting 0" picks out the rows whose label starts so: 002, 005, 008 and
    // the like. Every other row is dimmed, and still drawn.
    let query = "Setting 0";
    for phone in [false, true] {
        for dark in [true, false] {
            let case = Case { count: 200, icons: false, last: true, dark };
            let mut rig = Rig::new(case, phone);
            rig.open(false);
            for letter in query.chars() {
                rig.shot.key(Key::Character(letter.to_string()));
            }
            rig.queried = true;
            rig.shot.settle(Duration::from_millis(300));
            let matches = |index: usize| label(index).to_lowercase().starts_with(&query.to_lowercase());
            let problems = rig.check_every_row(&|index| !matches(index));
            report(case, phone, "list with a query", &problems);
        }
    }
}

#[test]
fn dropdown_rows_show_their_labels_when_opened_from_the_keyboard_and_again() {
    for phone in [false, true] {
        for last in [false, true] {
            let case = Case { count: 200, icons: true, last, dark: true };
            let mut rig = Rig::new(case, phone);
            rig.open(true);
            report(case, phone, "keyboard open", &rig.check_every_row(&|_| false));
            rig.close();
            rig.open(false);
            report(case, phone, "second open", &rig.check_every_row(&|_| false));
        }
    }
}

/// From the click until the list has settled, every frame draws the rows that
/// are on screen with their labels: no frame shows fills without text.
#[test]
fn dropdown_rows_are_never_drawn_without_their_labels_while_opening() {
    for phone in [false, true] {
        for dark in [true, false] {
            for count in [4, 200] {
                let case = Case { count, icons: false, last: true, dark };
                let mut rig = Rig::new(case, phone);
                let labels: Vec<String> = (0..count).map(label).collect();
                let (x, y) = rig.button();
                rig.shot.click(x, y);
                let mut problems = Vec::new();
                let mut frames_with_rows = 0;
                for frame in 0..45 {
                    rig.shot.settle(Duration::from_millis(16));
                    let expect = Expect { labels: &labels, dim: &|_| false, fade: rig.fade(), fading_in: true };
                    let (found, seen) = check_rows(&mut rig.shot, rig.window, &expect);
                    frames_with_rows += usize::from(!seen.is_empty());
                    problems.extend(found.into_iter().map(|found| format!("frame {frame}: {found}")));
                }
                assert!(frames_with_rows > 10, "{case:?}: only {frames_with_rows} frames had rows to check");
                report(case, phone, "opening frames", &problems);
            }
        }
    }
}

/// The check has teeth: a row whose label is the colour of its fill is
/// reported, and a sheet with its labels hidden is too.
#[test]
fn the_dropdown_row_check_flags_a_label_that_cannot_be_seen() {
    let mut shot = Snapshot::with_scale(|| BlankRoot.into_element(), (390.0, 200.0), 1.0);
    shot.settle(Duration::from_millis(300));
    let labels = vec!["Invisible".to_string()];
    let window = Area::new((0.0, 0.0).into(), (390.0, 200.0).into());
    let expect = Expect { labels: &labels, dim: &|_| false, fade: 0.0, fading_in: false };
    let (problems, seen) = check_rows(&mut shot, window, &expect);
    assert!(seen.contains(&0), "the row is on screen to be checked");
    assert!(
        problems.iter().any(|problem| problem.contains("nothing is drawn")),
        "the pixel check found {problems:?}"
    );
    assert!(
        problems.iter().any(|problem| problem.contains("against the row's fill")),
        "the ink check found {problems:?}"
    );
}

/// The window root of a device preview: the window is wide, and the density is
/// held at compact, so the page is a phone-wide column in its middle.
#[derive(PartialEq)]
struct PreviewRoot(Case);

impl Component for PreviewRoot {
    fn render(&self) -> impl IntoElement {
        use_hook(|| force_density(Platform::get().current_window().id, Some((SizeClass::Compact, PointerKind::Touch))));
        Root(self.0)
    }
}

/// In a device preview the dropdown's sheet rises from the bottom of the phone
/// column, not across the whole window.
#[test]
fn a_dropdown_sheet_in_a_device_preview_is_as_wide_as_the_phone_column() {
    let case = Case { count: 4, icons: false, last: false, dark: true };
    let size = (800.0, 700.0);
    let mut shot = Snapshot::with_scale(move || PreviewRoot(case).into_element(), size, 1.0);
    shot.settle(Duration::from_millis(300));
    let tree = Frame2::of(&mut shot);
    let button = tree
        .tree
        .nodes
        .iter()
        .find(|node| node.a11y.role == "ComboBox")
        .and_then(|node| node.area)
        .expect("the dropdown's button");
    shot.click(button.min_x() + button.width() / 2.0, button.min_y() + button.height() / 2.0);
    shot.settle(Duration::from_millis(500));
    let tree = Frame2::of(&mut shot);
    let slot = tree
        .tree
        .nodes
        .iter()
        .find(|node| node.a11y.component.as_deref() == Some("SheetSlot"))
        .and_then(|node| node.area)
        .expect("the sheet opened");
    let column = crate::theme::PHONE_PREVIEW_SIZE.0;
    assert!((slot.width() - column).abs() < 0.5, "the sheet is {} px wide, not the {column} px column", slot.width());
    assert!(
        (slot.min_x() - (size.0 - column) / 2.0).abs() < 0.5,
        "the sheet starts at {}, not at the column's edge",
        slot.min_x()
    );
}
