//! The toolbar's Reload button, which is Stop while the folder is being read.
//!
//! One `IconButton` throughout, so its box, hover, pressed and focus state
//! carry through the change; only the icon morphs (`IconButton::morph`: the
//! reload arrows turn, shrink and fade as the stop square turns in), and the
//! tooltip, which is also the button's accessible name, and the press
//! handler follow the state:
//!
//! | State | Icon | Name | Press |
//! |---|---|---|---|
//! | resting | reload | `Refresh (F5)` | read the folder again |
//! | a folder is being read | stop | `Stop (Esc)` | stop that read; the listing stays |
//!
//! The Stop state is not the raw state of the load: it shows once a read has
//! gone on for 150 ms and stays for at least 300 ms
//! ([`Dwell::LOADING`](crate::motion::Dwell::LOADING)), so a folder that
//! opens at once never flashes it, and one that ends just after it shows
//! does not take it away in the middle of the morph. During that last
//! moment nothing is running, so pressing it does nothing; Escape is
//! untouched (it falls through to what it would be without the button).
//! F5 and Ctrl+R read again whatever the button shows.

use super::directory::Reload;
use crate::controls::{IconButton, icon_button};
use crate::icons::Icon;
use crate::style::Dim;

/// The name and tooltip of the button in a state: `stopping` is whether the
/// Stop state shows.
pub(super) fn label(stopping: bool) -> &'static str {
    if stopping {
        "Stop (Esc)"
    } else {
        "Refresh (F5)"
    }
}

/// The button. `enabled` is false where there is no folder to read (Recent).
pub(super) fn reload_button(reload: &Reload, stopping: bool, enabled: bool) -> IconButton {
    let on_press = {
        let reload = reload.clone();
        move |_: ()| {
            if stopping {
                if reload.loading_now() {
                    reload.stop();
                }
            } else {
                reload.run();
            }
        }
    };
    icon_button(Icon::Refresh, on_press)
        .size(Dim::Control)
        .tooltip(label(stopping))
        .enabled(enabled)
        .morph(Icon::MediaPlaybackStop, stopping)
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn the_name_follows_the_state() {
        assert_eq!(label(false), "Refresh (F5)");
        assert_eq!(label(true), "Stop (Esc)");
    }
}
