//! A property row: the dense row of a tool or advanced settings pane. The
//! property's name sits at the left with an optional hint line under it (the
//! call or unit the value stands for, in the monospace face), its editor sits
//! at the right, and a changed property shows a reset button.
//!
//! It is the row of a [`Group`](crate::Group) for values that are edited in
//! place, as a form row is for settings. The inspector's Properties pane is
//! built from it; a developer-options page or a theme editor would be the
//! second user.

use freya::prelude::*;

use super::button::icon_button;
use crate::component::ComponentName;
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink, TextStyle};

/// The hint line: caption size in the monospace face, so a call reads as code.
const HINT: TextStyle = TextStyle {
    mono: true,
    ..CAPTION
};

/// A row for the property `name`. Add its editor with
/// [`control`](PropertyRow::control).
pub fn property_row(name: impl Into<String>) -> PropertyRow {
    PropertyRow {
        name: name.into(),
        hint: None,
        control: None,
        on_reset: None,
        key: None,
    }
}

#[derive(Clone, PartialEq)]
pub struct PropertyRow {
    name: String,
    hint: Option<String>,
    control: Option<Element>,
    on_reset: Option<EventHandler<()>>,
    key: Option<String>,
}

impl PropertyRow {
    /// The line under the name: what the value means in code or in units.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The editor at the row's end.
    pub fn control(mut self, control: impl Into<Element>) -> Self {
        self.control = Some(control.into());
        self
    }

    /// Marks the property as changed from what the app set, and shows a reset
    /// button that calls `on_reset`.
    pub fn on_reset(mut self, on_reset: impl Into<EventHandler<()>>) -> Self {
        self.on_reset = Some(on_reset.into());
        self
    }

    /// Keys the row, so its editor keeps its state across renders and does not
    /// borrow another row's.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl From<PropertyRow> for Element {
    fn from(row: PropertyRow) -> Element {
        let key = row.key.clone();
        let element = Element::from(PropertyRowView(row));
        match key {
            Some(key) => rect().key(key).child(element).into(),
            None => element,
        }
    }
}

#[derive(PartialEq)]
struct PropertyRowView(PropertyRow);

impl Component for PropertyRowView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let row = &self.0;
        let changed = row.on_reset.is_some();
        let name_ink = if changed { Ink::Accent } else { Ink::Text };
        let text = rect()
            .tw(&tokens, &[flex_col(), w_flex(), gap(S0_5)])
            .child(
                BODY.in_ink(name_ink)
                    .label(&tokens, row.name.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(row.hint.clone().map(|hint| {
                HINT.in_ink(Ink::Muted)
                    .label(&tokens, hint)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
            }));
        rect()
            .component("PropertyRow")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    min_h(Dim::Dense),
                    px(GUTTER),
                    py(S1),
                    gap(S2),
                ],
            )
            .child(text)
            .maybe_child(row.control.clone())
            .child(match row.on_reset.clone() {
                // The slot is kept when nothing changed, so every row's editor
                // ends at the same edge.
                Some(on_reset) => icon_button(Icon::Refresh, on_reset)
                    .size(Dim::Box7)
                    .radius(Radius::Mark)
                    .tooltip("Reset to the app's value")
                    .into_element(),
                None => rect()
                    .tw(&tokens, &[w(Dim::Box7), h(Dim::Box7)])
                    .into_element(),
            })
    }
}
