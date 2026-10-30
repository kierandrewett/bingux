//! The unsaved values of a form on a settings page, and the rows of a list a form edits.
//!
//! A page reads its values from the saved profile and keeps what the person types in a [`Draft`].
//! The draft is dirty while it differs from the profile, [`Draft::reset`] puts the saved values back,
//! and a page's Apply turns the difference into one `nmcli connection modify`. The draft never
//! replaces itself when the profile is read again: a switch changed elsewhere on the page must not
//! throw away what is typed in a field.

use freya::prelude::*;

use bingux_ui::controls::form::validate::Check;
use bingux_ui::controls::form_field;

use crate::icons::Icon;
use crate::pages::connection_editor::ProfileData;
use crate::ui::{Dim, icon_button, row};

/// What a form starts with, read from the saved profile: text values and lists, each under a name.
/// The names need not be `nmcli` keys; a form may split one stored value into several fields.
#[derive(Clone, Default, PartialEq)]
pub struct Values {
    pub texts: Vec<(&'static str, String)>,
    pub lists: Vec<(&'static str, Vec<String>)>,
}

/// One row of a list the person edits.
#[derive(Clone, PartialEq)]
pub struct Entry {
    id: u64,
    pub text: String,
}

/// The rows of one list, and the number the next new row gets.
#[derive(Clone, Copy, PartialEq)]
pub struct ListState {
    items: State<Vec<Entry>>,
    next: State<u64>,
}

impl ListState {
    /// The texts of the rows that are not blank, trimmed.
    pub fn texts(self) -> Vec<String> {
        self.items
            .read()
            .iter()
            .map(|entry| entry.text.trim().to_string())
            .filter(|text| !text.is_empty())
            .collect()
    }

    fn fill(self, texts: &[String]) {
        let mut next = self.next;
        let mut items = self.items;
        let first = *next.peek();
        items.set(
            texts
                .iter()
                .enumerate()
                .map(|(index, text)| Entry {
                    id: first + index as u64,
                    text: text.clone(),
                })
                .collect(),
        );
        next.set(first + texts.len() as u64);
    }

    fn add(self) {
        let mut next = self.next;
        let mut items = self.items;
        let id = *next.peek();
        next.set(id + 1);
        items.write().push(Entry {
            id,
            text: String::new(),
        });
    }

    fn remove(self, id: u64) {
        let mut items = self.items;
        items.write().retain(|entry| entry.id != id);
    }

    fn set_text(self, id: u64, text: String) {
        let mut items = self.items;
        let unchanged = items
            .peek()
            .iter()
            .find(|entry| entry.id == id)
            .is_none_or(|entry| entry.text == text);
        if !unchanged && let Some(entry) = items.write().iter_mut().find(|entry| entry.id == id) {
            entry.text = text;
        }
    }
}

/// The unsaved values of one form.
#[derive(Clone)]
pub struct Draft {
    texts: Vec<(&'static str, State<String>)>,
    lists: Vec<(&'static str, ListState)>,
    read: fn(&ProfileData) -> Values,
}

/// A draft that starts at `read(saved)`. Call from `render`, always with the same shape of values.
pub fn use_draft(saved: &ProfileData, read: fn(&ProfileData) -> Values) -> Draft {
    let initial = read(saved);
    let texts = initial
        .texts
        .iter()
        .map(|(name, value)| {
            let value = value.clone();
            (*name, use_state(move || value))
        })
        .collect();
    let lists = initial
        .lists
        .iter()
        .map(|(name, items)| {
            let entries: Vec<Entry> = items
                .iter()
                .enumerate()
                .map(|(index, text)| Entry {
                    id: index as u64,
                    text: text.clone(),
                })
                .collect();
            let count = entries.len() as u64;
            let state = ListState {
                items: use_state(move || entries),
                next: use_state(move || count),
            };
            (*name, state)
        })
        .collect();
    Draft { texts, lists, read }
}

impl Draft {
    /// The state of the text value `name`.
    pub fn text(&self, name: &str) -> State<String> {
        self.texts
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, state)| *state)
            .unwrap_or_else(|| panic!("the form has no value named {name}"))
    }

    /// The text value `name` as typed now.
    pub fn get(&self, name: &str) -> String {
        self.text(name).read().clone()
    }

    pub fn set(&self, name: &str, value: impl Into<String>) {
        let mut state = self.text(name);
        state.set(value.into());
    }

    pub fn list(&self, name: &str) -> ListState {
        self.lists
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, state)| *state)
            .unwrap_or_else(|| panic!("the form has no list named {name}"))
    }

    /// Whether anything differs from the saved profile.
    pub fn dirty(&self, saved: &ProfileData) -> bool {
        let saved = (self.read)(saved);
        let texts = saved
            .texts
            .iter()
            .any(|(name, value)| self.get(name) != *value);
        let lists = saved
            .lists
            .iter()
            .any(|(name, items)| self.list(name).texts() != *items);
        texts || lists
    }

    /// Put the saved values back in every field.
    pub fn reset(&self, saved: &ProfileData) {
        let saved = (self.read)(saved);
        for (name, value) in &saved.texts {
            self.set(name, value.clone());
        }
        for (name, items) in &saved.lists {
            self.list(name).fill(items);
        }
    }
}

/// What a list's rows look like.
#[derive(Clone, Copy)]
pub struct ListLook {
    /// The title of a row; several rows are numbered.
    pub title: &'static str,
    pub placeholder: &'static str,
    pub check: fn(&str) -> Check,
    /// The label of the row that adds one.
    pub add: &'static str,
    pub monospace: bool,
}

/// The rows of a list for a group: one field per entry with its remove button, then the row that
/// adds an entry.
pub fn list_rows(list: ListState, look: ListLook) -> Vec<Element> {
    let count = list.items.read().len();
    let mut rows: Vec<Element> = list
        .items
        .read()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let title = if count > 1 {
                format!("{} {}", look.title, index + 1)
            } else {
                look.title.to_string()
            };
            super::keyed(
                entry.id,
                EntryRow {
                    list,
                    id: entry.id,
                    title,
                    look: LookKey::of(look),
                },
            )
        })
        .collect();
    rows.push(Element::from(
        row(look.add)
            .leading(Icon::ListAdd)
            .on_press(move |_| list.add()),
    ));
    rows
}

/// The parts of a [`ListLook`] an entry row compares to know whether it changed.
#[derive(Clone, Copy)]
struct LookKey {
    placeholder: &'static str,
    check: fn(&str) -> Check,
    monospace: bool,
}

impl LookKey {
    fn of(look: ListLook) -> Self {
        Self {
            placeholder: look.placeholder,
            check: look.check,
            monospace: look.monospace,
        }
    }
}

impl PartialEq for LookKey {
    fn eq(&self, other: &Self) -> bool {
        self.placeholder == other.placeholder
            && self.check as usize == other.check as usize
            && self.monospace == other.monospace
    }
}

/// One entry of a list. It keeps what is typed in its own field and writes it to the list as it changes.
#[derive(PartialEq)]
struct EntryRow {
    list: ListState,
    id: u64,
    title: String,
    look: LookKey,
}

impl Component for EntryRow {
    fn render(&self) -> impl IntoElement {
        let list = self.list;
        let id = self.id;
        let initial = list
            .items
            .peek()
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.text.clone())
            .unwrap_or_default();
        let text = use_state(move || initial);
        use_side_effect(move || list.set_text(id, text.read().clone()));
        let mut field = form_field(self.title.clone(), text)
            .placeholder(self.look.placeholder)
            .validate(self.look.check)
            .trailing(icon_button(Icon::ListRemove, move |_| list.remove(id)).tooltip("Remove"));
        if self.look.monospace {
            field = field.monospace();
        }
        field
    }
}

/// An empty element as wide as a row's remove button, for the trailing end of a field that sits in
/// a group with removable rows, so every field in the group ends at the same place.
pub fn remove_slot() -> Element {
    let side = Size::px(Dim::Button.px());
    rect().width(side.clone()).height(side).into()
}
