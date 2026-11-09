//! Buttons joined in one rounded bar: Back and Forward, the choices of a view
//! switch.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::button::icon_button;
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;

/// One button of a [`button_group`]. Build it with [`group_button`].
#[derive(Clone, PartialEq)]
pub struct GroupButton {
    icon: Icon,
    label: String,
    on_press: EventHandler<()>,
    active: bool,
    enabled: bool,
}

/// A button of a group: `label` is its tooltip and its accessible name.
pub fn group_button(
    icon: Icon,
    label: impl Into<String>,
    on_press: impl Into<EventHandler<()>>,
) -> GroupButton {
    GroupButton {
        icon,
        label: label.into(),
        on_press: on_press.into(),
        active: false,
        enabled: true,
    }
}

impl GroupButton {
    /// Draws the button on: the choice of a view switch that is in use.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Dims the button and ignores presses; it keeps its place.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Icon buttons joined in one rounded bar with a hairline between each pair,
/// the height of a header button. Each button keeps its own Tab stop and
/// tooltip. Use it for a pair or a few that act on the same thing: Back and
/// Forward, a switch between a list and a grid. A [`toolbar`](super::toolbar)
/// builds the same bar for a [`linked`](super::ToolbarGroup::linked) group.
#[derive(Clone, PartialEq)]
pub struct ButtonGroup {
    buttons: Vec<GroupButton>,
}

/// A bar of `buttons`.
pub fn button_group(buttons: Vec<GroupButton>) -> ButtonGroup {
    ButtonGroup { buttons }
}

impl Component for ButtonGroup {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let buttons = self.buttons.iter().map(|button| {
            Element::from(
                icon_button(button.icon, button.on_press.clone())
                    .tooltip(&button.label)
                    .size(Dim::Control)
                    .active(button.active)
                    .enabled(button.enabled),
            )
        });
        linked_bar(&tokens, buttons.collect())
    }
}

/// `buttons` side by side on one rounded fill, with hairlines between.
pub(super) fn linked_bar(tokens: &crate::theme::Tokens, buttons: Vec<Element>) -> Element {
    let mut bar = rect().component("ButtonGroup").tw(
        tokens,
        &[
            flex_row(),
            items(Align::Center),
            rounded(Radius::Control),
            bg(Paint::Control),
            clip(),
        ],
    );
    let last = buttons.len().saturating_sub(1);
    for (at, button) in buttons.into_iter().enumerate() {
        bar = bar.child(button);
        if at < last {
            bar = bar.child(rect().tw(
                tokens,
                &[w(Dim::Hairline), h(Dim::Box5), bg(Paint::Separator)],
            ));
        }
    }
    bar.into()
}
