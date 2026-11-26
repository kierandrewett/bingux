//! The address bar: a text field that shows the page's address, takes an
//! address or words, and lists what matches while it is typed in.
//!
//! The field belongs to the person while they type and to the page the rest of
//! the time: a new address from the engine replaces the text unless the person
//! has changed it. Enter loads what was typed (or the suggestion the arrows
//! reached), Escape puts the page's address back, and Ctrl+L takes the
//! keyboard with the text selected.

use std::cell::Cell;
use std::rc::Rc;

use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use bingux_ui::search;
use freya::prelude::*;

use super::{OpenTab, Source, Suggestion, Visited};
use crate::history::History;
use crate::tabs::Place;

use crate::ui::Ctx;

/// What the address field shows: the page's address, or what the person typed
/// (Gecko's urlbar keeps `userTypedValue` the same way).
///
/// Why an enum: "typing" and "the highlighted suggestion" were two states
/// that had to be set and cleared together in six places. The highlight means
/// nothing while the field follows the page, and dismissing the list cleared
/// one and not the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UrlbarValue {
    /// The field shows the address of the page. Nobody is typing in it.
    Page,
    /// The person changed the text: it is theirs until they submit or leave.
    /// `highlight` is the suggestion the arrow keys have reached.
    Typed { highlight: Option<usize> },
}

impl UrlbarValue {
    /// The person typed: editing, with no row reached yet.
    pub(super) fn after_typing() -> Self {
        Self::Typed { highlight: None }
    }

    /// Whether the text belongs to the person.
    pub(super) fn is_typed(self) -> bool {
        matches!(self, Self::Typed { .. })
    }

    /// The suggestion the keys have reached, if the person is editing.
    pub(super) fn highlight(self) -> Option<usize> {
        match self {
            Self::Page => None,
            Self::Typed { highlight } => highlight,
        }
    }

    /// An arrow key with `rows` suggestions listed. A field that follows the
    /// page lists none, so it does not move.
    pub(super) fn arrowed(self, rows: usize, down: bool) -> Self {
        match self {
            Self::Page => Self::Page,
            Self::Typed { highlight } => Self::Typed {
                highlight: FieldState::move_highlight(highlight, rows, down),
            },
        }
    }
}

/// The states of the address field that belong to the window, not to a tab.
#[derive(Clone, Copy, PartialEq)]
pub struct FieldState {
    /// The text of the field.
    text: State<String>,
    /// Whether the field follows the page or the person is typing in it.
    entry: State<UrlbarValue>,
    /// The field searches the open tabs alone (*Search all tabs*), until the
    /// person leaves it.
    tabs_only: State<bool>,
    /// Counts the requests to focus the field. A new count is a new field
    /// that takes the focus with its text selected.
    focus: State<u32>,
    /// The field's box, which the suggestions hang from.
    area: State<Area>,
}

impl FieldState {
    /// The states, as a hook: call it on every render, in the same place.
    pub fn use_new() -> Self {
        Self {
            text: use_state(String::new),
            entry: use_state(|| UrlbarValue::Page),
            tabs_only: use_state(|| false),
            focus: use_state(|| 0_u32),
            area: use_state(Area::default),
        }
    }

    /// Put the cursor in the field, with its text selected.
    pub fn focus(self) {
        let mut entry = self.entry;
        entry.set(UrlbarValue::Page);
        let mut focus = self.focus;
        let next = *focus.peek() + 1;
        focus.set(next);
    }

    /// Put the cursor in an empty field that lists the open tabs and nothing
    /// else, as Firefox's *Search all tabs* does: the person types to narrow
    /// them, and Enter goes to the highlighted tab, or the first.
    pub fn search_tabs(self) {
        let (mut text, mut entry, mut tabs_only) = (self.text, self.entry, self.tabs_only);
        text.set(String::new());
        tabs_only.set(true);
        // The list shows only while the person is editing.
        entry.set(UrlbarValue::after_typing());
        let mut focus = self.focus;
        let next = *focus.peek() + 1;
        focus.set(next);
    }

    /// The field takes the focus when the window first shows it.
    pub fn focus_at_open(self) {
        let mut focus = self.focus;
        focus.set(1);
    }

    /// The row the arrows reach after `step`: Down goes to the first row and on,
    /// back to the typed text after the last; Up goes the other way.
    pub fn move_highlight(current: Option<usize>, rows: usize, down: bool) -> Option<usize> {
        if rows == 0 {
            return None;
        }
        match (current, down) {
            (None, true) => Some(0),
            (None, false) => Some(rows - 1),
            (Some(at), true) => (at + 1 < rows).then_some(at + 1),
            (Some(at), false) => at.checked_sub(1),
        }
    }
}

/// The address bar.
#[derive(PartialEq)]
pub struct AddressBar {
    pub ctx: Ctx,
}

impl Component for AddressBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let state = ctx.address;
        let id = use_a11y();
        let focus = use_focus(id);
        let mut hovered = use_state(|| None::<usize>);
        let enter_with_alt = use_hook(|| Rc::new(Cell::new(false)));
        let mut entry = state.entry;
        let mut address = state.text;
        let mut field_area = state.area;
        let mut tabs_only = state.tabs_only;
        let focused = focus().is_focused();

        let (key, shown) = {
            let browser = ctx.browser.read();
            (
                browser.tabs.active_key(),
                browser
                    .active()
                    .map(|tab| tab.address_text())
                    .unwrap_or_default(),
            )
        };
        let editing = entry().is_typed();
        // History is the profile's `history` searchable, asked off the UI
        // thread as the text changes; a private window has none.
        let typed = address.read().clone();
        let has_history = ctx.browser.read().has_history();
        let found = search::use_search_with(
            History::suggestions(&typed),
            editing && has_history && !tabs_only() && !typed.trim().is_empty(),
        );
        // What the list offers: only while the person has changed the text.
        let rows: Vec<Suggestion> = if editing {
            let browser = ctx.browser.read();
            let open: Vec<OpenTab> = browser
                .tabs
                .all()
                .iter()
                .map(|tab| OpenTab {
                    key: tab.key,
                    title: tab.title.clone(),
                    url: tab.url.clone(),
                })
                .collect();
            if tabs_only() {
                // *Search all tabs*: the open tabs alone, and no row for the text itself.
                Suggestion::tabs(&typed, &open)
            } else {
                let visited: Vec<Visited> = found.hits.iter().map(History::visited).collect();
                Suggestion::list(&typed, browser.search(), &open, &visited)
            }
        } else {
            Vec::new()
        };

        // The field follows the page while nobody is typing in it.
        use_side_effect_with_deps(
            &(key, shown.clone(), editing),
            move |(_, shown, editing): &(u64, String, bool)| {
                if !*editing {
                    address.set(shown.clone());
                }
            },
        );
        // The search of the open tabs ends with the typing.
        use_side_effect_with_deps(&editing, move |editing: &bool| {
            if !*editing {
                tabs_only.set(false);
            }
        });
        // The keyboard leaving the field ends the typing, unless the pointer is
        // on a row (a press on it takes the keyboard for a moment).
        use_side_effect_with_deps(&focused, move |focused: &bool| {
            if !*focused && entry.peek().is_typed() && hovered.peek().is_none() {
                entry.set(UrlbarValue::Page);
            }
        });

        let submit = {
            let rows = rows.clone();
            let enter_with_alt = enter_with_alt.clone();
            let scoped = tabs_only();
            move |text: String| {
                let chosen = entry
                    .peek()
                    .highlight()
                    .and_then(|at| rows.get(at))
                    .cloned();
                // A search of the open tabs goes to the first match when no row
                // was reached, and to nothing when nothing matches: what was typed is
                // not an address there.
                let chosen = if scoped {
                    chosen.or_else(|| rows.first().cloned())
                } else {
                    chosen
                };
                let alt = enter_with_alt.replace(false);
                entry.set(UrlbarValue::Page);
                if scoped && chosen.is_none() {
                    return;
                }
                match chosen {
                    Some(Suggestion {
                        source: Source::Tab(tab),
                        ..
                    }) => ctx.with(|browser| browser.select(tab)),
                    Some(row) if alt => {
                        ctx.with(|browser| browser.open_tab(&row.url, Place::AfterActive, true));
                    }
                    Some(row) => ctx.with(|browser| browser.navigate(key, &row.url)),
                    None if alt => {
                        ctx.with(|browser| browser.open_tab(&text, Place::AfterActive, true));
                    }
                    None => ctx.with(|browser| browser.navigate(key, &text)),
                }
            }
        };
        let count = rows.len();
        let wanted = state.focus.read().to_owned();
        let mut field = text_field(address, submit)
            .fill_width()
            .placeholder(if tabs_only() {
                tr!("browser-address-search-tabs-placeholder")
            } else if is_compact() {
                tr!("browser-address-compact-placeholder")
            } else {
                tr!("browser-address-placeholder")
            })
            .leading(Icon::Search)
            .a11y_id(id)
            .revert_to(shown.clone())
            .on_change(move |_: String| {
                entry.set(UrlbarValue::after_typing());
            })
            .on_arrow(move |step: f64| {
                // Up is a positive step in a field, and Up goes to the row above.
                let next = entry.peek().arrowed(count, step < 0.0);
                entry.set(next);
            })
            .on_escape(move |_: ()| entry.set(UrlbarValue::Page))
            .on_enter_modifiers({
                let enter_with_alt = enter_with_alt.clone();
                move |held: Modifiers| enter_with_alt.set(held.contains(Modifiers::ALT))
            });
        if wanted > 0 {
            field = field.autofocus().select_all_on_open();
        }

        let list: Vec<Element> = rows
            .iter()
            .enumerate()
            .map(|(at, row)| {
                let target = row.clone();
                suggestion_row(row.icon(), row.title.clone(), row.detail.clone())
                    .highlighted(hovered().is_none() && entry().highlight() == Some(at))
                    .on_press(move |_: ()| {
                        entry.set(UrlbarValue::Page);
                        match target.source {
                            Source::Tab(tab) => ctx.with(|browser| browser.select(tab)),
                            _ => ctx.with(|browser| browser.navigate(key, &target.url)),
                        }
                    })
                    .on_hover(move |inside: bool| hovered.set(inside.then_some(at)))
                    .into()
            })
            .collect();
        let open = editing && !list.is_empty() && (focused || hovered().is_some());
        let anchor = field_area.read().clone();

        rect()
            .component("AddressBar")
            .tw(&tokens, &[w_flex()])
            .on_sized(move |event: Event<SizedEventData>| field_area.set_if_modified(event.area))
            .child(rect().key(wanted).tw(&tokens, &[w_full()]).child(field))
            .child(
                popover(
                    open,
                    &anchor,
                    rect().width(Size::fill()).children(list).into_element(),
                )
                .attached()
                .on_dismiss(move |_: ()| entry.set(UrlbarValue::Page)),
            )
    }
}
