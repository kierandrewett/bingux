//! Sheets: modal forms that slide down from the top of the window, as
//! GNOME's dialog header and SwiftUI `.sheet`. Cancel sits on the left, the
//! title in the middle and the primary action on the right. The body scrolls.
//!
//! Open one with [`Overlays::show_sheet`](super::overlays::Overlays::show_sheet).
//! Cancel and Escape close it. The primary action does not close it on its
//! own: the caller calls `close_sheet` once the work succeeds, so a failed
//! save can keep the form open.
//!
//! The body is stored when the sheet opens. It must read its values through
//! `State` handles, not plain values captured at open time.

use freya::prelude::*;

use crate::controls::button::{action_button, primary_button};
use crate::layout::divider;
use crate::scale::*;
use crate::shell::overlays::{Overlays, use_presence};
use crate::surface::use_tokens;

/// The width of a sheet, as a GNOME dialog of its kind.
const SHEET_WIDTH: f32 = 480.0;
/// The gap between the top and bottom of the window and the sheet.
const SHEET_MARGIN: f32 = S5;
/// How far the sheet drops in from above as it opens.
const SHEET_DROP: f32 = S3;
/// The height of the sheet header: padding, a compact control and the divider.
const SHEET_HEADER_HEIGHT: f32 = S3 * 2.0 + CONTROL_HEIGHT + 1.0;
/// The height of the tab bar slot under the header, when a sheet has tabs.
const SHEET_TABS_HEIGHT: f32 = BUTTON_HEIGHT + S2;
/// The tallest a sheet grows before its body scrolls, as a share of the window.
const SHEET_MAX_HEIGHT_PERCENT: f32 = 80.0;
/// One hundred percent, to turn `SHEET_MAX_HEIGHT_PERCENT` into a fraction.
const WHOLE_PERCENT: f32 = 100.0;

/// The primary action in a sheet header.
#[derive(Clone, PartialEq)]
pub struct SheetAction {
    pub label: String,
    pub enabled: bool,
    pub on_press: EventHandler<()>,
}

/// A modal form: a title, a body and an optional primary action.
#[derive(Clone, PartialEq)]
pub struct Sheet {
    pub title: String,
    pub body: Element,
    pub tabs: Option<Element>,
    pub primary: Option<SheetAction>,
}

/// A sheet titled `title` that shows `body`.
pub fn sheet(title: impl Into<String>, body: impl Into<Element>) -> Sheet {
    Sheet {
        title: title.into(),
        body: body.into(),
        tabs: None,
        primary: None,
    }
}

impl Sheet {
    /// Add the primary action on the right of the header, such as "Save".
    pub fn primary(
        mut self,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.primary = Some(SheetAction {
            label: label.into(),
            enabled: true,
            on_press: on_press.into(),
        });
        self
    }

    /// Enable or disable the primary action. Explain why in the body when
    /// it is disabled.
    pub fn primary_enabled(mut self, enabled: bool) -> Self {
        if let Some(action) = self.primary.as_mut() {
            action.enabled = enabled;
        }
        self
    }

    /// A tab bar under the header, above the body. It stays put while the
    /// body scrolls, so use it to switch between pages of one form.
    pub fn tabs(mut self, tabs: impl Into<Element>) -> Self {
        self.tabs = Some(tabs.into());
        self
    }
}

/// Draws the scrim and the sheet over the window. Rendered by
/// [`OverlayHost`](super::overlays::OverlayHost) while a sheet is open.
#[derive(PartialEq)]
pub(crate) struct SheetLayer {
    pub overlays: Overlays,
    pub sheet: Sheet,
    pub closing: bool,
}

impl Component for SheetLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let a11y_id = use_a11y();
        let progress = use_presence(0, self.closing, tokens.motion(140), move |_| {
            overlays.finish_sheet()
        });
        let cancel = action_button("Cancel", move |_: ()| overlays.close_sheet()).compact();
        let primary = self.sheet.primary.clone();
        let primary_press = primary.clone();
        let primary_element = primary.map(|action| {
            let on_press = action.on_press.clone();
            Element::from(
                primary_button(action.label, move |_: ()| on_press.call(()))
                    .compact()
                    .enabled(action.enabled),
            )
        });

        let header = rect()
            .width(Size::fill())
            .content(Content::Flex)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .padding((S3, S3, S3, S3))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::Start)
                    .child(cancel),
            )
            .child(
                label()
                    .width(Size::flex(1.0))
                    .text(self.sheet.title.clone())
                    .font_size(TEXT_LG)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(tokens.text)
                    .text_align(TextAlign::Center)
                    .max_lines(1),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::End)
                    .maybe_child(primary_element),
            );

        let header_height = SHEET_HEADER_HEIGHT
            + if self.sheet.tabs.is_some() {
                SHEET_TABS_HEIGHT
            } else {
                0.0
            };
        // The panel takes focus when the sheet opens. A text field marked
        // `autofocus` is created after it, and freya keeps the last request.
        let panel = rect()
            .a11y_id(a11y_id)
            .a11y_auto_focus(true)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt(self.sheet.title.clone())
            .width(Size::px(SHEET_WIDTH))
            .height(Size::auto())
            .max_height(Size::window_percent(SHEET_MAX_HEIGHT_PERCENT))
            .corner_radius(RADIUS_XL)
            .background(tokens.popover)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(tokens.outline),
            )
            .shadow(
                Shadow::new()
                    .position(ShadowPosition::Normal)
                    .y(16.0)
                    .blur(48.0)
                    .color(Color::BLACK.with_af32(0.45)),
            )
            .opacity(progress)
            .offset_y(-SHEET_DROP * (1.0 - progress))
            .on_press(|event: Event<PressEventData>| event.stop_propagation())
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Named(NamedKey::Escape) {
                    event.stop_propagation();
                    overlays.close_sheet();
                } else if event.key == Key::Named(NamedKey::Enter)
                    && let Some(action) = primary_press.as_ref()
                    && action.enabled
                {
                    event.stop_propagation();
                    action.on_press.call(());
                }
            })
            .child(header)
            .child(divider(&tokens))
            .maybe_child(self.sheet.tabs.clone().map(|tabs| {
                rect()
                    .width(Size::fill())
                    .height(Size::px(SHEET_TABS_HEIGHT))
                    .padding((S2, S4, 0.0, S4))
                    .child(tabs)
            }))
            .child(
                ScrollView::new()
                    .drag_scrolling(false)
                    .auto_hide_scrollbars(false)
                    .width(Size::fill())
                    .height(Size::auto())
                    .max_height(Size::func(move |ctx| {
                        Some(ctx.root / WHOLE_PERCENT * SHEET_MAX_HEIGHT_PERCENT - header_height)
                    }))
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(S4)
                            .child(self.sheet.body.clone()),
                    ),
            );

        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .layer(Layer::OverlayLevel(8))
            .cross_align(Alignment::Center)
            .padding((SHEET_MARGIN, 0.0, SHEET_MARGIN, 0.0))
            .background(Color::BLACK.with_af32(0.4 * progress))
            .child(panel)
    }
}
