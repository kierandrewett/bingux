//! The connection of the page as a button beside the address: its icon opens
//! the page info.

use bingux_ui::prelude::*;
use freya::prelude::*;

use super::{Summary, Tone};
use crate::ui::Ctx;

impl Tone {
    /// The ink the connection button's icon takes.
    fn ink(self) -> Option<Ink> {
        match self {
            Self::Neutral | Self::Good => None,
            Self::Warning => Some(Ink::Warning),
            Self::Danger => Some(Ink::Danger),
        }
    }
}

/// The connection of the page: its icon opens the page info.
pub struct ConnectionButton;

impl ConnectionButton {
    pub fn element(ctx: Ctx) -> Element {
        let summary = Summary::of_tab(ctx.browser.read().active());
        let state = ctx.site_info;
        let mut anchor = state.anchor;
        let mut button =
            icon_button(summary.icon, move |_: ()| state.open()).tooltip(&summary.headline);
        if let Some(ink) = summary.tone.ink() {
            button = button.ink(ink);
        }
        rect()
            .on_sized(move |event: Event<SizedEventData>| anchor.set_if_modified(event.area))
            .child(button)
            .into()
    }
}
