//! The tooltip bubble's size, whatever it hangs from.
//!
//! A bubble is as wide as its text on one line, up to `Dim::TooltipMax` and
//! the window less its margins, and never as wide as the control it describes
//! or the box that control sits in. Every case here puts a short control (a
//! 44 px icon button) in a place that is narrower than the text: a clipped box,
//! a scrolled box, a tab strip, a window corner, a window narrower than the
//! text. It hovers the control and measures the bubble that is drawn, and
//! reads the pixels to see that the bubble is painted.
//!
//! Each case runs twice. The second time the same text has already been laid
//! out at a narrow width in the same window (the text cache's "poison"), so a
//! paragraph kept from that layout would show here as a bubble of the narrow
//! width.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;

use super::{bubble, hover_tip};
use crate::component::ComponentName;
use crate::controls::icon_button;
use crate::controls::tabs::{TabItem, tab_strip};
use crate::icons::Icon;
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const WINDOW: (f32, f32) = (640.0, 480.0);
const FRAME: Duration = Duration::from_millis(16);
/// The window's edge margin the placement keeps (`tooltip_place`).
const EDGE: f32 = 8.0;
const LABELS: [&str; 5] = [
    "New tab",
    "Refresh (F5)",
    "Move to Trash",
    "Copies the selected files and folders into the folder open in the other tab",
    "Zur\u{fc}ck \u{b7} \u{dc}bersicht \u{b7} \u{9589}\u{3058}\u{308b} \u{b7} \u{395}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3ac}",
];
/// About sixty characters: wider than the cap, so it wraps.
const SIXTY: &str = "Sixty characters of label text, written to need two or more";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Place {
    /// In the open: the control is 44 px wide and nothing else limits the bubble.
    Plain,
    /// In a 40 px box that clips what is outside it.
    Clipped,
    /// In a narrow scroll view that has been scrolled.
    Scrolled,
    /// The New tab button of the tab strip, pinned at the end.
    Strip,
    /// A 400 px control: the bubble is not limited by it.
    Wide,
    /// In the bottom-right corner of the window.
    Corner,
    /// A banner with an action and its dismiss cross, as wide as the window.
    Banner,
    /// The same banner floating as a toast.
    Toast,
}

/// What the window mounts for a case.
#[derive(Clone, PartialEq)]
struct Page {
    text: String,
    place: Place,
    /// Lay the text out at 20 px first (hidden), then show the case.
    poison: bool,
    window: (f32, f32),
}

impl Page {
    /// Where the case's control is.
    fn at(&self) -> (f32, f32) {
        match self.place {
            Place::Corner => (self.window.0 - 44.0 - EDGE, self.window.1 - 44.0 - EDGE),
            _ => (200.0_f32.min((self.window.0 - 44.0) / 2.0), 120.0),
        }
    }
}

/// The same text in a box far narrower than it: it wraps there.
fn narrow(text: &str, tokens: &crate::theme::Tokens, window: (f32, f32)) -> Element {
    rect()
        .position(Position::new_absolute().left(0.0).top(window.1 - 120.0))
        .width(Size::px(20.0))
        .opacity(0.0)
        .interactive(false)
        .child(bubble(tokens, text, false))
        .into_element()
}

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let button = || {
            icon_button(Icon::ListAdd, |_: ()| {})
                .size(Dim::Control)
                .tooltip(&self.text)
        };
        let (x, y) = self.at();
        let at = Position::new_absolute().left(x).top(y);
        let case: Element = match self.place {
            Place::Plain | Place::Corner => rect().position(at).child(button()).into_element(),
            Place::Wide => rect()
                .position(at)
                .child(hover_tip(
                    rect()
                        .width(Size::px(400.0))
                        .height(Size::px(44.0))
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt(self.text.clone()),
                    self.text.clone(),
                ))
                .into_element(),
            Place::Clipped => rect()
                .position(at)
                .width(Size::px(40.0))
                .height(Size::px(40.0))
                .overflow(Overflow::Clip)
                .child(button())
                .into_element(),
            Place::Scrolled => {
                let mut list = rect();
                for row in 0..12 {
                    list = list.child(rect().height(Size::px(40.0)).child(if row == 8 {
                        button().into_element()
                    } else {
                        rect().into_element()
                    }));
                }
                rect()
                    .position(at)
                    .width(Size::px(44.0))
                    .height(Size::px(200.0))
                    .child(ScrollView::new().child(list))
                    .into_element()
            }
            // The strip's own button: its tooltip is "New tab" whatever the case says.
            Place::Strip => rect()
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::px(self.window.0))
                .child(
                    tab_strip(vec![TabItem::new(1, "Home"), TabItem::new(2, "Documents")], 1)
                        .on_new(|_| {}),
                )
                .into_element(),
            // The banner's own dismiss cross, flush beside an Open button:
            // its tooltip is "Dismiss" whatever the case says.
            Place::Banner => rect()
                .position(Position::new_absolute().left(0.0).top(120.0))
                .width(Size::px(self.window.0))
                .padding(Gaps::new_all(16.0))
                .child(
                    crate::shell::Banner::new("Could not reach the network.")
                        .kind(crate::shell::BannerKind::Error)
                        .action("Open", |_: ()| {})
                        .dismissible(|_: ()| {}),
                )
                .into_element(),
            Place::Toast => ShowToast.into_element(),
        };
        let mut page = rect().expanded();
        if self.poison {
            page = page.child(narrow(&self.text, &tokens, self.window));
        }
        page.child(case)
    }
}

#[derive(PartialEq, Clone)]
struct ShowToast;

impl Component for ShowToast {
    fn render(&self) -> impl IntoElement {
        let overlays = crate::shell::use_overlays();
        use_hook(move || {
            overlays.show_toast(
                crate::shell::Toast::new("Could not reach the network.")
                    .kind(crate::shell::BannerKind::Error)
                    .action("Open", |_: ()| {}),
            );
        });
        rect()
    }
}

#[derive(PartialEq, Clone)]
struct Root(Page);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(OverlayHost::new(self.0.clone()))
    }
}

fn mount(page: Page) -> Snapshot {
    let window = page.window;
    Snapshot::new(move || Root(page.clone()).into_element(), window)
}

/// One drawn bubble: the box around the text, and the text's own box.
#[derive(Clone, Copy, Debug)]
struct Drawn {
    bubble: Area,
    label: Area,
}

/// The bubble drawn for `text`: the smallest box with an opacity of its own
/// around the label. `None` while there is none or it is still transparent.
fn drawn(shot: &mut Snapshot, text: &str) -> Option<Drawn> {
    let found = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        (label.text == text).then(|| node.layout().area)
    });
    found.into_iter().find_map(|label| {
        let centre = Area::center(&label);
        let around = shot.runner().find_many(|node, element| {
            let own = element.effect()?.opacity?;
            let area = node.layout().area;
            area.contains(centre).then(|| (area, own))
        });
        let (bubble, opacity) = around
            .into_iter()
            .min_by(|a, b| (a.0.width() * a.0.height()).total_cmp(&(b.0.width() * b.0.height())))?;
        (opacity > 0.99).then_some(Drawn { bubble, label })
    })
}

fn wait_for(shot: &mut Snapshot, text: &str) -> Drawn {
    for _ in 0..180 {
        shot.runner().poll_n(FRAME, 1);
        if let Some(found) = drawn(shot, text) {
            return found;
        }
    }
    panic!("no bubble was drawn for {text:?}");
}

/// The frame's pixels, for asking what is painted where.
struct Pixels {
    bytes: Vec<u8>,
    row: usize,
    width: usize,
    height: usize,
}

impl Pixels {
    fn of(shot: &mut Snapshot) -> Self {
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface = raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        Self { bytes, row, width: image.width() as usize, height: image.height() as usize }
    }

    fn at(&self, x: f32, y: f32) -> [u8; 3] {
        let x = (x.floor().max(0.0) as usize).min(self.width - 1);
        let y = (y.floor().max(0.0) as usize).min(self.height - 1);
        let at = y * self.row + x * 4;
        [self.bytes[at + 2], self.bytes[at + 1], self.bytes[at]]
    }
}

/// What the bubble of `text` measures when its control is wide (400 px), so
/// nothing around it limits it: the size it must have however it is hung. The
/// same window, fonts and text style as the cases, so only the place differs.
fn reference(text: &str) -> (f32, f32) {
    thread_local! {
        static KNOWN: RefCell<HashMap<String, (f32, f32)>> = RefCell::new(HashMap::new());
    }
    if let Some(known) = KNOWN.with(|known| known.borrow().get(text).copied()) {
        return known;
    }
    let mut shot = mount(Page { text: text.to_owned(), place: Place::Wide, poison: false, window: WINDOW });
    shot.settle(Duration::from_millis(300));
    shot.runner().move_cursor((400.0, 142.0));
    let found = wait_for(&mut shot, text);
    let size = (found.bubble.width(), found.bubble.height());
    KNOWN.with(|known| known.borrow_mut().insert(text.to_owned(), size));
    size
}

/// Hovers the case's control and checks the bubble that opens.
fn check(label: &str, place: Place, poison: bool, window: (f32, f32)) {
    let text = match place {
        Place::Strip => "New tab",
        Place::Banner | Place::Toast => "Dismiss",
        _ => label,
    };
    let name = format!("{place:?} {text:?} poison={poison} window={window:?}");
    let (free_width, free_height) = reference(text);
    let page = Page { text: label.to_owned(), place, poison, window };
    let (x, y) = page.at();
    let mut shot = mount(page);
    shot.settle(Duration::from_millis(300));
    if place == Place::Scrolled {
        // Row 8 is below the 200 px view: bring it in.
        shot.scroll(220.0, 200.0, -240.0);
        shot.settle(Duration::from_millis(400));
    }
    let anchor = if place == Place::Wide {
        Area::new((x, y).into(), (400.0, 44.0).into())
    } else {
        shot.icon_buttons()
            .into_iter()
            .find(|button| button.name == text)
            .unwrap_or_else(|| panic!("{name}: no button named {text:?}"))
            .area
    };
    let before = Pixels::of(&mut shot);
    shot.runner().move_cursor((anchor.center().x as f64, anchor.center().y as f64));
    let found = wait_for(&mut shot, text);
    let bubble = found.bubble;
    let room = Dim::TooltipMax.px().min(window.0 - 2.0 * EDGE);
    eprintln!(
        "{name}: anchor {:.1}x{:.1}, bubble {:.1}x{:.1} at ({:.1}, {:.1}), label {:.1}x{:.1}, free {free_width:.1}x{free_height:.1}, room {room:.1}",
        anchor.width(),
        anchor.height(),
        bubble.width(),
        bubble.height(),
        bubble.min_x(),
        bubble.min_y(),
        found.label.width(),
        found.label.height(),
    );
    let lines = ((found.label.height() / (free_height - 8.0)).round() as usize).max(1);
    if free_width <= room {
        // The text fits on one line: the bubble is exactly as wide as it, and
        // one line high.
        assert!(
            (bubble.width() - free_width).abs() <= 1.0,
            "{name}: the bubble is {:.1} wide, the text on one line is {free_width:.1}",
            bubble.width()
        );
        assert_eq!(lines, 1, "{name}: the text wrapped onto {lines} lines in a bubble {:.1} wide", bubble.width());
        assert!(
            (bubble.height() - free_height).abs() <= 1.0,
            "{name}: one line is {free_height:.1} high, the bubble is {:.1}",
            bubble.height()
        );
    } else {
        // Too long for one line: it wraps inside the room, on at most four
        // lines, with a line fuller than three characters.
        assert!(bubble.width() <= room + 0.5, "{name}: {:.1} wide, the room is {room:.1}", bubble.width());
        assert!(bubble.width() >= room * 0.5, "{name}: {:.1} wide is narrow for a long text", bubble.width());
        assert!((2..=4).contains(&lines), "{name}: {lines} lines");
        assert!(text.chars().count() / lines >= 3, "{name}: a column of letters");
    }
    assert!(
        bubble.min_x() >= EDGE - 0.5
            && bubble.min_y() >= EDGE - 0.5
            && bubble.max_x() <= window.0 - EDGE + 0.5
            && bubble.max_y() <= window.1 - EDGE + 0.5,
        "{name}: the bubble {bubble:?} leaves the {window:?} window"
    );
    assert!(!bubble.intersects(&anchor), "{name}: the bubble {bubble:?} covers its control {anchor:?}");

    // It is painted: just inside its left edge, where the padding is, the
    // pixel is the bubble's and not what was there before it opened.
    let after = Pixels::of(&mut shot);
    let probe = (bubble.min_x() + 3.0, bubble.min_y() + bubble.height() / 2.0);
    assert_ne!(
        after.at(probe.0, probe.1),
        before.at(probe.0, probe.1),
        "{name}: nothing is painted where the bubble is ({probe:?}); it is clipped or hidden"
    );
}

#[test]
fn a_bubble_is_as_wide_as_its_text_not_its_control() {
    for place in [Place::Wide, Place::Plain, Place::Clipped, Place::Scrolled, Place::Corner] {
        for label in LABELS.into_iter().chain([SIXTY]) {
            check(label, place, false, WINDOW);
        }
    }
    // The strip's own button says "New tab" whatever the label.
    check("New tab", Place::Strip, false, WINDOW);
    // A banner's and a toast's dismiss cross, flush beside an action, at the
    // desktop and phone widths, with and without a narrow layout before.
    for place in [Place::Banner, Place::Toast] {
        for poison in [false, true] {
            for window in [WINDOW, (390.0, 800.0)] {
                check("Dismiss", place, poison, window);
            }
        }
    }
}

#[test]
fn a_bubble_ignores_text_that_was_laid_out_narrow_before() {
    for place in [Place::Plain, Place::Clipped, Place::Scrolled, Place::Corner] {
        for label in LABELS.into_iter().chain([SIXTY]) {
            check(label, place, true, WINDOW);
        }
    }
    check("New tab", Place::Strip, true, WINDOW);
}

#[test]
fn a_window_narrower_than_the_text_caps_the_bubble_at_its_own_width() {
    // 220 px: less than the 280 px cap plus margins, more than a short label.
    for place in [Place::Plain, Place::Clipped] {
        for label in [LABELS[0], LABELS[3], SIXTY] {
            check(label, place, true, (220.0, 300.0));
        }
    }
}

/// A control named "New tab" whose bubble is drawn the way the bug drew it:
/// laid out in a 40 px room, so the text wraps to a column.
#[derive(Clone, PartialEq)]
struct Narrow;

impl Component for Narrow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut shown = use_state(|| false);
        rect()
            .expanded()
            .child(
                rect()
                    .position(Position::new_absolute().left(200.0).top(120.0))
                    .component("IconButton")
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("New tab")
                    .width(Size::px(44.0))
                    .height(Size::px(44.0))
                    .on_pointer_over(move |_| shown.set(true))
                    .on_pointer_out(move |_| shown.set(false)),
            )
            .maybe_child(shown().then(|| {
                rect()
                    .position(Position::new_absolute().left(200.0).top(170.0))
                    .width(Size::px(40.0))
                    .opacity(0.999)
                    .child(bubble(&tokens, "New tab", false))
            }))
    }
}

#[derive(PartialEq, Clone)]
struct NarrowRoot;

impl Component for NarrowRoot {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(OverlayHost::new(Narrow))
    }
}

#[test]
fn the_check_catches_the_bubble_the_bug_drew() {
    let mut shot = Snapshot::new(|| NarrowRoot.into_element(), WINDOW);
    shot.settle(Duration::from_millis(300));
    let report = shot.tooltip_findings();
    assert_eq!(report.controls, 1);
    assert_eq!(report.shown, 1, "the fixture's bubble is found: {report:?}");
    let rules: Vec<_> = report.findings.iter().map(|found| found.rule).collect();
    assert!(rules.contains(&"wrapped narrow"), "a column of letters is found: {report:?}");
}

#[test]
fn the_check_passes_every_control_of_the_cases() {
    for place in [Place::Plain, Place::Clipped, Place::Scrolled, Place::Strip, Place::Banner, Place::Toast] {
        let mut shot = mount(Page { text: LABELS[0].to_owned(), place, poison: false, window: WINDOW });
        shot.settle(Duration::from_millis(300));
        if place == Place::Scrolled {
            shot.scroll(220.0, 200.0, -240.0);
            shot.settle(Duration::from_millis(400));
        }
        let report = shot.tooltip_findings();
        assert!(report.shown >= 1, "{place:?}: no bubble was measured: {report:?}");
        assert!(report.findings.is_empty(), "{place:?}: {report:?}");
    }
}
