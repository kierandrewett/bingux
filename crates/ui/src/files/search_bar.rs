//! The search bar of the file views: a slim bar under the toolbar with the
//! field, where to look, and the filters.
//!
//! ```text
//! [ search the folder ............ x ]  [ Subfolders v ]  [ Filters (2) ]
//! [ Type: Images x ] [ Size > 10 MB x ]  Clear all        Searching Pictures… 1,204 found  Stop
//! ```
//!
//! The first row is always the same: the library [`search_field`], a
//! dropdown for where to look ([`Scope`]) and the Filters button, which says
//! how many filters are set and opens the panel
//! ([`search_filters`](super::search_filters)). The second row says how the
//! search is going and, when filters are set, shows each as a token with its
//! own remove button and a Clear all. Nothing in either row changes size
//! while results stream in. A token row that wraps makes the second row
//! taller and pushes the list down; it never lies over the first row. On a
//! phone the field has a row to itself, the scope and the Filters button share
//! the next, the tokens wrap in a block under them and the status line is last.
//!
//! # Keys
//!
//! | Key | Does |
//! | --- | ---- |
//! | Ctrl+F, or a letter typed on the list | open the bar and focus the field; with the bar open, focus it |
//! | Ctrl+Shift+F | open or close the Filters panel |
//! | Enter | commit what is typed (a `key:value` still open becomes a filter), remember the search and open the selected result, or the first |
//! | Down | the first result is selected and the keyboard goes to the list |
//! | Up | the previous result, without leaving the field |
//! | Escape | clear the text; with no text, close the search and give back the selection from before |
//! | Escape in the list, or on the scope or Filters button | close the search (an open scope list closes first) |
//!
//! With the field empty and focused, the searches made lately drop down
//! under it (at most eight, newest first, with a row that forgets them all).
//! Arrows choose one, Enter or a press runs it.

use bingux_files::find::{FilterKey, Filters, Hidden, Modified, NameMode, Recents, Scope, SizeCond};
use bingux_files::{PlaceKind, Places, display};
use freya::prelude::*;
use std::path::{Path, PathBuf};

use super::address_list::{Lead, ListRow, Part, popover, scroll_offset};
use super::search::{Find, count};
use super::search_filters::{FiltersButton, kind_name};
use crate::component::ComponentName;
use crate::controls::{SpinnerSize, action_button, dropdown, icon_button, search_field, spinner};
use crate::icons::Icon;
use crate::shell::menu_query::Mark;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

// ---------------------------------------------------------------------------
// Where to look
// ---------------------------------------------------------------------------

/// One entry of the scope dropdown.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopeChoice {
    pub key: String,
    pub label: String,
    pub icon: Icon,
    pub scope: Scope,
}

/// The entries of the scope dropdown: this folder, its subfolders, home,
/// everywhere, then the places of the sidebar a person searches (the user
/// folders and the bookmarks). `current` is always among them.
pub fn scope_choices(places: &Places, home: &Path, current: &Scope) -> Vec<ScopeChoice> {
    let mut choices = vec![
        ScopeChoice {
            key: "folder".into(),
            label: "This folder".into(),
            icon: Icon::Folder,
            scope: Scope::Folder,
        },
        ScopeChoice {
            key: "subfolders".into(),
            label: "Subfolders".into(),
            icon: Icon::Folder,
            scope: Scope::Subfolders,
        },
        ScopeChoice {
            key: "home".into(),
            label: "Home".into(),
            icon: Icon::UserHome,
            scope: Scope::Home,
        },
        ScopeChoice {
            key: "everywhere".into(),
            label: "Everywhere".into(),
            icon: Icon::Computer,
            scope: Scope::Everywhere,
        },
    ];
    let mut seen: Vec<PathBuf> = vec![home.to_path_buf()];
    for place in places.standard.iter().chain(&places.bookmarks) {
        let Some(path) = &place.path else { continue };
        let icon = match place.kind {
            PlaceKind::Documents => Icon::FolderDocuments,
            PlaceKind::Downloads => Icon::FolderDownload,
            PlaceKind::Pictures => Icon::FolderPictures,
            PlaceKind::Music => Icon::FolderMusic,
            PlaceKind::Videos => Icon::FolderVideos,
            PlaceKind::Desktop => Icon::FolderDesktop,
            PlaceKind::Bookmark => Icon::Bookmarks,
            _ => continue,
        };
        if seen.contains(path) {
            continue;
        }
        seen.push(path.clone());
        choices.push(ScopeChoice {
            key: format!("place:{}", path.display()),
            label: place.name.clone(),
            icon,
            scope: Scope::Place(path.clone()),
        });
    }
    if let Scope::Place(path) = current
        && !seen.contains(path)
    {
        choices.push(ScopeChoice {
            key: format!("place:{}", path.display()),
            label: path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned()),
            icon: Icon::Folder,
            scope: Scope::Place(path.clone()),
        });
    }
    choices
}

/// The key of `scope` among the choices.
pub fn scope_key(scope: &Scope) -> String {
    match scope {
        Scope::Folder => "folder".into(),
        Scope::Subfolders => "subfolders".into(),
        Scope::Home => "home".into(),
        Scope::Everywhere => "everywhere".into(),
        Scope::Place(path) => format!("place:{}", path.display()),
    }
}

/// Where the search looks, in words: `Documents`, `this folder`, `Home`.
pub fn place_phrase(scope: &Scope, folder: &str, choices: &[ScopeChoice]) -> String {
    match scope {
        Scope::Folder => "this folder".into(),
        Scope::Subfolders => folder.to_string(),
        Scope::Home => "Home".into(),
        Scope::Everywhere => "everywhere".into(),
        Scope::Place(_) => choices
            .iter()
            .find(|choice| choice.scope == *scope)
            .map_or_else(|| folder.to_string(), |choice| choice.label.clone()),
    }
}

// ---------------------------------------------------------------------------
// What the bar says
// ---------------------------------------------------------------------------

/// What the bar tells about the search.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarStatus {
    /// Where it looks, in words.
    pub place: String,
    /// Something is typed or a filter is set.
    pub asked: bool,
    /// The scope reads folders (not "This folder").
    pub walking: bool,
    pub running: bool,
    pub found: usize,
    pub folders: usize,
    pub truncated: bool,
    pub stopped: bool,
    /// "This folder": how many of how many names match.
    pub matches: Option<(usize, usize)>,
    /// The query cannot be run, and why.
    pub invalid: Option<String>,
    /// `in:` names a folder that is not there.
    pub missing: Option<String>,
    /// A token the parser could not read, as `size: not a size`.
    pub problem: Option<String>,
}

/// The line the bar shows.
pub fn status_text(status: &BarStatus) -> String {
    if let Some(reason) = &status.invalid {
        return reason.clone();
    }
    if let Some(folder) = &status.problem {
        return folder.clone();
    }
    if let Some(folder) = &status.missing {
        return format!("There is no folder “{folder}” to look in");
    }
    if !status.asked {
        return "Try type:image, ext:pdf, size:>10MB or modified:week".to_string();
    }
    if !status.walking {
        return match status.matches {
            Some((matches, of)) => format!("{} of {} match in {}", count(matches), count(of), status.place),
            None => String::new(),
        };
    }
    let found = count(status.found);
    if status.running {
        return format!("Searching {}… {found} found", status.place);
    }
    let mut text = format!("{found} found in {}", status.place);
    if status.truncated {
        text.push_str(" · limit reached, narrow the search");
    } else if status.stopped {
        text.push_str(" · stopped");
    }
    text
}

/// What a token of the active filters says.
pub fn token_label(key: FilterKey, filters: &Filters) -> String {
    match key {
        FilterKey::Kind => format!(
            "Type: {}",
            filters.kinds.iter().map(|kind| kind_name(*kind)).collect::<Vec<_>>().join(", ")
        ),
        FilterKey::Ext => format!(
            "Extension: {}",
            filters.exts.iter().map(|ext| format!(".{ext}")).collect::<Vec<_>>().join(", ")
        ),
        FilterKey::Size => match filters.size {
            Some(SizeCond::Gt(n)) => format!("Size over {}", display::size(n)),
            Some(SizeCond::Ge(n)) => format!("Size at least {}", display::size(n)),
            Some(SizeCond::Lt(n)) => format!("Size under {}", display::size(n)),
            Some(SizeCond::Le(n)) => format!("Size at most {}", display::size(n)),
            Some(SizeCond::Eq(n)) => format!("Size {}", display::size(n)),
            Some(SizeCond::Between(low, high)) => format!("Size {} to {}", display::size(low), display::size(high)),
            None => String::new(),
        },
        FilterKey::Modified => match filters.modified {
            Some(Modified::Today) => "Modified today".into(),
            Some(Modified::Yesterday) => "Modified yesterday".into(),
            Some(Modified::Days(7)) => "Modified in the last 7 days".into(),
            Some(Modified::Days(30)) => "Modified in the last 30 days".into(),
            Some(Modified::Days(n)) => format!("Modified in the last {n} days"),
            Some(Modified::ThisYear) => "Modified this year".into(),
            Some(Modified::On(moment)) => format!("Modified in {moment}"),
            Some(Modified::Since(moment)) => format!("Modified since {moment}"),
            Some(Modified::Before(moment)) => format!("Modified before {moment}"),
            Some(Modified::Between(from, to)) => format!("Modified {from} to {to}"),
            None => String::new(),
        },
        FilterKey::Name => {
            let mode = match filters.name_mode {
                NameMode::Contains => "",
                NameMode::Starts => "starts with words",
                NameMode::Exact => "is exactly the words",
                NameMode::Regex => "regular expression",
            };
            let globs = filters.globs.join(", ");
            match (mode.is_empty(), globs.is_empty()) {
                (false, false) => format!("Name {mode}, {globs}"),
                (false, true) => format!("Name {mode}"),
                (true, _) => format!("Name {globs}"),
            }
        }
        FilterKey::Within => format!("In {}", filters.within.clone().unwrap_or_default()),
        FilterKey::Hidden => match filters.hidden {
            Hidden::Include => "Including hidden files".into(),
            Hidden::Only => "Only hidden files".into(),
            Hidden::Exclude => "Without hidden files".into(),
            Hidden::Follow => String::new(),
        },
        FilterKey::Case => "Match case".into(),
    }
}

// ---------------------------------------------------------------------------
// The bar
// ---------------------------------------------------------------------------

/// A filter as a token: its words and a button that removes it.
#[derive(PartialEq)]
struct FilterToken {
    label: String,
    on_remove: EventHandler<()>,
}

impl Component for FilterToken {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let on_remove = self.on_remove.clone();
        rect()
            .component("FilterToken")
            .a11y_alt(self.label.clone())
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    h(Dim::InlineButton),
                    pl(S2),
                    rounded(Radius::Control),
                    bg(Paint::AccentSoft),
                ],
            )
            .child(
                CAPTION
                    .in_ink(Ink::Text)
                    .label(&tokens, self.label.clone())
                    .max_lines(1),
            )
            .child(
                icon_button(Icon::WindowClose, move |_: ()| on_remove.call(()))
                    .size(Dim::InlineButton)
                    .radius(Radius::Control)
                    .tooltip(&format!("Remove filter: {}", self.label)),
            )
    }
}

/// The search bar: put it under the toolbar while [`Find::open`] is set.
#[derive(PartialEq)]
pub struct SearchBar {
    pub find: State<Find>,
    /// The field's text.
    pub draft: State<String>,
    pub choices: Vec<ScopeChoice>,
    pub status: BarStatus,
    pub include_noise: State<bool>,
    pub recents: State<Recents>,
    /// Enter: open the selected result.
    pub on_open: EventHandler<()>,
    /// Up (-1) in the field: the previous result.
    pub on_step: EventHandler<i32>,
    /// Down in the field: the keyboard goes to the list.
    pub on_list: EventHandler<()>,
    /// Escape with nothing left to clear: close the search.
    pub on_close: EventHandler<()>,
    pub on_stop: EventHandler<()>,
}

impl SearchBar {
    /// Turn the `key:value` tokens of the field into filters, and put what is
    /// left in the field.
    fn lift(
        find: State<Find>,
        draft: State<String>,
        mut fresh: State<u32>,
        text: &str,
        finished: bool,
    ) -> Option<String> {
        let mut now = find.peek().clone();
        let lifted = now.lift(text, finished)?;
        let mut find = find;
        find.set(now);
        // The field writes the text it was given after it tells us of the
        // change, so ours goes in once it has. A text set from here shows in
        // the field only in a field that is new, so each such text is a new
        // field (it takes the keyboard again by itself).
        let text = lifted.clone();
        spawn(async move {
            let mut draft = draft;
            draft.set(text);
            *fresh.write() += 1;
        });
        Some(lifted)
    }
}

impl Component for SearchBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let mut find = self.find;
        let mut draft = self.draft;
        let mut recents = self.recents;
        let id = use_a11y();
        let focus = use_focus(id);
        let mut field_area = use_state(Area::default);
        let mut fresh = use_state(|| 0_u32);
        let mut list_open = use_state(|| true);
        let mut highlight = use_state(|| None::<usize>);
        let mut hovered = use_state(|| None::<usize>);
        let scroll = use_scroll_controller(ScrollConfig::default);

        // A search started from the list (a letter typed, Ctrl+F) while the
        // bar is open takes the keyboard back.
        let wanted = find.read().focus;
        use_side_effect_with_deps(&wanted, move |wanted: &u32| {
            if *wanted > 0 {
                id.request_focus();
            }
        });

        let now = find.read().clone();
        let text = draft.read().clone();

        // The recent searches drop down while the field is empty and has the
        // keyboard.
        let recent_items: Vec<String> = recents.read().items().to_vec();
        let showing_recents = focus().is_focused() && text.is_empty() && !recent_items.is_empty() && list_open();
        let rows_total = recent_items.len() + 1;

        let pick = {
            let find = self.find;
            let draft = self.draft;
            EventHandler::new(move |index: usize| {
                let items: Vec<String> = recents.peek().items().to_vec();
                if index == items.len() {
                    recents.write().clear();
                    save_recents(&recents.peek());
                } else if let Some(text) = items.get(index) {
                    let mut draft = draft;
                    draft.set(text.clone());
                    SearchBar::lift(find, draft, fresh, text, true);
                    *fresh.write() += 1;
                }
                highlight.set(None);
                hovered.set(None);
                list_open.set(false);
                id.request_focus();
            })
        };

        let on_change = EventHandler::new(move |text: String| {
            list_open.set(true);
            highlight.set(None);
            hovered.set(None);
            SearchBar::lift(find, draft, fresh, &text, false);
        });
        let on_submit = {
            let on_open = self.on_open.clone();
            let pick = pick.clone();
            EventHandler::new(move |text: String| {
                if showing_recents && let Some(index) = *highlight.peek() {
                    pick.call(index);
                    return;
                }
                let text = SearchBar::lift(find, draft, fresh, &text, true).unwrap_or(text);
                let query = find.peek().query(&text);
                if !query.is_blank() {
                    recents.write().push(&query.to_string());
                    save_recents(&recents.peek());
                }
                on_open.call(());
            })
        };
        let on_arrow = {
            let on_step = self.on_step.clone();
            let on_list = self.on_list.clone();
            EventHandler::new(move |amount: f64| {
                // A positive amount is Up, a negative one Down.
                let up = amount > 0.0;
                if showing_recents {
                    let next = match (*highlight.peek(), up) {
                        (None, false) => Some(0),
                        (None, true) => Some(rows_total - 1),
                        (Some(at), false) => Some((at + 1).min(rows_total - 1)),
                        (Some(0), true) => None,
                        (Some(at), true) => Some(at - 1),
                    };
                    highlight.set(next);
                    if let Some(at) = next {
                        let current = <(i32, i32)>::from(scroll).1;
                        let mut scroll = scroll;
                        scroll.scroll_to_y(scroll_offset(at, rows_total, current, false));
                    }
                } else if up {
                    on_step.call(-1);
                } else {
                    on_list.call(());
                }
            })
        };
        let panel_was_open = now.filters_open;
        let on_escape = {
            let on_close = self.on_close.clone();
            EventHandler::new(move |_: ()| {
                // The Filters panel hears this Escape and closes; it is not a
                // second one for the field.
                if panel_was_open {
                    return;
                }
                // The recent searches drop away with the search: Escape with
                // nothing typed leaves, however many are listed.
                if !draft.peek().is_empty() {
                    draft.set(String::new());
                    *fresh.write() += 1;
                } else {
                    on_close.call(());
                }
            })
        };

        let placeholder = format!("Search {}", self.status.place);
        let field = search_field(draft, on_change)
            .placeholder(placeholder)
            .fill_width()
            .autofocus()
            .a11y_id(id)
            .on_submit(on_submit)
            .on_arrow(on_arrow)
            .on_escape(on_escape);

        let scope = {
            let choices = self.choices.clone();
            let icons: Vec<Icon> = choices.iter().map(|choice| choice.icon).collect();
            let options: Vec<(String, String)> =
                choices.iter().map(|choice| (choice.key.clone(), choice.label.clone())).collect();
            dropdown(options, scope_key(&now.scope), move |key: String| {
                if let Some(choice) = choices.iter().find(|choice| choice.key == key) {
                    find.write().scope = choice.scope.clone();
                }
            })
            .icons(icons)
            .title("Search in")
        };
        // An open list keeps its own Escape (the dropdown stops it); a closed
        // dropdown that has the keyboard lets Escape through, so the window
        // ends the search from here as it does from the field and the list.
        let filters_button = FiltersButton {
            find: self.find,
            include_noise: self.include_noise,
        };

        // Ctrl+Shift+F in the field opens or closes the Filters panel (the
        // field keeps character keys to itself, so the bar listens here).
        let toggle_filters = move |event: Event<KeyboardEventData>| {
            let chord = event.modifiers.contains(Modifiers::CONTROL) && event.modifiers.contains(Modifiers::SHIFT);
            if chord && matches!(&event.key, Key::Character(text) if text.eq_ignore_ascii_case("f")) {
                event.stop_propagation();
                event.prevent_default();
                let open = find.peek().filters_open;
                find.write().filters_open = !open;
            }
        };
        let first_row = if compact {
            rect()
                .tw(&tokens, &[flex_col(), w_full(), gap(S2)])
                .child(
                    rect()
                        .key(*fresh.read())
                        .on_key_down(toggle_filters.clone())
                        .tw(&tokens, &[w_full()])
                        .on_sized(move |event: Event<SizedEventData>| field_area.set_if_modified(event.area))
                        .child(field),
                )
                .child(
                    rect()
                        .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2), w_full()])
                        .child(scope)
                        .child(filters_button),
                )
        } else {
            rect()
                .tw(&tokens, &[flex_row(), grow_children(), items(Align::Center), gap(S2), w_full()])
                .child(
                    rect()
                        .key(*fresh.read())
                        .on_key_down(toggle_filters.clone())
                        .tw(&tokens, &[w_flex()])
                        .on_sized(move |event: Event<SizedEventData>| field_area.set_if_modified(event.area))
                        .child(field),
                )
                .child(scope)
                .child(filters_button)
        };

        // The second row: tokens for what is set, and how the search is going.
        let active = now.filters.active();
        let token_row = rect()
            .tw(&tokens, &[flex_row(), wrap(S1), items(Align::Center), if compact { w_full() } else { w_flex() }])
            .children(active.iter().map(|key| {
                let key = *key;
                Element::from(FilterToken {
                    label: token_label(key, &now.filters),
                    on_remove: EventHandler::new(move |_: ()| find.write().filters.clear(key)),
                })
            }))
            .maybe_child((active.len() > 1).then(|| {
                Element::from(
                    action_button("Clear all", move |_: ()| find.write().filters.clear_all())
                        .flat()
                        .compact(),
                )
            }));
        let status_text = status_text(&self.status);
        let status = rect()
            .component("SearchStatus")
            .a11y_role(AccessibilityRole::Status)
            .a11y_alt(status_text.clone())
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2), h(Dim::Control)])
            .maybe_child(self.status.running.then(|| Element::from(spinner(SpinnerSize::Small))))
            .child(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, status_text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(self.status.running.then(|| {
                let on_stop = self.on_stop.clone();
                Element::from(action_button("Stop", move |_: ()| on_stop.call(())).flat().compact())
            }));
        // Tokens take the room there is and the status line sits at the end;
        // with no tokens the status line is the row, from the start. A row of
        // tokens that wraps makes the row taller (it has a minimum, not a
        // height), never lets a token spill over the row above. On a phone the
        // tokens are a block of their own and the status line is under it.
        let second_row = if compact {
            rect()
                .component("SearchTail")
                .tw(&tokens, &[flex_col(), gap(S1), w_full()])
                .maybe_child((!active.is_empty()).then_some(token_row))
                .child(rect().tw(&tokens, &[w_full()]).child(status))
        } else {
            rect()
                .component("SearchTail")
                .tw(&tokens, &[flex_row(), grow_children(), items(Align::Center), gap(S2), w_full(), min_h(Dim::Control)])
                .maybe_child((!active.is_empty()).then_some(token_row))
                .child(if active.is_empty() {
                    Element::from(rect().tw(&tokens, &[w_flex()]).child(status))
                } else {
                    Element::from(status)
                })
        };

        // The recent searches, under the field.
        let panel = {
            let mut rows: Vec<Element> = recent_items
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    let pick = pick.clone();
                    Element::from(ListRow {
                        icon: Lead::Action(Icon::Clock),
                        parts: vec![Part::plain(text.clone())],
                        current: false,
                        mark: Mark::Plain,
                        highlighted: hovered().is_none() && highlight() == Some(index),
                        on_press: EventHandler::new(move |_: ()| pick.call(index)),
                        on_hover: EventHandler::new(move |inside: bool| hovered.set(inside.then_some(index))),
                    })
                })
                .collect();
            let forget = recent_items.len();
            let pick = pick.clone();
            rows.push(Element::from(ListRow {
                icon: Lead::Action(Icon::EditClear),
                parts: vec![Part::muted("Clear recent searches")],
                current: false,
                mark: Mark::Plain,
                highlighted: hovered().is_none() && highlight() == Some(forget),
                on_press: EventHandler::new(move |_: ()| pick.call(forget)),
                on_hover: EventHandler::new(move |inside: bool| hovered.set(inside.then_some(forget))),
            }));
            popover(showing_recents, &field_area.read(), rows, scroll)
        };

        rect()
            .component("SearchBar")
            .tw(&tokens, &[flex_col(), w_full(), p(S2), gap(S2)])
            .child(first_row)
            .child(second_row)
            .child(Element::from(panel))
    }
}

/// The file the recent searches are kept in: the person's own, `Recents::file()`.
///
/// A test thread has a file of its own, so a test never opens with what another one searched for
/// (the recent searches drop down under the field and would cover the bar's buttons) and never
/// writes the person's list.
#[cfg(not(test))]
pub(super) fn recents_file() -> PathBuf {
    Recents::file()
}

#[cfg(test)]
pub(super) fn recents_file() -> PathBuf {
    super::test_state::private_file("recent-searches")
}

/// Write the recent searches where they are kept, off the UI thread. A
/// write that fails is dropped: it is a convenience.
fn save_recents(recents: &Recents) {
    let snapshot = recents.clone();
    // The path is the window's: this thread knows it, the writer's thread may not.
    let file = recents_file();
    let _ = std::thread::Builder::new().name("bingux-recent-searches".into()).spawn(move || {
        let _ = snapshot.save_to(&file);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_files::Place;
    use bingux_files::find::Kind;

    fn status(walking: bool) -> BarStatus {
        BarStatus {
            place: "Pictures".into(),
            asked: true,
            walking,
            found: 1204,
            ..BarStatus::default()
        }
    }

    #[test]
    fn the_status_line_says_what_the_search_is_doing() {
        let mut s = status(true);
        s.running = true;
        assert_eq!(status_text(&s), "Searching Pictures… 1,204 found");
        s.running = false;
        assert_eq!(status_text(&s), "1,204 found in Pictures");
        s.stopped = true;
        assert_eq!(status_text(&s), "1,204 found in Pictures · stopped");
        s.truncated = true;
        assert!(status_text(&s).ends_with("limit reached, narrow the search"));
        let mut folder = status(false);
        folder.matches = Some((3, 120));
        folder.place = "this folder".into();
        assert_eq!(status_text(&folder), "3 of 120 match in this folder");
        let idle = BarStatus { place: "x".into(), ..BarStatus::default() };
        assert!(status_text(&idle).starts_with("Try type:image"));
        // A mistake outranks the rest.
        let mut bad = status(true);
        bad.invalid = Some("Not a valid regular expression".into());
        bad.missing = Some("Nowhere".into());
        assert_eq!(status_text(&bad), "Not a valid regular expression");
        bad.invalid = None;
        assert_eq!(status_text(&bad), "There is no folder “Nowhere” to look in");
    }

    #[test]
    fn a_token_says_its_filter_in_words() {
        let filters = bingux_files::find::Query::parse("type:image,video size:>10MB modified:week ext:png name:*.x case:yes hidden:only in:Docs match:regex").filters;
        let label = |key| token_label(key, &filters);
        assert_eq!(label(FilterKey::Kind), "Type: Images, Video");
        assert_eq!(label(FilterKey::Size), "Size over 10 MB");
        assert_eq!(label(FilterKey::Modified), "Modified in the last 7 days");
        assert_eq!(label(FilterKey::Ext), "Extension: .png");
        assert_eq!(label(FilterKey::Name), "Name regular expression, *.x");
        assert_eq!(label(FilterKey::Case), "Match case");
        assert_eq!(label(FilterKey::Hidden), "Only hidden files");
        assert_eq!(label(FilterKey::Within), "In Docs");
        let _ = Kind::Image;
    }

    fn places() -> Places {
        let place = |kind, name: &str, path: &str| Place { kind, name: name.into(), path: Some(PathBuf::from(path)) };
        Places {
            standard: vec![
                place(PlaceKind::Home, "Home", "/home/me"),
                place(PlaceKind::Documents, "Documents", "/home/me/Documents"),
                place(PlaceKind::Pictures, "Pictures", "/home/me/Pictures"),
                place(PlaceKind::Trash, "Trash", "/home/me/.trash"),
            ],
            devices: vec![place(PlaceKind::Volume, "USB", "/run/media/me/USB")],
            bookmarks: vec![place(PlaceKind::Bookmark, "Work", "/srv/work")],
        }
    }

    #[test]
    fn the_scope_dropdown_lists_the_core_scopes_then_places() {
        let choices = scope_choices(&places(), Path::new("/home/me"), &Scope::Subfolders);
        let labels: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["This folder", "Subfolders", "Home", "Everywhere", "Documents", "Pictures", "Work"]);
        // A scope that is not among them (a bookmark since removed) is added.
        let gone = Scope::Place(PathBuf::from("/srv/gone"));
        let choices = scope_choices(&places(), Path::new("/home/me"), &gone);
        assert_eq!(choices.last().map(|c| c.label.as_str()), Some("gone"));
        assert!(choices.iter().any(|c| c.key == scope_key(&gone)));
        // Every key names one scope.
        let keys: std::collections::HashSet<&str> = choices.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys.len(), choices.len());
        assert_eq!(place_phrase(&Scope::Place(PathBuf::from("/home/me/Pictures")), "Docs", &scope_choices(&places(), Path::new("/home/me"), &Scope::Subfolders)), "Pictures");
        assert_eq!(place_phrase(&Scope::Subfolders, "Docs", &choices), "Docs");
        assert_eq!(place_phrase(&Scope::Folder, "Docs", &choices), "this folder");
    }
}
