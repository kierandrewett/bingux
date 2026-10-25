//! Whole-page and page-top messages: `StatusPage` (`AdwStatusPage`,
//! SwiftUI `ContentUnavailableView`) for empty and error pages, and
//! `Banner` (`AdwBanner`) for a dismissible notice above the content.

use freya::animation::*;
use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::button::{action_button, row_button};
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, HEADING, Ink};

/// The widest a status page's text gets.
const PAGE_MAX_WIDTH: f32 = 420.0;

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
            .component("StatusPage")
            .expanded()
            .tw(&tokens, &[flex_col(), center(), p(S8), gap(S3)])
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            w(Dim::Box24),
                            h(Dim::Box24),
                            rounded(Radius::Tile),
                            bg(Paint::Card),
                            center(),
                            mb(S2),
                        ],
                    )
                    .child(icons::icon(
                        self.icon,
                        IconSize::Hero,
                        Paint::Muted.color(&tokens),
                    )),
            )
            .child(
                HEADING
                    .in_ink(Ink::Text)
                    .label(&tokens, self.title.clone())
                    .text_align(TextAlign::Center),
            )
            .maybe_child(self.description.clone().map(|text| {
                rect().max_width(Size::px(PAGE_MAX_WIDTH)).child(
                    BODY.in_ink(Ink::Muted)
                        .label(&tokens, text)
                        .text_align(TextAlign::Center),
                )
            }))
            .maybe_child(self.action.clone().map(|(label, on_press)| {
                rect()
                    .tw(&tokens, &[mt(S3)])
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
        let tone = match self.kind {
            BannerKind::Info => Tone::Accent,
            BannerKind::Warning => Tone::Warning,
            BannerKind::Error => Tone::Danger,
        };
        // The banner and the space under it grow from nothing, so the page
        // below slides down rather than jumping.
        let height = Dim::Box11.px() * progress;
        let space_under = S4.px() * progress;
        let on_dismiss = self.on_dismiss.clone();
        Element::from(
            rect()
                .component("Banner")
                .tw(&tokens, &[flex_col(), w_full()])
                .opacity(progress)
                .child(
                    rect()
                        .tw(&tokens, &[w_full(), clip()])
                        .height(Size::px(height))
                        .child(
                            rect()
                                .a11y_role(AccessibilityRole::Status)
                                .a11y_alt(self.text.clone())
                                .tw(
                                    &tokens,
                                    &[
                                        w_full(),
                                        h(Dim::Box11),
                                        pl(S3),
                                        pr(S2),
                                        gap(S3),
                                        flex_row(),
                                        grow_children(),
                                        items(Align::Center),
                                        rounded(Radius::Card),
                                    ],
                                )
                                .tw(
                                    &tokens,
                                    &[
                                        bg(Paint::Tint(tone)),
                                        border(Line::Edge(Paint::TintEdge(tone))),
                                    ],
                                )
                                .child(
                                    BODY.label(&tokens, self.text.clone())
                                        .width(Size::flex(1.0)),
                                )
                                .maybe_child(self.action.clone().map(|(label, on_press)| {
                                    row_button(label, on_press).prominent()
                                }))
                                .maybe_child(on_dismiss.map(|on_dismiss| {
                                    rect()
                                        .tw(
                                            &tokens,
                                            &[
                                                w(Dim::Box7),
                                                h(Dim::Box7),
                                                rounded(Radius::Card),
                                                center(),
                                            ],
                                        )
                                        .a11y_role(AccessibilityRole::Button)
                                        .a11y_alt("Dismiss")
                                        .on_press(move |event: Event<PressEventData>| {
                                            event.stop_propagation();
                                            on_dismiss.call(());
                                        })
                                        .child(icons::icon(
                                            Icon::WindowClose,
                                            IconSize::Mark,
                                            Paint::Muted.color(&tokens),
                                        ))
                                })),
                        ),
                )
                .child(rect().height(Size::px(space_under))),
        )
    }
}
