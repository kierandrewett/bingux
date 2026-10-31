//! The address bar in the headless runner: its controls under a real pointer,
//! and that turning it into the typed-path field moves nothing.
//!
//! Every control of the bar (a crumb, the chevron after it, the `…` crumb, the
//! phone's folder name and its pencil) must paint a hover and a pressed fill
//! that differ from the resting fill and from each other. This is read from
//! the pixels the renderer produced, in both schemes.
//!
//! Entering the typed-path field is a swap of what is inside one box: the
//! box, its first icon, the x and the middle of the first text, and the
//! button at the end are compared by their layout rectangles before and
//! after, on a desktop and on a phone, in both schemes.

use std::path::PathBuf;
use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use torin::prelude::Area;

use super::address_bar::AddressBar;
use super::directory::Where;
use crate::inspector::snapshot::{Kind, Node, Snapshot as Tree};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};
use crate::window::WindowHost;

#[derive(Clone, Copy, PartialEq, Debug)]
struct Case {
    dark: bool,
    phone: bool,
}

/// What the bar shows, and whether it starts in the field with a text typed.
#[derive(Clone, PartialEq)]
struct Setup {
    at: Where,
    typed: Option<String>,
}

impl Setup {
    fn pics() -> Setup {
        Setup {
            at: Where::Folder(home().join("Pics")),
            typed: None,
        }
    }
}

#[derive(PartialEq)]
struct Root(Case, Setup);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let scheme = if self.0.dark {
            Scheme::PreferDark
        } else {
            Scheme::PreferLight
        };
        let tokens = Tokens::new(DEFAULT_ACCENT, scheme, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(Bar(self.1.clone()))
    }
}

#[derive(PartialEq)]
struct Bar(Setup);

fn home() -> PathBuf {
    PathBuf::from("/var/tmp/PathBar/h")
}

thread_local! {
    /// What the bar asked to open, in order.
    static OPENED: std::cell::RefCell<Vec<Where>> = const { std::cell::RefCell::new(Vec::new()) };
}

impl Component for Bar {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::surface::use_tokens();
        let starts = self.0.typed.clone();
        let editing = use_state({
            let starts = starts.is_some();
            move || starts
        });
        let draft = use_hook(move || {
            let mut draft = crate::controls::OmniDraft::create();
            draft.set_in_place(starts.unwrap_or_default());
            draft
        });
        let find = use_state(super::search::Find::default);
        let home = home();
        rect()
            .expanded()
            .tw(&tokens, &[p(S4)])
            .child(AddressBar::new(
                self.0.at.clone(),
                home,
                editing,
                draft,
                find,
                |to: Where| OPENED.with_borrow_mut(|opened| opened.push(to)),
                |_: PathBuf| {},
            ))
    }
}

fn open(case: Case) -> Snapshot {
    open_with(case, Setup::pics())
}

fn open_with(case: Case, setup: Setup) -> Snapshot {
    let size = if case.phone {
        (390.0, 160.0)
    } else {
        (800.0, 160.0)
    };
    let mut shot =
        Snapshot::with_scale(move || Root(case, setup.clone()).into_element(), size, 1.0);
    shot.settle(Duration::from_millis(300));
    shot
}

const CASES: [Case; 4] = [
    Case {
        dark: false,
        phone: false,
    },
    Case {
        dark: true,
        phone: false,
    },
    Case {
        dark: false,
        phone: true,
    },
    Case {
        dark: true,
        phone: true,
    },
];

fn tree(shot: &mut Snapshot) -> Tree {
    Tree::capture_headless(shot.runner(), 1.0)
}

/// With `BINGUX_PATHBAR_SHOTS=DIR` set, writes the frame as a PNG to look at.
fn keep(shot: &mut Snapshot, case: Case, what: &str) {
    let Some(dir) = std::env::var_os("BINGUX_PATHBAR_SHOTS") else {
        return;
    };
    let name = format!(
        "{}-{}-{}.png",
        if case.phone { "phone" } else { "desktop" },
        if case.dark { "dark" } else { "light" },
        what.replace(' ', "-")
    );
    shot.save(PathBuf::from(dir).join(name));
}

fn area(node: &Node) -> Area {
    node.area.expect("the node is laid out")
}

fn centre(area: Area) -> (f32, f32) {
    (
        area.min_x() + area.width() / 2.0,
        area.min_y() + area.height() / 2.0,
    )
}

/// What the bar is made of, as laid out.
#[derive(Debug)]
struct Layout {
    bar: Area,
    face: Area,
    icon: Area,
    text: Area,
    /// Where the first text's first glyph starts.
    text_x: f32,
    button: Option<Area>,
    /// What the first text says: the first crumb's name, or the field's text.
    words: String,
}

fn inside(node: &Node, outer: Area) -> bool {
    node.area.is_some_and(|a| {
        a.min_x() >= outer.min_x() - 0.5
            && a.max_x() <= outer.max_x() + 0.5
            && a.min_y() >= outer.min_y() - 0.5
            && a.max_y() <= outer.max_y() + 0.5
    })
}

/// The first node of `tree` that `pick` accepts, leftmost first.
fn leftmost<'a>(tree: &'a Tree, pick: impl Fn(&Node) -> bool) -> Option<&'a Node> {
    tree.nodes
        .iter()
        .filter(|node| node.area.is_some() && pick(node))
        .min_by(|a, b| area(a).min_x().total_cmp(&area(b).min_x()))
}

/// The layout of the bar now. The glyphs rasterise on workers and the bar is
/// read once they have a box, so a loaded machine gets a few more frames.
fn layout(shot: &mut Snapshot) -> Layout {
    for _ in 0..40 {
        if let Some(found) = try_layout(shot) {
            return found;
        }
        shot.settle(Duration::from_millis(100));
    }
    try_layout(shot).expect("the bar has a box, an icon and a text")
}

fn try_layout(shot: &mut Snapshot) -> Option<Layout> {
    let tree = tree(shot);
    let named = |name: &str| {
        tree.nodes
            .iter()
            .find(|node| node.a11y.component.as_deref() == Some(name))
    };
    let bar = area(named("AddressBar")?);
    // The face of the bar: the box that holds the crumbs, or the field.
    let face = ["AddressCompactBar", "AddressCrumbs", "TextField"]
        .iter()
        .find_map(|name| named(name))
        .map(area)?;
    // The first glyph: a square of icon size, whatever draws it.
    let icon = leftmost(&tree, |node| {
        inside(node, face)
            && area(node).width() >= 14.0
            && area(node).width() <= 20.0
            && (area(node).width() - area(node).height()).abs() < 0.5
    })
    .map(|node| area(node))?;
    let text_node = leftmost(&tree, |node| {
        inside(node, face)
            && matches!(node.kind, Kind::Label | Kind::Paragraph)
            && node.text.as_deref().is_some_and(|text| !text.is_empty())
    })?;
    // A paragraph starts its text after its own padding.
    let pad = text_node
        .props
        .as_ref()
        .map_or(0.0, |props| props.padding[3]);
    let text = area(text_node);
    let text_x = text.min_x() + pad;
    let button = tree
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("IconButton") && inside(node, bar))
        .map(|node| area(node))
        .next();
    let words = text_node.text.clone().unwrap_or_default();
    Some(Layout {
        bar,
        face,
        icon,
        text,
        text_x,
        button,
        words,
    })
}

/// Enters the typed-path field the way a person does: a press on a phone's
/// pencil, a press on the empty end of a desktop's trail.
fn enter(shot: &mut Snapshot, case: Case) {
    let before = layout(shot);
    let at = if case.phone {
        centre(before.button.expect("a phone's pencil"))
    } else {
        (
            before.face.max_x() - 12.0,
            before.face.min_y() + before.face.height() / 2.0,
        )
    };
    shot.click(at.0, at.1);
    shot.settle(Duration::from_millis(300));
    let names: Vec<String> = tree(shot)
        .nodes
        .iter()
        .filter_map(|node| node.a11y.component.clone())
        .collect();
    assert!(
        names.iter().any(|name| name == "LocationField"),
        "{case:?}: the field opened"
    );
}

fn leave(shot: &mut Snapshot, case: Case) {
    shot.key(Key::Named(NamedKey::Escape));
    shot.settle(Duration::from_millis(300));
    let names: Vec<String> = tree(shot)
        .nodes
        .iter()
        .filter_map(|node| node.a11y.component.clone())
        .collect();
    assert!(
        !names.iter().any(|name| name == "LocationField"),
        "{case:?}: Escape closed the field"
    );
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

fn same(problems: &mut Vec<String>, case: Case, what: &str, crumbs: Area, typed: Area) {
    if !(close(crumbs.min_x(), typed.min_x())
        && close(crumbs.min_y(), typed.min_y())
        && close(crumbs.width(), typed.width())
        && close(crumbs.height(), typed.height()))
    {
        problems.push(format!(
            "{case:?}: the {what} moved when the field opened: {crumbs:?} -> {typed:?}"
        ));
    }
}

#[test]
fn entering_the_field_moves_nothing() {
    let mut problems = Vec::new();
    for case in CASES {
        let mut shot = open(case);
        keep(&mut shot, case, "crumbs");
        let crumbs = layout(&mut shot);
        enter(&mut shot, case);
        let typed = layout(&mut shot);
        keep(&mut shot, case, "typed");
        same(&mut problems, case, "bar", crumbs.bar, typed.bar);
        same(&mut problems, case, "box", crumbs.face, typed.face);
        same(&mut problems, case, "first icon", crumbs.icon, typed.icon);
        if !close(crumbs.text_x, typed.text_x) {
            problems.push(format!(
                "{case:?}: the first text starts at x {} in crumbs and {} typed",
                crumbs.text_x, typed.text_x
            ));
        }
        let middle = |a: Area| a.min_y() + a.height() / 2.0;
        if (middle(crumbs.text) - middle(typed.text)).abs() > 1.0 {
            problems.push(format!(
                "{case:?}: the first text is centred at y {} in crumbs and {} typed",
                middle(crumbs.text),
                middle(typed.text)
            ));
        }
        match (crumbs.button, typed.button) {
            (Some(a), Some(b)) => same(&mut problems, case, "button", a, b),
            (None, None) if !case.phone => {}
            (a, b) => problems.push(format!(
                "{case:?}: the button at the end is {a:?} in crumbs and {b:?} typed"
            )),
        }
        // And back: nothing is left behind.
        leave(&mut shot, case);
        let back = layout(&mut shot);
        same(&mut problems, case, "bar", crumbs.bar, back.bar);
        same(&mut problems, case, "box", crumbs.face, back.face);
        same(&mut problems, case, "first icon", crumbs.icon, back.icon);
        if !close(crumbs.text_x, back.text_x) {
            problems.push(format!("{case:?}: the first text is not back where it was"));
        }
        eprintln!("LAYOUT {case:?}\n  crumbs {crumbs:?}\n  typed  {typed:?}");
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
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

    fn pixel(&self, x: f32, y: f32) -> [u8; 3] {
        let x = (x.round().max(0.0) as usize).min(self.width - 1);
        let y = (y.round().max(0.0) as usize).min(self.height - 1);
        let at = y * self.row + x * 4;
        [self.bytes[at + 2], self.bytes[at + 1], self.bytes[at]]
    }
}

/// A point of a control's face that no glyph is drawn on: its left edge, a
/// little in, at mid height.
fn bare(face: Area) -> (f32, f32) {
    (face.min_x() + 3.0, face.min_y() + face.height() / 2.0)
}

struct Control {
    name: &'static str,
    face: Area,
}

/// The controls of the bar: by component for the crumbs, and by the button for
/// the phone's pencil. The phone's name is the area of the folder face.
fn controls(shot: &mut Snapshot, case: Case) -> Vec<Control> {
    let tree = tree(shot);
    let by = |component: &str| -> Vec<Area> {
        tree.nodes
            .iter()
            .filter(|node| node.a11y.component.as_deref() == Some(component))
            .filter_map(|node| node.area)
            .collect()
    };
    let mut found = Vec::new();
    if case.phone {
        // The face is the child of the stop that fills the trail.
        let trail = tree
            .nodes
            .iter()
            .find(|node| node.a11y.component.as_deref() == Some("AddressCompact"))
            .expect("the trail");
        found.push(Control {
            name: "folder name",
            face: area(trail),
        });
        let pencil = by("IconButton").into_iter().next().expect("the pencil");
        found.push(Control {
            name: "pencil",
            face: pencil,
        });
    } else {
        let mut crumbs = by("AddressCrumb");
        crumbs.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()));
        let mut chevrons = by("AddressChevron");
        chevrons.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()));
        found.push(Control {
            name: "first crumb",
            face: crumbs[0],
        });
        found.push(Control {
            name: "last crumb",
            face: *crumbs.last().unwrap(),
        });
        found.push(Control {
            name: "first chevron",
            face: chevrons[0],
        });
    }
    found
}

#[test]
fn every_control_of_the_bar_has_a_hover_and_a_pressed_fill() {
    for case in CASES {
        let mut shot = open(case);
        let controls = controls(&mut shot, case);
        for control in controls {
            let at = bare(control.face);
            // Rest: the pointer is away.
            shot.hover(380.0, 150.0);
            shot.settle(Duration::from_millis(250));
            let rest = Frame::of(&mut shot).pixel(at.0, at.1);
            shot.hover(at.0, at.1);
            shot.settle(Duration::from_millis(250));
            let hover = Frame::of(&mut shot).pixel(at.0, at.1);
            keep(&mut shot, case, &format!("hover {}", control.name));
            shot.press(at);
            shot.settle(Duration::from_millis(250));
            let pressed = Frame::of(&mut shot).pixel(at.0, at.1);
            keep(&mut shot, case, &format!("pressed {}", control.name));
            // Release away from the control so nothing is activated.
            shot.move_to((380.0, 150.0));
            shot.release((380.0, 150.0));
            shot.settle(Duration::from_millis(250));
            let after = Frame::of(&mut shot).pixel(at.0, at.1);
            assert_ne!(
                rest, hover,
                "{case:?}: the {} paints no hover fill",
                control.name
            );
            assert_ne!(
                hover, pressed,
                "{case:?}: the {} paints no pressed fill",
                control.name
            );
            assert_ne!(
                rest, pressed,
                "{case:?}: the {} is pressed like it is at rest",
                control.name
            );
            assert_eq!(
                rest, after,
                "{case:?}: the {} stays lit after the pointer left",
                control.name
            );
        }
    }
}

/// The folder name's face stays lit while its sheet is open, as a dropdown's
/// button does, and the sheet closes on Escape.
#[test]
fn the_folder_name_stays_lit_while_its_sheet_is_open() {
    for dark in [false, true] {
        let case = Case { dark, phone: true };
        let mut shot = open(case);
        let face = controls(&mut shot, case)[0].face;
        let at = bare(face);
        shot.hover(380.0, 150.0);
        shot.settle(Duration::from_millis(250));
        let rest = Frame::of(&mut shot).pixel(at.0, at.1);
        let middle = centre(face);
        shot.click(middle.0, middle.1);
        shot.settle(Duration::from_millis(500));
        // The pointer goes away, so only the open sheet can keep the face lit.
        shot.hover(380.0, 150.0);
        shot.settle(Duration::from_millis(300));
        keep(&mut shot, case, "sheet open");
        let open = Frame::of(&mut shot).pixel(at.0, at.1);
        assert_ne!(
            rest, open,
            "{case:?}: the folder name is not lit while its sheet is open"
        );
        shot.key(Key::Named(NamedKey::Escape));
        shot.settle(Duration::from_millis(500));
        let closed = Frame::of(&mut shot).pixel(at.0, at.1);
        assert_eq!(
            rest, closed,
            "{case:?}: the folder name stays lit after its sheet closed"
        );
    }
}

#[test]
fn the_rings_of_both_modes_are_around_their_boxes() {
    for case in CASES {
        let mut shot = open(case);
        shot.key(Key::Named(NamedKey::Tab));
        shot.settle(Duration::from_millis(200));
        let probes = shot.focus_rings();
        assert!(
            !probes.is_empty(),
            "{case:?}: Tab puts a ring on the first control"
        );
        assert!(
            shot.ring_mismatches().is_empty(),
            "{case:?}: a ring is off its box in crumbs mode"
        );
        enter(&mut shot, case);
        assert!(
            !shot.focus_rings().is_empty(),
            "{case:?}: the field has a ring"
        );
        assert!(
            shot.ring_mismatches().is_empty(),
            "{case:?}: a ring is off its box in field mode"
        );
    }
}

// ---- the named places: Trash and Recent ------------------------------------

fn trash() -> PathBuf {
    bingux_files::ops::trash_folder().expect("a data home")
}

/// A named place, what its trail starts with, what the phone's one row says,
/// and what the field holds.
struct Named {
    what: &'static str,
    at: Where,
    first: &'static str,
    last: &'static str,
    field: String,
}

fn named_places() -> Vec<Named> {
    vec![
        Named {
            what: "trash",
            at: Where::Folder(trash()),
            first: "Trash",
            last: "Trash",
            field: "trash:///".into(),
        },
        Named {
            what: "trash-inside",
            at: Where::Folder(trash().join("Old folder")),
            first: "Trash",
            last: "Old folder",
            field: "trash:///Old folder/".into(),
        },
        Named {
            what: "recent",
            at: Where::Recent,
            first: "Recent",
            last: "Recent",
            field: "recent:///".into(),
        },
    ]
}

/// Every text drawn inside the bar, left to right.
fn words_in_bar(shot: &mut Snapshot) -> Vec<String> {
    let tree = tree(shot);
    let Some(bar) = tree
        .nodes
        .iter()
        .find(|node| node.a11y.component.as_deref() == Some("AddressBar"))
    else {
        return Vec::new();
    };
    let bar = area(bar);
    let mut found: Vec<(f32, String)> = tree
        .nodes
        .iter()
        .filter(|node| inside(node, bar) && matches!(node.kind, Kind::Label | Kind::Paragraph))
        .filter_map(|node| {
            Some((
                area(node).min_x(),
                node.text.clone().filter(|text| !text.is_empty())?,
            ))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, text)| text).collect()
}

#[test]
fn a_named_place_shows_its_name_in_the_trail_and_its_address_in_the_field_without_moving_anything()
{
    let mut problems = Vec::new();
    for case in CASES {
        for place in named_places() {
            let mut shot = open_with(
                case,
                Setup {
                    at: place.at.clone(),
                    typed: None,
                },
            );
            keep(&mut shot, case, &format!("{}-crumbs", place.what));
            let crumbs = layout(&mut shot);
            let expected = if case.phone { place.last } else { place.first };
            if crumbs.words != expected {
                problems.push(format!(
                    "{case:?} {}: the trail starts with {:?}, not {expected:?}",
                    place.what, crumbs.words
                ));
            }
            let all = words_in_bar(&mut shot).join("|");
            for raw in ["files", ".local", "share", "Trash/", "home"] {
                if all.contains(raw) {
                    problems.push(format!(
                        "{case:?} {}: the trail shows {raw:?}: {all}",
                        place.what
                    ));
                }
            }
            enter(&mut shot, case);
            keep(&mut shot, case, &format!("{}-typed", place.what));
            let typed = layout(&mut shot);
            if typed.words != place.field {
                problems.push(format!(
                    "{case:?} {}: the field holds {:?}, not {:?}",
                    place.what, typed.words, place.field
                ));
            }
            same(
                &mut problems,
                case,
                &format!("{} bar", place.what),
                crumbs.bar,
                typed.bar,
            );
            same(
                &mut problems,
                case,
                &format!("{} box", place.what),
                crumbs.face,
                typed.face,
            );
            same(
                &mut problems,
                case,
                &format!("{} first icon", place.what),
                crumbs.icon,
                typed.icon,
            );
            if !close(crumbs.text_x, typed.text_x) {
                problems.push(format!(
                    "{case:?} {}: the first text starts at x {} in the trail and {} in the field",
                    place.what, crumbs.text_x, typed.text_x
                ));
            }
            leave(&mut shot, case);
            let back = layout(&mut shot);
            same(
                &mut problems,
                case,
                &format!("{} box back", place.what),
                crumbs.face,
                back.face,
            );
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Opens the bar in its field with `text` typed and presses Enter.
fn submit(text: &str) -> Vec<Where> {
    OPENED.with_borrow_mut(Vec::clear);
    let case = CASES[0];
    let mut shot = open_with(
        case,
        Setup {
            at: Where::Folder(home().join("Pics")),
            typed: Some(text.to_owned()),
        },
    );
    shot.settle(Duration::from_millis(300));
    shot.key(Key::Named(NamedKey::Enter));
    // The check of a text that names a folder on the disk runs on a worker.
    for _ in 0..30 {
        shot.settle(Duration::from_millis(100));
        if OPENED.with_borrow(|opened| !opened.is_empty()) {
            break;
        }
    }
    OPENED.with_borrow(Clone::clone)
}

#[test]
fn enter_on_any_way_to_write_the_trash_opens_the_trash() {
    let real = trash();
    for text in [
        "trash:///".to_owned(),
        "trash://".to_owned(),
        "TRASH:".to_owned(),
        format!("{}/", real.display()),
        real.display().to_string(),
        format!("file://{}", real.display()),
        "Trash".to_owned(),
    ] {
        assert_eq!(submit(&text), vec![Where::Folder(real.clone())], "{text:?}");
    }
}

#[test]
fn enter_on_recent_opens_the_recent_files() {
    for text in ["recent:///", "recent://", "Recent", "recent:"] {
        assert_eq!(submit(text), vec![Where::Recent], "{text:?}");
    }
}

#[test]
fn enter_on_something_that_is_not_in_the_trash_opens_nothing() {
    // The complaint names it by its address, not by the bin's path.
    assert_eq!(submit("trash:///Nothing here"), Vec::<Where>::new());
}
