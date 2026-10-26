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
    flat: bool,
}

/// A breadcrumb trail of `crumbs`. A trail too long for its width drops its
/// first crumbs behind an ellipsis, so the current place stays in view.
pub fn breadcrumb<K: Clone + PartialEq + 'static>(
    crumbs: Vec<(K, String)>,
    on_select: impl Into<EventHandler<K>>,
) -> Breadcrumb<K> {
    Breadcrumb {
        crumbs,
        on_select: on_select.into(),
        code: false,
        flat: false,
    }
}

impl<K: Clone + PartialEq + 'static> Breadcrumb<K> {
    /// Set the crumbs in the code face, in a code-height row, as an element
    /// inspector's trail is.
    pub fn code(mut self) -> Self {
        self.code = true;
        self
    }

    /// Draw no fill, so the trail sits on the fill of the box it is in, such
    /// as a path field.
    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }
}

/// A generous advance of one character in the code and the body face, to
/// decide how many crumbs fit. Freya cannot measure text before layout.
const CODE_ADVANCE: f32 = 8.0;
const BODY_ADVANCE: f32 = 8.6;

impl<K: Clone + PartialEq + 'static> Breadcrumb<K> {
    /// The width one crumb takes with the chevron before it.
    fn crumb_width(&self, text: &str) -> f32 {
        let advance = if self.code {
            CODE_ADVANCE
        } else {
            BODY_ADVANCE
        };
        text.chars().count() as f32 * advance + IconSize::Text.px() + 2.0 * S1.px() + S2.px()
    }
}

impl<K: Clone + PartialEq + 'static> Component for Breadcrumb<K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut width = use_state(|| 0.0_f32);
        let last = self.crumbs.len().saturating_sub(1);
        // The tail that fits: from the end back while the room lasts. Before the
        // first measure every crumb is shown.
        let room = width() - 2.0 * S2.px() - IconSize::Text.px() - S2.px();
        let mut first = 0;
        if room > 0.0 {
            let mut used = 0.0;
            first = self.crumbs.len();
            for (index, (_, text)) in self.crumbs.iter().enumerate().rev() {
                used += self.crumb_width(text);
                if used > room && first != self.crumbs.len() {
                    break;
                }
                first = index;
            }
        }
        let chevron = |tokens: &crate::theme::Tokens| -> Element {
            rect()
                .tw(tokens, &[flex_row(), h_full(), items(Align::Center)])
                .child(icons::icon(
                    Icon::GoNext,
                    IconSize::Text,
                    Ink::Muted.color(tokens),
                ))
                .into()
        };
        let mut row: Vec<Element> = Vec::new();
        if first > 0 {
            row.push(
                rect()
                    .tw(
                        &tokens,
                        &[flex_row(), h_full(), items(Align::Center), px(S1)],
                    )
                    .child(
                        BODY.in_ink(Ink::Muted)
                            .label(&tokens, "…".to_string())
                            .max_lines(1),
                    )
                    .into(),
            );
        }
        for (index, (key, text)) in self.crumbs.iter().enumerate().skip(first) {
            if index > 0 {
                row.push(chevron(&tokens));
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
                    bg(if self.flat {
                        Paint::Transparent
                    } else {
                        Paint::Surface
                    }),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                width.set_if_modified(event.area.width());
            })
            .children(row)
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
            CODE.in_ink(ink)
                .label(&tokens, self.text.clone())
                .max_lines(1)
        } else {
            BODY.in_ink(ink)
                .label(&tokens, self.text.clone())
                .max_lines(1)
        };
        let key = self.key.clone();
        let on_select = self.on_select.clone();
        let content = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    h_full(),
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
        .tw(&tokens, &[h_full()])
        .into_element()
    }
}
