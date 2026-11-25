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
use freya::prelude::*;

use crate::model::suggest::{self, OpenTab, Source, Suggestion};
use crate::model::tabs::Place;

use super::ctx::Ctx;

/// The address bar.
#[derive(PartialEq)]
pub struct AddressBar {
    pub ctx: Ctx,
}

fn icon_of(suggestion: &Suggestion) -> Icon {
    match suggestion.source {
        Source::Typed if suggestion.detail.is_empty() => Icon::Search,
        Source::Typed => Icon::WebBrowser,
        Source::Tab(_) => Icon::WindowNew,
        Source::History => Icon::Clock,
    }
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

impl Component for AddressBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let chrome = ctx.chrome;
        let id = use_a11y();
        let focus = use_focus(id);
        let mut hovered = use_state(|| None::<usize>);
        let enter_with_alt = use_hook(|| Rc::new(Cell::new(false)));
        let mut typing = chrome.typing;
        let mut address = chrome.address;
        let mut highlight = chrome.highlight;
        let mut field_area = chrome.field_area;
        let focused = focus().is_focused();

        let (key, shown) = {
            let browser = ctx.browser.read();
            (
                browser.tabs.active_key(),
                browser.active().map(|tab| tab.address_text()).unwrap_or_default(),
            )
        };
        let editing = typing();
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
            suggest::suggest(&address.read(), browser.search(), &open, &browser.history)
        } else {
            Vec::new()
        };

        // The field follows the page while nobody is typing in it.
        use_side_effect_with_deps(&(key, shown.clone(), editing), move |(_, shown, editing): &(u64, String, bool)| {
            if !*editing {
                address.set(shown.clone());
            }
        });
        // The keyboard leaving the field ends the typing, unless the pointer is
        // on a row (a press on it takes the keyboard for a moment).
        use_side_effect_with_deps(&focused, move |focused: &bool| {
            if !*focused && *typing.peek() && hovered.peek().is_none() {
                typing.set(false);
                highlight.set(None);
            }
        });

        let submit = {
            let rows = rows.clone();
            let enter_with_alt = enter_with_alt.clone();
            move |text: String| {
                let chosen = (*highlight.peek()).and_then(|at| rows.get(at)).cloned();
                let alt = enter_with_alt.replace(false);
                typing.set(false);
                highlight.set(None);
                match chosen {
                    Some(Suggestion {
                        source: Source::Tab(tab), ..
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
        let wanted = chrome.focus_address.read().to_owned();
        let mut field = text_field(address, submit)
            .fill_width()
            .placeholder(if is_compact() { "Search or address" } else { "Search or enter an address" })
            .leading(Icon::Search)
            .a11y_id(id)
            .revert_to(shown.clone())
            .on_change(move |_: String| {
                typing.set(true);
                highlight.set(None);
            })
            .on_arrow(move |step: f64| {
                // Up is a positive step in a field, and Up goes to the row above.
                let next = move_highlight(*highlight.peek(), count, step < 0.0);
                highlight.set(next);
            })
            .on_escape(move |_: ()| {
                typing.set(false);
                highlight.set(None);
            })
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
                suggestion_row(icon_of(row), row.title.clone(), row.detail.clone())
                    .highlighted(hovered().is_none() && highlight() == Some(at))
                    .on_press(move |_: ()| {
                        typing.set(false);
                        highlight.set(None);
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
                popover(open, &anchor, rect().width(Size::fill()).children(list).into_element())
                    .attached()
                    .on_dismiss(move |_: ()| typing.set(false)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arrows_walk_the_list_and_come_back_to_the_typed_text() {
        assert_eq!(move_highlight(None, 3, true), Some(0));
        assert_eq!(move_highlight(Some(0), 3, true), Some(1));
        assert_eq!(move_highlight(Some(2), 3, true), None, "past the last is the typed text again");
        assert_eq!(move_highlight(None, 3, false), Some(2));
        assert_eq!(move_highlight(Some(1), 3, false), Some(0));
        assert_eq!(move_highlight(Some(0), 3, false), None);
        assert_eq!(move_highlight(None, 0, true), None);
    }
}
