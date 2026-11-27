//! The list of every tab, rendered: what the last button of the strip opens
//! and how its rows answer.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use super::tabs::{TabItem, TabStatus, tab_strip};
use crate::component::ComponentName;
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};
use crate::window::WindowHost;

/// The room the strip has.
const STRIP: f32 = 900.0;

/// What the host was asked to do, in order: `select 3`, `close 4`, `mute 2`,
/// `search`, `close-duplicates`.
#[derive(Clone)]
struct Log(Rc<RefCell<Vec<String>>>);

impl PartialEq for Log {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Log {
    fn new() -> Self {
        Log(Rc::new(RefCell::new(Vec::new())))
    }

    fn push(&self, what: impl Into<String>) {
        self.0.borrow_mut().push(what.into());
    }

    fn all(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

#[derive(Clone, PartialEq)]
struct Spec {
    tabs: Vec<TabItem>,
    selected: u64,
    search: bool,
    duplicates: bool,
    log: Log,
}

impl Spec {
    /// `count` tabs titled `Page N`, the first `pinned` of them pinned.
    fn new(count: u64, pinned: u64, selected: u64) -> Self {
        Spec {
            tabs: (1..=count)
                .map(|id| TabItem::new(id, format!("Page {id}")).pinned(id <= pinned))
                .collect(),
            selected,
            search: true,
            duplicates: true,
            log: Log::new(),
        }
    }

    fn with_status(mut self, id: u64, status: TabStatus) -> Self {
        for tab in &mut self.tabs {
            if tab.id == id {
                tab.status = Some(status);
            }
        }
        self
    }
}

#[derive(PartialEq)]
struct Root(Spec);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let tokens = Tokens::new(DEFAULT_ACCENT, Scheme::PreferDark, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(OverlayHost::new(Host(self.0.clone())))
    }
}

#[derive(PartialEq)]
struct Host(Spec);

impl Component for Host {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let spec = self.0.clone();
        let mut tabs = use_state({
            let tabs = spec.tabs.clone();
            move || tabs
        });
        let mut selected = use_state(move || spec.selected);
        let (on_select, on_close, on_mute) =
            (self.0.log.clone(), self.0.log.clone(), self.0.log.clone());
        let mut strip = tab_strip(tabs.read().clone(), selected())
            .on_new(|_| {})
            .on_select(move |id: u64| {
                on_select.push(format!("select {id}"));
                selected.set(id);
            })
            .on_close(move |id: u64| {
                on_close.push(format!("close {id}"));
                tabs.write().retain(|tab| tab.id != id);
            })
            .on_status(move |id: u64| on_mute.push(format!("mute {id}")));
        if self.0.search {
            let log = self.0.log.clone();
            strip = strip.on_search_tabs("Ctrl+Shift+A", move |_| log.push("search"));
        }
        if self.0.duplicates {
            let log = self.0.log.clone();
            strip = strip.on_close_duplicates(move |_| log.push("close-duplicates"));
        }
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .child(
                rect()
                    .component("TabBar")
                    .tw(
                        &tokens,
                        &[w_full(), h(Dim::TabBar), bg(super::tabs::BAR), pl(S2)],
                    )
                    .child(rect().width(Size::px(STRIP)).child(strip)),
            )
    }
}

fn open_in(spec: Spec, size: (f32, f32)) -> Snapshot {
    let mut shot = Snapshot::new(move || Root(spec.clone()).into_element(), size);
    shot.settle(Duration::from_millis(400));
    shot
}

fn start(spec: Spec) -> Snapshot {
    open_in(spec, (1000.0, 700.0))
}

fn rest(shot: &mut Snapshot) {
    shot.settle(Duration::from_millis(500));
}

fn centre(area: &Area) -> (f32, f32) {
    (
        area.min_x() + area.width() / 2.0,
        area.min_y() + area.height() / 2.0,
    )
}

/// The area of the element of class `class` named `label`.
fn labelled(shot: &mut Snapshot, class: &'static str, label: &str) -> Option<Area> {
    let label = label.to_string();
    shot.runner()
        .find_many(|node, element| {
            let data = element.accessibility();
            (data.builder.class_name() == Some(class)
                && data.builder.label() == Some(label.as_str()))
            .then(|| node.layout().area)
        })
        .into_iter()
        .next()
}

/// The area of the icon button named `label` inside the list's row named `row`. A tab shows its
/// speaker and close button in the strip too, under the same names, so a button is found by its
/// place in the row and not by name alone.
fn in_row(shot: &mut Snapshot, row: &str, label: &str) -> Option<Area> {
    let row = lines(shot).into_iter().find(|(name, ..)| name == row)?.1;
    let label = label.to_string();
    shot.runner()
        .find_many(|node, element| {
            let data = element.accessibility();
            let area = node.layout().area;
            let (x, y) = centre(&area);
            (data.builder.class_name() == Some("IconButton")
                && data.builder.label() == Some(label.as_str())
                && (row.min_x()..=row.max_x()).contains(&x)
                && (row.min_y()..=row.max_y()).contains(&y))
            .then_some(area)
        })
        .into_iter()
        .next()
}

fn open_list(shot: &mut Snapshot) {
    let button = labelled(shot, "IconButton", "Search and manage tabs")
        .expect("the strip has the Search and manage tabs button");
    let at = centre(&button);
    shot.click(at.0, at.1);
    rest(shot);
}

/// The lines of the open list, top to bottom: the label and, for a tab's row,
/// whether it is the selected one.
fn lines(shot: &mut Snapshot) -> Vec<(String, Area, Option<bool>)> {
    let mut found: Vec<(String, Area, Option<bool>)> = shot.runner().find_many(|node, element| {
        let data = element.accessibility();
        matches!(
            data.builder.class_name(),
            Some("TabListRow" | "TabListHeader")
        )
        .then(|| {
            (
                data.builder.label().unwrap_or_default().to_string(),
                node.layout().area,
                data.builder.is_selected(),
            )
        })
    });
    found.sort_by(|a, b| a.1.min_y().total_cmp(&b.1.min_y()));
    found
}

fn labels(shot: &mut Snapshot) -> Vec<String> {
    lines(shot).into_iter().map(|(label, ..)| label).collect()
}

fn count(shot: &mut Snapshot, class: &'static str) -> usize {
    shot.runner()
        .find_many(|_, element| {
            (element.accessibility().builder.class_name() == Some(class)).then_some(())
        })
        .len()
}

#[test]
fn the_list_button_opens_the_commands_the_pinned_tabs_under_their_label_and_then_the_others() {
    let mut shot = start(Spec::new(5, 2, 4));
    assert!(
        lines(&mut shot).is_empty(),
        "the list is shut until asked for"
    );
    open_list(&mut shot);
    assert_eq!(
        labels(&mut shot),
        [
            "Search all tabs",
            "Close duplicate tabs",
            "Pinned",
            "Page 1",
            "Page 2",
            "Page 3",
            "Page 4",
            "Page 5",
        ],
        "commands, then the pinned tabs in strip order, then the rest"
    );
    let selected: Vec<String> = lines(&mut shot)
        .into_iter()
        .filter(|(.., selected)| *selected == Some(true))
        .map(|(label, ..)| label)
        .collect();
    assert_eq!(selected, ["Page 4"], "only the selected tab is ticked");
}

#[test]
fn a_command_the_host_does_not_offer_has_no_row() {
    let mut spec = Spec::new(3, 0, 1);
    spec.duplicates = false;
    let mut shot = start(spec.clone());
    open_list(&mut shot);
    assert_eq!(
        labels(&mut shot),
        ["Search all tabs", "Page 1", "Page 2", "Page 3"],
        "Close duplicate tabs is listed only when there are duplicates"
    );
    spec.search = false;
    let mut shot = start(spec);
    open_list(&mut shot);
    assert_eq!(labels(&mut shot), ["Page 1", "Page 2", "Page 3"]);
}

#[test]
fn choosing_a_tab_selects_it_and_closes_the_list() {
    let spec = Spec::new(4, 0, 1);
    let mut shot = start(spec.clone());
    open_list(&mut shot);
    let row = lines(&mut shot)
        .into_iter()
        .find(|(label, ..)| label == "Page 3")
        .expect("the row of Page 3")
        .1;
    let at = centre(&row);
    shot.click(at.0, at.1);
    rest(&mut shot);
    assert_eq!(spec.log.all(), ["select 3"]);
    assert!(lines(&mut shot).is_empty(), "the list closed");
}

#[test]
fn the_search_and_duplicate_rows_run_the_hosts_commands_and_close_the_list() {
    for (row, command) in [
        ("Search all tabs", "search"),
        ("Close duplicate tabs", "close-duplicates"),
    ] {
        let spec = Spec::new(3, 0, 1);
        let mut shot = start(spec.clone());
        open_list(&mut shot);
        let area = lines(&mut shot)
            .into_iter()
            .find(|(label, ..)| label == row)
            .unwrap_or_else(|| panic!("no {row} row"))
            .1;
        let at = centre(&area);
        shot.click(at.0, at.1);
        rest(&mut shot);
        assert_eq!(spec.log.all(), [command], "{row}");
        assert!(lines(&mut shot).is_empty(), "{row}: the list closed");
    }
}

#[test]
fn a_row_has_a_sound_button_only_while_its_tab_plays_or_is_muted_and_a_close_button_only_if_unpinned()
 {
    let spec = Spec::new(4, 1, 2)
        .with_status(1, TabStatus::Audio)
        .with_status(3, TabStatus::Muted)
        .with_status(4, TabStatus::Loading);
    let mut shot = start(spec);
    open_list(&mut shot);
    assert_eq!(
        count(&mut shot, "TabListSound"),
        2,
        "the playing tab and the muted one, not the loading one"
    );
    assert_eq!(
        count(&mut shot, "TabListClose"),
        0,
        "close buttons wait for the pointer"
    );
    let rows = lines(&mut shot);
    let row = |label: &str| rows.iter().find(|(name, ..)| name == label).unwrap().1;
    let at = centre(&row("Page 3"));
    shot.hover(at.0, at.1);
    rest(&mut shot);
    assert_eq!(
        count(&mut shot, "TabListClose"),
        1,
        "an unpinned row shows it under the pointer"
    );
    let at = centre(&row("Page 1"));
    shot.hover(at.0, at.1);
    rest(&mut shot);
    assert_eq!(
        count(&mut shot, "TabListClose"),
        0,
        "a pinned tab is closed from its own menu, not from the list"
    );
}

#[test]
fn the_sound_and_close_buttons_of_a_row_answer_for_that_tab_and_leave_the_list_open() {
    let spec = Spec::new(4, 0, 1).with_status(2, TabStatus::Audio);
    let mut shot = start(spec.clone());
    open_list(&mut shot);
    let sound = in_row(&mut shot, "Page 2", "Mute Page 2").expect("the speaker of Page 2");
    let at = centre(&sound);
    shot.click(at.0, at.1);
    rest(&mut shot);
    assert_eq!(spec.log.all(), ["mute 2"]);
    assert_eq!(
        lines(&mut shot).len(),
        6,
        "the list stays open: {:?}",
        labels(&mut shot)
    );
    let rows = lines(&mut shot);
    let at = centre(&rows.iter().find(|(name, ..)| name == "Page 4").unwrap().1);
    shot.hover(at.0, at.1);
    rest(&mut shot);
    let close = in_row(&mut shot, "Page 4", "Close Page 4").expect("the close button of Page 4");
    let at = centre(&close);
    shot.click(at.0, at.1);
    rest(&mut shot);
    assert_eq!(spec.log.all(), ["mute 2", "close 4"]);
    assert_eq!(
        labels(&mut shot),
        [
            "Search all tabs",
            "Close duplicate tabs",
            "Page 1",
            "Page 2",
            "Page 3"
        ],
        "the tab's row went and the list is still open"
    );
}

#[test]
fn a_press_on_the_strips_own_speaker_under_the_open_list_closes_the_list_and_mutes_nothing() {
    // Why the test above finds the buttons by their row and not by name alone: the strip's own
    // tab of Page 2 has a speaker of the same name, which comes first in the tree and lies under
    // the list's scrim. A press there is a press outside the list. Looking up "Mute Page 2" took
    // that one, nothing reached the host, and it read as a row that did not answer.
    let spec = Spec::new(4, 0, 1).with_status(2, TabStatus::Audio);
    let mut shot = start(spec.clone());
    open_list(&mut shot);
    let named = shot.runner().find_many(|node, element| {
        let data = element.accessibility();
        (data.builder.class_name() == Some("IconButton")
            && data.builder.label() == Some("Mute Page 2"))
        .then(|| node.layout().area)
    });
    assert_eq!(
        named.len(),
        2,
        "the strip's speaker and the row's: {named:?}"
    );
    let in_strip = named[0];
    assert!(
        in_strip.min_y() < lines(&mut shot)[0].1.min_y(),
        "the first is the strip's, above the list: {in_strip:?}"
    );
    let at = centre(&in_strip);
    shot.click(at.0, at.1);
    rest(&mut shot);
    assert!(
        spec.log.all().is_empty(),
        "outside the list: {:?}",
        spec.log.all()
    );
    assert!(
        lines(&mut shot).is_empty(),
        "a press outside closes the list"
    );
}

#[test]
fn a_rows_sound_and_close_buttons_are_named_for_their_tab() {
    let spec = Spec::new(4, 0, 1).with_status(2, TabStatus::Audio);
    let mut shot = start(spec);
    open_list(&mut shot);
    assert!(
        in_row(&mut shot, "Page 2", "Mute Page 2").is_some(),
        "the speaker of the playing tab names what it does"
    );
    let rows = lines(&mut shot);
    let at = centre(&rows.iter().find(|(name, ..)| name == "Page 4").unwrap().1);
    shot.hover(at.0, at.1);
    rest(&mut shot);
    assert!(
        in_row(&mut shot, "Page 4", "Close Page 4").is_some(),
        "the close button of the row under the pointer names its tab"
    );
}

#[test]
fn the_selected_tab_is_scrolled_to_the_middle_of_a_long_list_when_it_opens() {
    let mut shot = start(Spec::new(40, 0, 30));
    open_list(&mut shot);
    let rows = lines(&mut shot);
    let selected = rows
        .iter()
        .find(|(_, _, selected)| *selected == Some(true))
        .expect("the selected row is on screen")
        .1;
    let body = one_area(&mut shot, "PanelRows");
    let middle = body.min_y() + body.height() / 2.0;
    assert!(
        (centre(&selected).1 - middle).abs() <= Dim::MenuItem.px_in(SizeClass::Regular),
        "the selected row {selected:?} is at the middle of the list {body:?}"
    );
}

#[test]
fn a_list_taller_than_the_window_has_room_for_stops_at_the_window() {
    let mut shot = open_in(Spec::new(40, 0, 1), (1000.0, 500.0));
    open_list(&mut shot);
    let body = one_area(&mut shot, "PanelRows");
    assert!(
        body.max_y() <= 500.0,
        "the list ends inside the window: {body:?}"
    );
    assert!(body.height() > 200.0, "and still shows rows: {body:?}");
}

#[test]
fn the_commands_and_pinned_tabs_stay_in_view_over_forty_tabs_and_the_card_stops_short_of_the_window()
 {
    // Firefox's all-tabs panel keeps Search All Tabs and Close Duplicate Tabs where they are and
    // scrolls the tab list under them; the list opens on the selected tab, Page 30, which
    // scrolled them out of the window when the whole list was one scroll.
    let window = (1280.0, 800.0);
    let mut shot = open_in(Spec::new(40, 2, 30), window);
    open_list(&mut shot);
    let body = one_area(&mut shot, "PanelRows");
    let rows = lines(&mut shot);
    let area = |label: &str| {
        rows.iter()
            .find(|(name, ..)| name == label)
            .unwrap_or_else(|| panic!("no {label} row"))
            .1
    };
    for label in [
        "Search all tabs",
        "Close duplicate tabs",
        "Pinned",
        "Page 1",
        "Page 2",
    ] {
        let row = area(label);
        assert!(
            row.min_y() >= 0.0 && row.max_y() <= body.min_y() + 0.5,
            "{label} {row:?} is in the window and above the rows that scroll {body:?}"
        );
    }
    let selected = area("Page 30");
    assert!(
        selected.min_y() >= body.min_y() - 0.5 && selected.max_y() <= body.max_y() + 0.5,
        "Page 30 {selected:?} is the one the list scrolled to, inside {body:?}"
    );
    let card = body.max_y() - area("Search all tabs").min_y();
    assert!(
        card <= window.1 * 0.75,
        "the card, from its first row to the end of the rows, is at most three quarters of the window, not the whole of it: {card}"
    );
    // The wheel scrolls the tabs and leaves what is above them alone.
    let before = area("Search all tabs");
    let scrolled_from = area("Page 30");
    let over = centre(&body);
    shot.scroll(over.0, over.1, -200.0);
    rest(&mut shot);
    let rows = lines(&mut shot);
    let again = |label: &str| rows.iter().find(|(name, ..)| name == label).unwrap().1;
    assert_eq!(
        again("Search all tabs"),
        before,
        "the commands did not move"
    );
    assert_ne!(again("Page 30"), scrolled_from, "the tabs did");
}

#[test]
fn the_arrow_keys_walk_the_rows_and_enter_chooses_one() {
    let spec = Spec::new(4, 0, 1);
    let mut shot = start(spec.clone());
    open_list(&mut shot);
    // Down from the selected row's place: the first stop after it.
    shot.key(Key::Named(NamedKey::ArrowDown));
    shot.key(Key::Named(NamedKey::ArrowDown));
    rest(&mut shot);
    shot.key(Key::Named(NamedKey::Enter));
    rest(&mut shot);
    assert_eq!(spec.log.all(), ["select 3"]);
    assert!(lines(&mut shot).is_empty(), "Enter closed the list");
}

#[test]
fn escape_closes_the_list_and_chooses_nothing() {
    let spec = Spec::new(3, 0, 1);
    let mut shot = start(spec.clone());
    open_list(&mut shot);
    assert!(!lines(&mut shot).is_empty());
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    assert!(lines(&mut shot).is_empty());
    assert!(spec.log.all().is_empty());
}

fn one_area(shot: &mut Snapshot, class: &'static str) -> Area {
    shot.runner()
        .find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(class))
                .then(|| node.layout().area)
        })
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no {class} on screen"))
}
