//! The find card: find in a page, a document or a list, floating over the top
//! right of the content (`docs/CONSISTENCY.md` §2 of the Doop designs). One rule
//! for every app: a field with the count of matches, Previous and Next, the
//! app's options on a second row, and an X button that closes it (Escape too).
//! It floats over the content and never moves it; on a phone it spans the width
//! less the margins.
//!
//! ```ignore
//! find_card(text)
//!     .placeholder("Find in page")
//!     .result(Some((2, 5)))
//!     .on_change(move |text: String| search(text, true))
//!     .on_step(move |forward: bool| step(forward))
//!     .on_close(move |_: ()| close())
//! ```

use std::cell::Cell;
use std::rc::Rc;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::icons::Icon;
use crate::prelude::*;
use crate::typography;

/// The card's widest size on a desktop window.
const CARD_WIDTH: f32 = 420.0;

/// What the card says about the search: which match of how many, or that
/// there is none. `None` while nothing is searched for or no answer came yet.
pub fn find_result_text(text: &str, result: Option<(u32, u32)>, compact: bool) -> Option<String> {
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

/// One of the app's search options (Match case, Whole words, Highlight all).
#[derive(Clone, PartialEq)]
pub struct FindOption {
    pub label: String,
    pub on: bool,
    pub toggle: EventHandler<bool>,
}

impl FindOption {
    pub fn new(label: impl Into<String>, on: bool, toggle: impl Into<EventHandler<bool>>) -> Self {
        Self { label: label.into(), on, toggle: toggle.into() }
    }
}

/// The find card. Build it with [`find_card`].
#[derive(PartialEq)]
pub struct FindCard {
    text: State<String>,
    placeholder: String,
    result: Option<(u32, u32)>,
    focus: u32,
    options: Vec<FindOption>,
    change: Option<EventHandler<String>>,
    step: Option<EventHandler<bool>>,
    close: Option<EventHandler<()>>,
}

/// A find card over `text`, the field's state.
pub fn find_card(text: State<String>) -> FindCard {
    FindCard {
        text,
        placeholder: "Find".into(),
        result: None,
        focus: 0,
        options: Vec::new(),
        change: None,
        step: None,
        close: None,
    }
}

impl FindCard {
    /// The field's placeholder ("Find" by default; "Find in page" in a browser).
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// The current match and the number of matches, once the search answered.
    /// `(_, 0)` is "Phrase not found" and outlines the field in danger ink.
    pub fn result(mut self, result: Option<(u32, u32)>) -> Self {
        self.result = result;
        self
    }

    /// Counts the requests to focus the field: a new count focuses it with its
    /// text selected (Ctrl+F again on an open card).
    pub fn focus_requests(mut self, count: u32) -> Self {
        self.focus = count;
        self
    }

    /// The app's search options, as checkboxes on the card's second row (not on
    /// a phone).
    pub fn options(mut self, options: Vec<FindOption>) -> Self {
        self.options = options;
        self
    }

    /// The text changed: search again from the start.
    pub fn on_change(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.change = Some(handler.into());
        self
    }

    /// Next (`true`: Enter, the down button) or Previous (`false`: Shift+Enter,
    /// the up button).
    pub fn on_step(mut self, handler: impl Into<EventHandler<bool>>) -> Self {
        self.step = Some(handler.into());
        self
    }

    /// The X button or Escape.
    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.close = Some(handler.into());
        self
    }
}

impl Component for FindCard {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let text = self.text;
        let compact = is_compact();
        let enter_with_shift = use_hook(|| Rc::new(Cell::new(false)));
        let (change, step, close) = (self.change.clone(), self.step.clone(), self.close.clone());
        let current = text.read().clone();
        let not_found = !current.is_empty() && matches!(self.result, Some((_, 0)));

        let mut field = text_field(text, {
            let enter_with_shift = enter_with_shift.clone();
            let step = step.clone();
            move |_: String| {
                let backwards = enter_with_shift.replace(false);
                if let Some(step) = &step {
                    step.call(!backwards);
                }
            }
        })
        .flat()
        .fill_width()
        .leading(Icon::Search)
        .placeholder(&self.placeholder)
        .on_change(move |changed: String| {
            if let Some(change) = &change {
                change.call(changed);
            }
        })
        .on_escape({
            let close = close.clone();
            move |_: ()| {
                if let Some(close) = &close {
                    close.call(());
                }
            }
        })
        .on_enter_modifiers({
            let enter_with_shift = enter_with_shift.clone();
            move |held: Modifiers| enter_with_shift.set(held.contains(Modifiers::SHIFT))
        })
        .danger_outline(not_found);
        if self.focus > 0 {
            field = field.autofocus().select_all_on_open();
        }

        let status = find_result_text(&current, self.result, compact);
        let stepper = |forward: bool| {
            let step = step.clone();
            move |_: ()| {
                if let Some(step) = &step {
                    step.call(forward);
                }
            }
        };
        let top = rect()
            .tw(&tokens, &[w_full(), flex_row(), grow_children(), items(Align::Center), gap(S1)])
            .child(rect().key(self.focus).tw(&tokens, &[w_flex()]).child(field))
            .maybe_child(status.map(|status| {
                let ink = if not_found { Ink::Danger } else { Ink::Muted };
                Element::from(typography::CAPTION.in_ink(ink).label(&tokens, status).max_lines(1))
            }))
            .child(icon_button(Icon::PanUp, stepper(false)).tooltip("Previous match").enabled(!current.is_empty()))
            .child(icon_button(Icon::PanDown, stepper(true)).tooltip("Next match").enabled(!current.is_empty()))
            .child(
                icon_button(Icon::WindowClose, move |_: ()| {
                    if let Some(close) = &close {
                        close.call(());
                    }
                })
                .tooltip("Close (Escape)"),
            );
        let options = (!compact && !self.options.is_empty()).then(|| {
            rect().tw(&tokens, &[flex_row(), items(Align::Center), gap(S3), px(S1)]).children(
                self.options
                    .iter()
                    .map(|option| {
                        let toggle = option.toggle.clone();
                        checkbox(option.on, move |on: bool| toggle.call(on)).text(&option.label).into()
                    })
                    .collect::<Vec<Element>>(),
            )
        });
        let card = rect()
            .component("FindCard")
            .a11y_role(AccessibilityRole::Search)
            .a11y_alt(self.placeholder.clone())
            .width(if compact { Size::fill() } else { Size::px(CARD_WIDTH) })
            .tw(
                &tokens,
                &[
                    flex_col(),
                    gap(S1),
                    p(S1),
                    rounded(Radius::Overlay),
                    bg(Paint::Popover),
                    border(Line::Hairline),
                    shadow(Elevation::Raised),
                ],
            )
            .child(top)
            .maybe_child(options);
        // Floats at the content's top right, 8 px in; never moves the content.
        rect()
            .layer(Layer::Overlay)
            .position(Position::new_absolute().top(0.0).right(0.0))
            .width(if compact { Size::fill() } else { Size::auto() })
            .tw(&tokens, &[p(S2)])
            .child(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_says_which_match_of_how_many_or_that_there_are_none() {
        assert_eq!(find_result_text("", Some((1, 3)), false), None, "nothing searched for");
        assert_eq!(find_result_text("a", None, false), None, "no answer yet");
        assert_eq!(find_result_text("a", Some((2, 5)), false).as_deref(), Some("2 of 5"));
        assert_eq!(find_result_text("a", Some((0, 0)), false).as_deref(), Some("Phrase not found"));
        assert_eq!(find_result_text("a", Some((2, 5)), true).as_deref(), Some("2/5"));
        assert_eq!(find_result_text("a", Some((0, 0)), true).as_deref(), Some("None"));
    }
}
