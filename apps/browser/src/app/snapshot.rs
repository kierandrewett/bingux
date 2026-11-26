//! The snapshot scenarios of app.

use crate::snapshot::{Rig, Scenario, nothing, scenario, wait};
use bingux_ui::snapshot::Snapshot;
use bingux_ui::web_view::TabEvent;

fn fullscreen(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    rig.tell(0, TabEvent::Fullscreen(true));
    wait(shot, 500);
}

fn rtl(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    rig.tell(0, TabEvent::Title("\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629} \u{2013} \u{645}\u{648}\u{642}\u{639} \u{627}\u{62e}\u{628}\u{627}\u{631}".to_string()));
    wait(shot, 500);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("page", &["example.com"], nothing),
        scenario("fullscreen", &["video.example.net"], fullscreen),
        scenario("rtl", &["news.bingux.dev"], rtl),
    ]
}
