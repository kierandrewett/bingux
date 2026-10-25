//! The inspector's header, in Chrome's order: the inspect and phone-size
//! toggles, a separator, the tab strip, then the flexible space and the
//! pause, refresh and close controls. The controls act on the plugin's state,
//! and the window passes their current values in so the buttons show them.

use freya::prelude::*;

use super::nav::{Tab, tab_strip};
use crate::controls::icon_button;
use crate::icons::Icon;
use crate::style::*;
use crate::theme::Tokens;

/// A toolbar button: 28 px square with a 4 px corner, as Chrome's.
fn tool(icon: Icon, on_press: impl Into<EventHandler<()>>) -> crate::controls::IconButton {
    icon_button(icon, on_press)
        .size(Dim::Box7)
        .radius(Radius::Mark)
}

/// The hairline between groups of controls.
fn divider(tokens: &Tokens) -> Rect {
    rect().tw(
        tokens,
        &[w(Dim::Hairline), h(Dim::IconControl), mx(S2), bg(Paint::Separator)],
    )
}

/// The header row. `inspecting` and `paused` light the matching button.
pub(super) fn toolbar(
    tokens: &Tokens,
    tab: Tab,
    tab_state: State<Tab>,
    inspecting: bool,
    paused: bool,
) -> Element {
    let pause_label = if paused {
        "Resume updates"
    } else {
        "Pause updates"
    };
    let phone = super::phone_preview();
    rect()
        .tw(tokens, &[w_full(), flex_col(), bg(Paint::Sidebar)])
        .child(
            rect()
                .tw(
                    tokens,
                    &[w_full(), flex_row(), items(Align::Center), px(S2), py(S1)],
                )
                .child(
                    tool(Icon::Inspect, move |_| {
                        super::set_picking(!super::picking());
                    })
                    .tooltip("Inspect an element (Ctrl+Shift+C)")
                    .active(inspecting),
                )
                .child(
                    tool(Icon::Phone, move |_| super::set_phone_preview(!phone))
                        .tooltip("Preview at phone size")
                        .active(phone),
                )
                .child(divider(tokens))
                .child(tab_strip(tab, move |picked: Tab| {
                    let mut tab_state = tab_state;
                    tab_state.set(picked);
                }))
                .child(rect().tw(tokens, &[w_flex()]))
                .child(
                    tool(Icon::Pause, move |_| super::set_paused(!paused))
                        .tooltip(pause_label)
                        .active(paused),
                )
                .child(tool(Icon::Refresh, |_| super::refresh()).tooltip("Refresh (F5)"))
                .child(
                    tool(Icon::WindowClose, |_| super::close_inspector())
                        .tooltip("Close inspector"),
                ),
        )
        .child(rect().tw(tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
        .into()
}
