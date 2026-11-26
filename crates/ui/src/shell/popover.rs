//! A floating card against a control that holds anything: the page info of a
//! browser, a list of downloads, the details of a connection.
//!
//! A menu is a list of text items and a popup is a modal panel in the middle
//! of the window. Between them sits what a toolbar button opens: a small card
//! under the button with groups and rows in it, that goes when the person
//! presses outside it or Escape. It is the same [`Panel`] every other floating
//! surface is, with the same shield, motion, placement (below the control,
//! above when there is no room, kept inside the window) and Back handling;
//! the one thing it adds is a size that follows its content, so the caller
//! does not count pixels.
//!
//! ```ignore
//! popover(open(), &button_area(), page_info_groups())
//!     .width(Dim::MenuWide)
//!     .on_dismiss(move |_| open.set(false))
//! ```
//!
//! [`Popover::attached`] is the variant for a list under a text field (the
//! address bar's suggestions): no shield, so the keyboard and the pointer
//! stay with the field, and as wide as the field.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink};

use super::panel::{Panel, PanelKind, PanelRect, PanelTrigger};

/// The height a popover is placed with until its content has been measured,
/// which is the next frame.
const FIRST_HEIGHT: f32 = 160.0;

/// A card of `content` that opens against `anchor`, a control's area in the
/// window (read it from the control's `on_sized`). `open` shows it; closing
/// plays the exit motion.
pub fn popover(open: bool, anchor: &Area, content: impl Into<Element>) -> Popover {
    Popover {
        open,
        anchor: PanelRect::from_area(anchor),
        width: Dim::MenuWide.px(),
        attached: false,
        content: content.into(),
        on_dismiss: EventHandler::new(|_: ()| {}),
    }
}

#[derive(Clone, PartialEq)]
pub struct Popover {
    open: bool,
    anchor: PanelRect,
    width: f32,
    attached: bool,
    content: Element,
    on_dismiss: EventHandler<()>,
}

impl Popover {
    /// The card's width. Default [`Dim::MenuWide`] (300 px); never wider than
    /// the window less its edges.
    pub fn width(mut self, width: Dim) -> Self {
        self.width = width.px();
        self
    }

    /// The card's width in pixels, for a card that fills a phone's window
    /// less its margins. Prefer [`Popover::width`] with a token.
    pub fn width_px(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// A list under a field: as wide as the control it is against, with no
    /// shield, so typing and the pointer go on working in the field. The
    /// owner closes it, and keeps the keys that move through it.
    pub fn attached(mut self) -> Self {
        self.attached = true;
        self
    }

    /// A press outside the card, Escape or Back asks to close it.
    pub fn on_dismiss(mut self, on_dismiss: impl Into<EventHandler<()>>) -> Self {
        self.on_dismiss = on_dismiss.into();
        self
    }
}

impl Component for Popover {
    fn render(&self) -> impl IntoElement {
        let mut measured = use_state(|| None::<f32>);
        let width = if self.attached {
            self.anchor.width
        } else {
            self.width
        };
        // The card is as tall as its content: the wrapper reports its height,
        // and the panel is placed with it from the next frame on.
        let body = rect()
            .component("Popover")
            .width(Size::fill())
            .on_sized(move |event: Event<SizedEventData>| {
                measured.set_if_modified(Some(event.area.height()));
            })
            .child(self.content.clone());
        let panel = Panel::new(
            PanelKind::Menu,
            self.open,
            PanelTrigger::Control(self.anchor),
            vec![body.into()],
        )
        .size(width, measured().unwrap_or(FIRST_HEIGHT))
        .on_dismiss(self.on_dismiss.clone());
        if self.attached {
            panel.nested(0)
        } else {
            panel
        }
    }
}

/// One row of a list under a field, for an [`attached`](Popover::attached)
/// popover: an icon, a title and, in quieter ink after it, a detail (an
/// address, a path). It is a menu item's row (the same height, inset and
/// fill), so a list of suggestions reads as the menus do. The row the keys
/// have reached is `highlighted`; the owner of the field moves it, since the
/// keyboard stays in the field.
pub fn suggestion_row(
    icon: Icon,
    title: impl Into<String>,
    detail: impl Into<String>,
) -> SuggestionRow {
    SuggestionRow {
        icon,
        title: title.into(),
        detail: detail.into(),
        highlighted: false,
        on_press: EventHandler::new(|_: ()| {}),
        on_hover: EventHandler::new(|_: bool| {}),
    }
}

#[derive(Clone, PartialEq)]
pub struct SuggestionRow {
    icon: Icon,
    title: String,
    detail: String,
    highlighted: bool,
    on_press: EventHandler<()>,
    on_hover: EventHandler<bool>,
}

impl SuggestionRow {
    pub fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }

    pub fn on_press(mut self, on_press: impl Into<EventHandler<()>>) -> Self {
        self.on_press = on_press.into();
        self
    }

    /// The pointer entered the row (`true`) or left it.
    pub fn on_hover(mut self, on_hover: impl Into<EventHandler<bool>>) -> Self {
        self.on_hover = on_hover.into();
        self
    }
}

impl Component for SuggestionRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let (on_press, on_enter, on_leave) = (
            self.on_press.clone(),
            self.on_hover.clone(),
            self.on_hover.clone(),
        );
        let name = if self.detail.is_empty() {
            self.title.clone()
        } else {
            format!("{}, {}", self.title, self.detail)
        };
        let mut label = paragraph()
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .width(Size::flex(1.0))
            .span(BODY.in_ink(Ink::Text).span(&tokens, self.title.clone()));
        if !self.detail.is_empty() {
            label = label.span(
                BODY.in_ink(Ink::Muted)
                    .span(&tokens, format!("  {}", self.detail)),
            );
        }
        rect()
            .component("SuggestionRow")
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_alt(name)
            .tw(&tokens, MENU_ITEM)
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering() || self.highlighted,
                    false,
                    self.highlighted,
                )],
            )
            .on_pointer_enter(move |_| {
                hovering.set(true);
                on_enter.call(true);
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
                Paint::Muted.color(&tokens),
            ))
            .child(label)
    }
}
