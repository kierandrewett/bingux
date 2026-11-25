//! The find bar: a strip docked under the toolbar with a field, the count of
//! matches, Previous and Next, two options and Close.

use std::cell::Cell;
use std::rc::Rc;

use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::ctx::Ctx;

/// What the strip says about the search: how many matches and which one, or
/// that there is none.
pub fn result_text(text: &str, result: Option<(u32, u32)>, compact: bool) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    match result {
        None => None,
        // A phone has no room for the long form beside the field and three buttons.
        Some((_, 0)) if compact => Some("None".to_string()),
        Some((_, 0)) => Some("Phrase not found".to_string()),
        Some((current, total)) if compact => Some(format!("{current}/{total}")),
        Some((current, total)) => Some(format!("{current} of {total}")),
    }
}

#[derive(PartialEq)]
pub struct FindBar {
    pub ctx: Ctx,
    /// The tab whose page is searched. A new tab is a new bar.
    pub key: u64,
}

impl Component for FindBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let key = self.key;
        let find = ctx.browser.read().tabs.get(key).map(|tab| tab.find.clone()).unwrap_or_default();
        let mut text = use_state({
            let initial = find.text.clone();
            move || initial
        });
        let enter_with_shift = use_hook(|| Rc::new(Cell::new(false)));
        let wanted = ctx.chrome.focus_find.read().to_owned();
        let compact = is_compact();

        let step = move |forward: bool| {
            let current = text.peek().clone();
            ctx.with(|browser| browser.find(key, &current, forward, false));
        };
        let mut field = text_field(text, {
            let enter_with_shift = enter_with_shift.clone();
            move |current: String| {
                let backwards = enter_with_shift.replace(false);
                ctx.with(|browser| browser.find(key, &current, !backwards, false));
            }
        })
        .flat()
        .fill_width()
        .leading(Icon::Search)
        .placeholder("Find in page")
        .on_change(move |changed: String| ctx.with(|browser| browser.find(key, &changed, true, true)))
        .on_escape(move |_: ()| ctx.with(|browser| browser.close_find(key)))
        .on_enter_modifiers({
            let enter_with_shift = enter_with_shift.clone();
            move |held: Modifiers| enter_with_shift.set(held.contains(Modifiers::SHIFT))
        })
        .danger_outline(find.not_found());
        if wanted > 0 {
            field = field.autofocus().select_all_on_open();
        }

        let status = result_text(&find.text, find.result, compact);
        let (match_case, highlight_all, whole_words) = (find.match_case, find.highlight_all, find.whole_words);
        let options = !compact;
        rect()
            .component("FindBar")
            .a11y_role(AccessibilityRole::Search)
            .a11y_alt("Find in page")
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S2),
                    px(S2),
                    py(S1),
                    bg(Paint::Surface),
                    border(Line::HairlineBottom),
                ],
            )
            .child(rect().key(wanted).tw(&tokens, &[w_flex()]).child(field))
            .maybe_child(status.map(|status| {
                let ink = if find.not_found() { Ink::Danger } else { Ink::Muted };
                Element::from(typography::CAPTION.in_ink(ink).label(&tokens, status).max_lines(1))
            }))
            .child(
                icon_button(Icon::PanUp, move |_: ()| step(false))
                    .tooltip("Previous match")
                    .enabled(!find.text.is_empty()),
            )
            .child(
                icon_button(Icon::PanDown, move |_: ()| step(true))
                    .tooltip("Next match")
                    .enabled(!find.text.is_empty()),
            )
            .maybe_child(options.then(|| {
                checkbox(match_case, move |on: bool| {
                    ctx.with(|browser| browser.set_find_options(key, on, whole_words, highlight_all));
                })
                .text("Match case")
            }))
            .maybe_child(options.then(|| {
                checkbox(highlight_all, move |on: bool| {
                    ctx.with(|browser| browser.set_find_options(key, match_case, whole_words, on));
                })
                .text("Highlight all")
            }))
            .child(icon_button(Icon::WindowClose, move |_: ()| ctx.with(|browser| browser.close_find(key))).tooltip("Close find bar"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_says_how_many_and_which_or_that_there_are_none() {
        assert_eq!(result_text("", Some((1, 3)), false), None, "nothing searched for");
        assert_eq!(result_text("a", None, false), None, "no answer yet");
        assert_eq!(result_text("a", Some((2, 5)), false).as_deref(), Some("2 of 5"));
        assert_eq!(result_text("a", Some((0, 0)), false).as_deref(), Some("Phrase not found"));
        assert_eq!(result_text("a", Some((2, 5)), true).as_deref(), Some("2/5"));
        assert_eq!(result_text("a", Some((0, 0)), true).as_deref(), Some("None"));
    }
}
