//! The About window, headless: the card keeps one size on every page and at
//! both densities, Close sits where every header's does, each page says what
//! the application gave it and leaves out what it did not, and Escape closes
//! it. The details block is tested without a window in `details`.

use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use super::{AppInfo, Page, open_at};
use crate::shell::overlays::{OverlayHost, use_overlays};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const DESKTOP: (f32, f32) = (900.0, 700.0);
const PHONE: (f32, f32) = (390.0, 800.0);

fn complete() -> AppInfo {
    AppInfo::default()
        .name("Files")
        .id("bingux-files")
        .version("0.1.0")
        .description("Browse and organise files and folders")
        .website("https://github.com/kierandrewett/bingux")
        .issues("https://github.com/kierandrewett/bingux/issues")
        .licence("GPL-3.0-or-later")
        .licence_text("Bingux is licensed under the GNU General Public License,\nversion 3 or any later version.\n\nThe complete text is at https://www.gnu.org/licenses/gpl-3.0.html")
        .authors(["Kieran Drewett", "Someone Else"])
        .commit("444fb3b9f2ac")
        .built("2026-10-28")
        .third_party("Summary: MIT AND ISC\n\nab_glyph 0.2.32: Apache-2.0\naccesskit 0.24.1: Apache-2.0 OR MIT\n")
}

fn minimal() -> AppInfo {
    AppInfo::default().name("Notes").id("bingux-notes")
}

fn long_description() -> AppInfo {
    complete().description(
        "A file manager that keeps each tab's folder, history and scroll position, shows thumbnails and \
         previews, copies, moves, renames and trashes with undo, and treats drives and removable media \
         as places like any other, all in the same window as the file chooser other applications use.",
    )
}

#[derive(PartialEq)]
struct Open {
    info: AppInfo,
    page: Page,
}

impl Component for Open {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let (info, page) = (self.info.clone(), self.page);
        use_hook(move || open_at(overlays, info, page));
        rect()
    }
}

#[derive(PartialEq)]
struct Root {
    info: AppInfo,
    page: Page,
}

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(OverlayHost::new(Open { info: self.info.clone(), page: self.page }))
    }
}

fn opened(info: AppInfo, page: Page, size: (f32, f32)) -> Snapshot {
    let mut shot = Snapshot::new(move || Root { info: info.clone(), page }.into_element(), size);
    shot.settle(Duration::from_millis(900));
    shot
}

fn by_class(shot: &mut Snapshot, name: &'static str) -> Vec<Area> {
    shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area)
    })
}

/// The card: the About body fills it.
fn card(shot: &mut Snapshot) -> Option<Area> {
    by_class(shot, "About").into_iter().next()
}

/// Every text on screen: labels, and the spans of paragraphs (a value, a
/// description, the licence text).
fn labels(shot: &mut Snapshot) -> Vec<String> {
    shot.runner().find_many(|_, element| {
        Label::try_downcast(element).map(|label| label.text.to_string()).or_else(|| {
            Paragraph::try_downcast(element)
                .map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect())
        })
    })
}

/// Click the centre of the text `text` (a label, or a paragraph on a phone,
/// where a row's title may wrap). Returns whether it was on screen.
fn press(shot: &mut Snapshot, text: &str) -> bool {
    let at = shot
        .runner()
        .find_many(|node, element| {
            let shown: String = Label::try_downcast(element).map(|label| label.text.to_string()).or_else(|| {
                Paragraph::try_downcast(element)
                    .map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect())
            })?;
            (shown == text).then(|| {
                let area = node.layout().area;
                (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0)
            })
        })
        .into_iter()
        .next();
    let Some((x, y)) = at else { return false };
    shot.click(x, y);
    true
}

fn has(labels: &[String], text: &str) -> bool {
    labels.iter().any(|label| label == text)
}

fn key(shot: &mut Snapshot, key: Key, modifiers: Modifiers) {
    shot.key_with(key, modifiers);
    shot.settle(Duration::from_millis(500));
}

#[test]
fn the_card_keeps_one_size_on_every_page_at_both_densities() {
    for (size, label) in [(DESKTOP, "desktop"), (PHONE, "phone")] {
        let mut shot = opened(complete(), Page::Main, size);
        let main = card(&mut shot).unwrap_or_else(|| panic!("{label}: the About card is open"));
        assert!(main.width() <= size.0 && main.height() <= size.1, "{label}: the card fits the window: {main:?}");
        for door in ["Licence", "Credits", "Legal"] {
            assert!(press(&mut shot, door), "{label}: the {door} door is on the main page: {:?}", labels(&mut shot));
            shot.settle(Duration::from_millis(500));
            let page = card(&mut shot).unwrap_or_else(|| panic!("{label}: {door} keeps the card"));
            assert_eq!((page.width(), page.height()), (main.width(), main.height()), "{label}: {door} changed the card's size");
            key(&mut shot, Key::Named(NamedKey::ArrowLeft), Modifiers::ALT);
            let back = card(&mut shot).expect("the card after Back");
            assert_eq!((back.width(), back.height()), (main.width(), main.height()), "{label}: Back changed the card's size");
            assert!(has(&labels(&mut shot), "Copy details"), "{label}: Alt+Left returned to the main page from {door}");
        }
    }
}

#[test]
fn close_keeps_the_right_inset_of_every_header() {
    for (size, label) in [(DESKTOP, "desktop"), (PHONE, "phone")] {
        for page in [Page::Main, Page::Licence, Page::Credits, Page::Legal] {
            let mut shot = opened(complete(), page, size);
            let card = card(&mut shot).unwrap_or_else(|| panic!("{label} {page:?}: the card is open"));
            let inset = shot.runner().run_in(|| S2.px());
            let close = by_class(&mut shot, "IconButton")
                .into_iter()
                .max_by(|a, b| a.max_x().total_cmp(&b.max_x()))
                .expect("the Close button");
            let gap = card.max_x() - close.max_x();
            assert!(
                (gap - inset).abs() < 0.6,
                "{label} {page:?}: Close is {gap} px from the card's right edge, not {inset}"
            );
        }
    }
}

#[test]
fn a_complete_app_shows_its_links_doors_and_details() {
    let mut shot = opened(complete(), Page::Main, DESKTOP);
    let seen = labels(&mut shot);
    for expected in ["About", "Website", "Report an issue", "Licence", "Credits", "Legal", "Details", "App ID", "Build", "Copy details"] {
        assert!(has(&seen, expected), "missing {expected:?} in {seen:?}");
    }
    assert!(has(&seen, "github.com/kierandrewett/bingux"), "the website row shows its address");
}

#[test]
fn a_minimal_app_leaves_out_the_rows_it_has_no_facts_for() {
    let mut shot = opened(minimal(), Page::Main, DESKTOP);
    let seen = labels(&mut shot);
    for absent in ["Website", "Report an issue", "Build"] {
        assert!(!has(&seen, absent), "{absent:?} has nothing to show: {seen:?}");
    }
    for kept in ["Licence", "Credits", "Legal", "App ID", "Copy details"] {
        assert!(has(&seen, kept), "{kept:?} is always there: {seen:?}");
    }
}

#[test]
fn a_long_description_wraps_and_scrolls_inside_the_same_card() {
    let mut short = opened(complete(), Page::Main, PHONE);
    let mut long = opened(long_description(), Page::Main, PHONE);
    let (a, b) = (card(&mut short).expect("card"), card(&mut long).expect("card"));
    assert_eq!((a.width(), a.height()), (b.width(), b.height()), "the description's length must not resize the card");
}

#[test]
fn licence_credits_and_legal_say_what_the_app_gave_them() {
    let mut shot = opened(complete(), Page::Credits, DESKTOP);
    let seen = labels(&mut shot);
    for expected in ["Written by", "Kieran Drewett", "Someone Else", "Built with", "bingux-ui"] {
        assert!(has(&seen, expected), "Credits misses {expected:?}: {seen:?}");
    }

    let mut shot = opened(complete(), Page::Licence, DESKTOP);
    let seen = labels(&mut shot);
    assert!(has(&seen, "GPL-3.0-or-later"), "the licence page names the SPDX licence: {seen:?}");

    let mut shot = opened(complete(), Page::Legal, DESKTOP);
    let seen = labels(&mut shot);
    assert!(has(&seen, "Interface library") || has(&seen, "bingux-ui"), "Legal lists bingux-ui: {seen:?}");
    assert!(has(&seen, "MIT AND ISC"), "Legal shows the summary of the libraries' licences: {seen:?}");

    let mut bare = opened(minimal(), Page::Credits, DESKTOP);
    let bare_labels = labels(&mut bare);
    assert!(!has(&bare_labels, "Written by"), "no authors, no Written by group: {bare_labels:?}");
}

#[test]
fn escape_closes_the_window_from_any_page() {
    for page in [Page::Main, Page::Legal] {
        let mut shot = opened(complete(), page, DESKTOP);
        assert!(card(&mut shot).is_some(), "{page:?} is open");
        key(&mut shot, Key::Named(NamedKey::Escape), Modifiers::default());
        shot.settle(Duration::from_millis(600));
        assert!(card(&mut shot).is_none(), "Escape closed {page:?}");
    }
}

#[test]
fn the_shortcut_is_ctrl_shift_question_mark_and_nothing_nearby() {
    let event = |key: Key, modifiers: Modifiers| KeyboardEventData::new(key, Code::Slash, modifiers);
    let character = |text: &str| Key::Character(text.to_owned());
    let both = Modifiers::CONTROL | Modifiers::SHIFT;
    assert!(super::is_shortcut(&event(character("?"), both)));
    assert!(super::is_shortcut(&event(character("/"), both)));
    assert!(!super::is_shortcut(&event(character("?"), Modifiers::SHIFT)), "Shift alone types a question mark");
    assert!(!super::is_shortcut(&event(character("?"), Modifiers::CONTROL)));
    assert!(!super::is_shortcut(&event(character("?"), both | Modifiers::ALT)));
    assert!(!super::is_shortcut(&event(character("a"), both)));
}
