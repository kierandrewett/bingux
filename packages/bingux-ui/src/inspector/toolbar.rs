//! The inspector's header bar, the shell's own: the inspect and phone-size
//! toggles at the left, the tabs as a segmented rail in the middle, and the
//! pause, refresh and window controls at the right. The controls act on the
//! plugin's state, and the window passes their current values in so the
//! buttons show them.

use freya::prelude::*;

use super::nav::Tab;
use crate::controls::{icon_button, segmented};
use crate::icons::Icon;
use crate::shell::{HeaderBar, WindowControls};
use crate::style::*;

/// The header bar. `inspecting` and `paused` light the matching button.
/// `narrow` lets the tabs scroll sideways instead of running under the
/// controls.
pub(super) fn header(
    tab: Tab,
    tab_state: State<Tab>,
    inspecting: bool,
    paused: bool,
    narrow: bool,
) -> Element {
    let pause_label = if paused {
        "Resume updates"
    } else {
        "Pause updates"
    };
    let phone = super::phone_preview();
    let tabs = segmented(
        Tab::ALL
            .iter()
            .map(|tab| (tab.key().to_string(), tab.title().to_string()))
            .collect(),
        tab.key(),
        move |picked: String| {
            if let Some(picked) = Tab::from_key(&picked) {
                let mut tab_state = tab_state;
                tab_state.set(picked);
            }
        },
    )
    .tabs();
    let tabs: Element = if narrow {
        tabs.scroll().into_element()
    } else {
        tabs.into_element()
    };
    HeaderBar::new("Inspector")
        .leading(
            icon_button(Icon::Inspect, move |_| {
                super::set_picking(!super::picking());
            })
            .tooltip("Inspect an element (Ctrl+Shift+C)")
            .active(inspecting),
        )
        .leading(
            icon_button(Icon::Phone, move |_| super::set_phone_preview(!phone))
                .tooltip("Preview at phone size")
                .active(phone),
        )
        .title_slot(
            rect()
                .tw_layout(&[w_full(), flex_row(), justify(Align::Center)])
                .child(tabs),
        )
        .trailing(
            icon_button(Icon::Application, |_| super::inspect_self())
                .tooltip("Inspect this inspector (Ctrl+Alt+Shift+I)"),
        )
        .trailing(
            icon_button(Icon::Pause, move |_| super::set_paused(!paused))
                .tooltip(pause_label)
                .active(paused),
        )
        .trailing(icon_button(Icon::Refresh, |_| super::refresh()).tooltip("Refresh (F5)"))
        .trailing(Element::from(WindowControls))
        .into()
}
