//! The rows of the lists the address bar opens: the subfolders of a crumb,
//! the typed-path suggestions, the trail on a phone. One look for all of
//! them, built on the menu's row recipe.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::{SpinnerSize, spinner};
use crate::icons::{self, Icon};
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::use_tokens;
use crate::search_highlight;
use crate::shell::menu_query::Mark;
use crate::shell::type_ahead::dimmed;
use crate::typography::{BODY, EMPHASIS, Ink};

/// A piece of a row's text.
#[derive(Clone, PartialEq)]
pub(super) struct Part {
    pub text: String,
    /// Drawn heavier, for the part that matches what was typed.
    pub emphasis: bool,
    pub muted: bool,
}

impl Part {
    pub(super) fn plain(text: impl Into<String>) -> Part {
        Part {
            text: text.into(),
            emphasis: false,
            muted: false,
        }
    }

    pub(super) fn muted(text: impl Into<String>) -> Part {
        Part {
            text: text.into(),
            emphasis: false,
            muted: true,
        }
    }

    /// `name` with the bytes of `matched` heavier and `after` muted behind it.
    pub(super) fn matched(name: &str, matched: std::ops::Range<usize>, after: &str) -> Vec<Part> {
        let mut parts = Vec::new();
        if matched.start > 0 {
            parts.push(Part::plain(&name[..matched.start]));
        }
        if !matched.is_empty() {
            parts.push(Part {
                text: name[matched.clone()].to_string(),
                emphasis: true,
                muted: false,
            });
        }
        if matched.end < name.len() {
            parts.push(Part::plain(&name[matched.end..]));
        }
        if parts.is_empty() {
            parts.push(Part::plain(name));
        }
        if !after.is_empty() {
            parts.push(Part::muted(after));
        }
        parts
    }
}

/// One choosable row: an icon, a name and, for the current choice, a tick.
#[derive(PartialEq)]
pub(super) struct ListRow {
    pub icon: Icon,
    pub parts: Vec<Part>,
    /// The folder the trail is in: ticked and filled.
    pub current: bool,
    /// The keyboard's row.
    pub highlighted: bool,
    /// How the row shows while a typed query is active.
    pub mark: Mark,
    pub on_press: EventHandler<()>,
    pub on_hover: EventHandler<bool>,
}

impl Component for ListRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let on_press = self.on_press.clone();
        let on_hover = self.on_hover.clone();
        let on_leave = on_hover.clone();
        // A typed query marks the letters it matched in a one-piece name.
        let label: Element = match (&self.mark, self.parts.as_slice()) {
            (Mark::Hit(range), [only]) => search_highlight::highlighted_label(
                BODY.in_ink(Ink::Text),
                &tokens,
                &only.text,
                std::slice::from_ref(range),
            )
            .width(Size::flex(1.0))
            .into(),
            _ => {
                let mut label = paragraph()
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0));
                for part in &self.parts {
                    let style = if part.emphasis { EMPHASIS } else { BODY };
                    let style = style.in_ink(if part.muted { Ink::Muted } else { Ink::Text });
                    label = label.span(style.span(&tokens, part.text.clone()));
                }
                label.into()
            }
        };
        rect()
            .component("AddressRow")
            .a11y_role(AccessibilityRole::MenuItem)
            .tw(&tokens, MENU_ITEM)
            .maybe(dimmed(&self.mark).is_some(), |row| {
                row.opacity(dimmed(&self.mark).unwrap_or(1.0))
            })
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering() || self.highlighted || self.current,
                    false,
                    self.current,
                )],
            )
            .on_pointer_enter(move |_| {
                hovering.set(true);
                on_hover.call(true);
            })
            .on_pointer_leave(move |_| {
                hovering.set(false);
                on_leave.call(false);
            })
            // A press never reaches the field or the shield under the row.
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(());
            })
            .child(icons::icon(
                self.icon,
                IconSize::Control,
                Paint::Text.color(&tokens),
            ))
            .child(label)
            .child(icons::icon(
                Icon::Check,
                IconSize::Text,
                if self.current {
                    Paint::Accent.color(&tokens)
                } else {
                    Paint::Transparent.color(&tokens)
                },
            ))
    }
}

/// A row that says something instead of offering a choice: "Loading…",
/// "No subfolders", the reason a path cannot be opened.
#[derive(PartialEq)]
pub(super) struct NoticeRow {
    pub text: String,
    pub spinning: bool,
    pub ink: Ink,
}

impl NoticeRow {
    pub(super) fn quiet(text: impl Into<String>) -> NoticeRow {
        NoticeRow {
            text: text.into(),
            spinning: false,
            ink: Ink::Muted,
        }
    }

    pub(super) fn busy(text: impl Into<String>) -> NoticeRow {
        NoticeRow {
            text: text.into(),
            spinning: true,
            ink: Ink::Muted,
        }
    }
}

impl Component for NoticeRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .component("AddressNotice")
            .tw(&tokens, MENU_ITEM)
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .maybe_child(
                self.spinning
                    .then(|| Element::from(spinner(SpinnerSize::Small))),
            )
            .child(
                BODY.in_ink(self.ink)
                    .label(&tokens, self.text.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0)),
            )
    }
}

/// The scroll offset that shows row `index` of `count` rows in a list capped
/// at the menu's height. With `centre` the row sits in the middle, else it
/// moves only as far as it must to come into view from `current`.
pub(super) fn scroll_offset(index: usize, count: usize, current: i32, centre: bool) -> i32 {
    let row = Dim::MenuItem.px();
    let list = count as f32 * row;
    let viewport = list.min(Dim::MenuList.px());
    let top = index as f32 * row;
    let furthest = (list - viewport).max(0.0);
    let want = if centre {
        top - (viewport - row) / 2.0
    } else {
        let shown = -(current as f32);
        if top < shown {
            top
        } else if top + row > shown + viewport {
            top + row - viewport
        } else {
            shown
        }
    };
    -(want.clamp(0.0, furthest) as i32)
}

/// The height of `count` rows, for a panel's content size.
pub(super) fn rows_height(count: usize) -> f32 {
    count as f32 * Dim::MenuItem.px()
}

/// A list under a field, with no shield: the keyboard and the pointer stay
/// with the field. It is as wide as `field` and scrolls past the menu height.
pub(super) fn popover(
    open: bool,
    field: &Area,
    rows: Vec<Element>,
    scroll: ScrollController,
) -> Panel {
    let anchor = PanelRect::from_area(field);
    let height = rows_height(rows.len());
    Panel::new(PanelKind::Menu, open, PanelTrigger::Control(anchor), rows)
        .size(anchor.width, height)
        .scroll(scroll)
        .nested()
}
