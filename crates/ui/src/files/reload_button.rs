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
//! | a subfolder search runs | stop | `Stop searching (Esc)` | stop the walk; what it found stays |
//!
//! The Stop state is not the raw state of the load: it shows once a read or a
//! search has gone on for 150 ms and stays for at least 300 ms
//! ([`Dwell::LOADING`](crate::motion::Dwell::LOADING)), so a folder that
//! opens at once never flashes it, and one that ends just after it shows
//! does not take it away in the middle of the morph. During that last
//! moment nothing is running, so pressing it does nothing; Escape is
//! untouched (it falls through to what it would be without the button).
//! F5 and Ctrl+R read again whatever the button shows.

use freya::prelude::*;

use super::directory::Reload;
use super::search::Deep;
use crate::controls::{IconButton, icon_button};
use crate::icons::Icon;
use crate::style::Dim;

/// The name and tooltip of the button in a state: `stopping` is whether the
/// Stop state shows, `loading` whether the folder is being read, and
/// `searching` whether a subfolder search runs.
pub(super) fn label(stopping: bool, loading: bool, searching: bool) -> &'static str {
    match (stopping, loading, searching) {
        (false, _, _) => "Refresh (F5)",
        (true, false, true) => "Stop searching (Esc)",
        (true, _, _) => "Stop (Esc)",
    }
}

/// Stop what is in flight: the read of the folder, and the search below it.
/// Both keep what they have.
pub(super) fn stop(reload: &Reload, deep: &Deep) {
    if reload.loading_now() {
        reload.stop();
    }
    if deep.running_now() {
        deep.stop();
    }
}

/// The button. `enabled` is false where there is no folder to read (Recent).
pub(super) fn reload_button(reload: &Reload, deep: &Deep, stopping: bool, enabled: bool) -> IconButton {
    let name = label(stopping, reload.loading(), deep.running());
    let on_press = {
        let reload = reload.clone();
        let deep = deep.clone();
        move |_: ()| {
            if stopping {
                stop(&reload, &deep);
            } else {
                reload.run();
            }
        }
    };
    icon_button(Icon::Refresh, on_press)
        .size(Dim::Control)
        .tooltip(name)
        .enabled(enabled)
        .morph(Icon::MediaPlaybackStop, stopping)
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn the_name_follows_the_state() {
        assert_eq!(label(false, false, false), "Refresh (F5)");
        // A read that has not gone on long enough to show Stop is still Refresh.
        assert_eq!(label(false, true, false), "Refresh (F5)");
        assert_eq!(label(true, true, false), "Stop (Esc)");
        assert_eq!(label(true, false, true), "Stop searching (Esc)");
        // Both at once: the read is what Escape stops.
        assert_eq!(label(true, true, true), "Stop (Esc)");
        // The hold after both ended.
        assert_eq!(label(true, false, false), "Stop (Esc)");
    }
}
