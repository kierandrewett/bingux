//! A breadcrumb trail: a row of crumbs separated by chevrons, such as an
//! inspector's ancestors of the selected element or a file manager's path.
//! Each crumb before the last is a Tab stop that selects it when pressed or
//! activated. The last crumb is the current place, in normal ink.

use freya::prelude::*;

use super::focus_stop::focus_stop;
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CODE, Ink};

/// A row of crumbs. `crumbs` pairs each crumb's key with its label, outermost
/// first; `on_select` is called with the key of the crumb the user picked.
#[derive(PartialEq)]
pub struct Breadcrumb<K: Clone + PartialEq + 'static> {
    crumbs: Vec<(K, String)>,
    on_select: EventHandler<K>,
    code: bool,
}

/// A breadcrumb trail of `crumbs`. The row scrolls sideways when it is long.
pub fn breadcrumb<K: Clone + PartialEq + 'static>(
    crumbs: Vec<(K, String)>,
    on_select: impl Into<EventHandler<K>>,
) -> Breadcrumb<K> {
    Breadcrumb {
        crumbs,
        on_select: on_select.into(),
        code: false,
    }
}

impl<K: Clone + PartialEq + 'static> Breadcrumb<K> {
    /// Set the crumbs in the code face, in a code-height row, as an element
    /// inspector's trail is.
    pub fn code(mut self) -> Self {
        self.code = true;
        self
    }
}

impl<K: Clone + PartialEq + 'static> Component for Breadcrumb<K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let last = self.crumbs.len().saturating_sub(1);
        let mut row: Vec<Element> = Vec::new();
        for (index, (key, text)) in self.crumbs.iter().enumerate() {
            if index > 0 {
                row.push(
                    icons::icon(Icon::GoNext, IconSize::Text, Ink::Muted.color(&tokens)).into(),
                );
            }
            row.push(Element::from(Crumb {
                key: key.clone(),
                text: text.clone(),
                current: index == last,
                code: self.code,
                on_select: self.on_select.clone(),
            }));
        }
        let height = if self.code {
            Dim::CodeRow
        } else {
            Dim::Control
        };
        rect()
            .component("Breadcrumb")
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(height),
                    flex_row(),
                    items(Align::Center),
                    px(S2),
                    clip(),
                    bg(Paint::Surface),
                ],
            )
            .child(
                scroll_x()
                    .width(Size::fill())
                    .height(Size::fill())
                    .children(row),
            )
    }
}

#[derive(PartialEq)]
struct Crumb<K: Clone + PartialEq + 'static> {
    key: K,
    text: String,
    current: bool,
    code: bool,
    on_select: EventHandler<K>,
}

impl<K: Clone + PartialEq + 'static> Component for Crumb<K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ink = if self.current { Ink::Text } else { Ink::Muted };
        let label = if self.code {
            CODE.in_ink(ink).label(&tokens, self.text.clone())
        } else {
            BODY.in_ink(ink).label(&tokens, self.text.clone())
        };
        let key = self.key.clone();
        let on_select = self.on_select.clone();
        let content = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    px(S1),
                    rounded(Radius::Mark),
                ],
            )
            .child(label);
        focus_stop(
            content,
            self.text.clone(),
            AccessibilityRole::Button,
            true,
            Radius::Mark,
            move |_| on_select.call(key.clone()),
        )
        .into_element()
    }
}
