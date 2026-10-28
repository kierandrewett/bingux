//! Whole-page messages: `StatusPage` (`AdwStatusPage`, SwiftUI
//! `ContentUnavailableView`) for empty and error pages. The dismissible
//! notice above content is [`crate::shell::Banner`].

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::button::action_button;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{BODY, HEADING, Ink, TextStyle};

/// The widest a status page's text gets.
const PAGE_MAX_WIDTH: f32 = 420.0;

/// Centred wrapping text that fills its box, as `centered_caption` does. A
/// bare centred label keeps a content-wide box but lays its text out across
/// the whole column, which pushes a short line off centre.
fn centred_paragraph(style: TextStyle, tokens: &Tokens, text: String) -> Paragraph {
    style
        .paragraph(tokens, text)
        .width(Size::fill())
        .max_width(Size::px(PAGE_MAX_WIDTH))
        .text_align(TextAlign::Center)
}

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
            .child(centred_paragraph(
                HEADING.in_ink(Ink::Text),
                &tokens,
                self.title.clone(),
            ))
            .maybe_child(
                self.description
                    .clone()
                    .map(|text| centred_paragraph(BODY.in_ink(Ink::Muted), &tokens, text)),
            )
            .maybe_child(self.action.clone().map(|(label, on_press)| {
                rect()
                    .tw(&tokens, &[mt(S3)])
                    .child(action_button(label, on_press))
            }))
            .maybe_child(self.child.clone())
    }
}
