//! Whole-page and page-top messages: `StatusPage` (`AdwStatusPage`,
//! SwiftUI `ContentUnavailableView`) for empty and error pages, and
//! `Banner` (`AdwBanner`) for a dismissible notice above the content.

use freya::animation::*;
use freya::prelude::*;

use crate::controls::button::{action_button, row_button};
use crate::icons::{self, Icon};
use crate::surface::{CORNER, FONT_SIZE, use_tokens};

/// A centred icon in a muted tile, a title, a description and an
/// optional action. Fills its parent.
#[derive(PartialEq)]
pub struct StatusPage {
    icon: Icon,
    title: String,
    description: Option<String>,
    action: Option<(String, EventHandler<()>)>,
    child: Option<Element>,
}

pub fn status_page(icon: Icon, title: impl Into<String>) -> StatusPage {
    StatusPage {
        icon,
        title: title.into(),
        description: None,
        action: None,
        child: None,
    }
}

impl StatusPage {
    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = Some(text.into());
        self
    }

    /// An elevated button under the description.
    pub fn action(
        mut self,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.action = Some((label.into(), on_press.into()));
        self
    }

    /// Any element under the description, instead of or after the action.
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.child = Some(child.into());
        self
    }
}

impl Component for StatusPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .expanded()
            .center()
            .padding(32.0)
            .spacing(12.0)
            .child(
                rect()
                    .width(Size::px(96.0))
                    .height(Size::px(96.0))
                    .corner_radius(28.0)
                    .background(tokens.elevated)
                    .center()
                    .margin((0.0, 0.0, 8.0, 0.0))
                    .child(icons::icon(self.icon, 48.0, tokens.muted)),
            )
            .child(
                label()
                    .text(self.title.clone())
                    .font_size(22.0)
                    .font_weight(FontWeight::BOLD)
                    .color(tokens.text)
                    .text_align(TextAlign::Center),
            )
            .maybe_child(self.description.clone().map(|text| {
                paragraph()
                    .max_width(Size::px(420.0))
                    .color(tokens.muted)
                    .font_size(FONT_SIZE)
                    .text_align(TextAlign::Center)
                    .child(text)
            }))
            .maybe_child(self.action.clone().map(|(label, on_press)| {
                rect()
                    .margin((12.0, 0.0, 0.0, 0.0))
                    .child(action_button(label, on_press))
            }))
            .maybe_child(self.child.clone())
    }
}

/// The kind of notice a banner carries; it sets the tint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind {
    Info,
    Warning,
    Error,
}

/// A full-width tinted bar with a message, an optional button and an
/// optional dismiss cross. It slides open over 160 ms when `visible`
/// turns on (no motion under reduced motion).
#[derive(PartialEq)]
pub struct Banner {
    text: String,
    kind: BannerKind,
    visible: bool,
    action: Option<(String, EventHandler<()>)>,
    on_dismiss: Option<EventHandler<()>>,
}

pub fn banner(text: impl Into<String>) -> Banner {
    Banner {
        text: text.into(),
        kind: BannerKind::Info,
        visible: true,
        action: None,
        on_dismiss: None,
    }
}

impl Banner {
    pub fn kind(mut self, kind: BannerKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// A pill button at the right.
    pub fn action(
        mut self,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.action = Some((label.into(), on_press.into()));
        self
    }

    /// Show a dismiss cross that calls `on_dismiss`; the caller hides the
    /// banner by rendering it with `visible(false)`.
    pub fn dismissible(mut self, on_dismiss: impl Into<EventHandler<()>>) -> Self {
        self.on_dismiss = Some(on_dismiss.into());
        self
    }
}

impl Component for Banner {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let motion = tokens.motion(160);
        let reveal =
            use_animation_transition(if self.visible { 1.0 } else { 0.0 }, move |from, to| {
                AnimNum::new(from, to)
                    .time(motion)
                    .function(Function::Cubic)
                    .ease(Ease::Out)
            });
        let progress = reveal.get().value();
        if progress <= 0.0 {
            return rect().height(Size::px(0.0)).into();
        }
        let tint = match self.kind {
            BannerKind::Info => tokens.accent,
            BannerKind::Warning => tokens.warning,
            BannerKind::Error => tokens.danger,
        };
        let on_dismiss = self.on_dismiss.clone();
        Element::from(
            rect()
                .width(Size::fill())
                .height(Size::px(44.0 * progress))
                .overflow(Overflow::Clip)
                .opacity(progress)
                .margin((0.0, 0.0, 16.0 * progress, 0.0))
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Status)
                        .a11y_alt(self.text.clone())
                        .width(Size::fill())
                        .height(Size::px(44.0))
                        .padding((0.0, 8.0, 0.0, 14.0))
                        .spacing(12.0)
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .corner_radius(CORNER)
                        .background(tint.with_af32(0.16))
                        .border(
                            Border::new()
                                .width(1.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(tint.with_af32(0.28)),
                        )
                        .child(
                            label()
                                .width(Size::flex(1.0))
                                .text(self.text.clone())
                                .font_size(FONT_SIZE)
                                .color(tokens.text)
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        )
                        .maybe_child(
                            self.action
                                .clone()
                                .map(|(label, on_press)| row_button(label, on_press).prominent()),
                        )
                        .maybe_child(on_dismiss.map(|on_dismiss| {
                            rect()
                                .width(Size::px(28.0))
                                .height(Size::px(28.0))
                                .corner_radius(14.0)
                                .center()
                                .cursor(CursorIcon::Pointer)
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Dismiss")
                                .on_press(move |event: Event<PressEventData>| {
                                    event.stop_propagation();
                                    on_dismiss.call(());
                                })
                                .child(icons::icon(Icon::WindowClose, 12.0, tokens.muted))
                        })),
                ),
        )
    }
}
