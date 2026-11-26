//! The snapshot scenarios of new tab.

use crate::snapshot::{Rig, Scenario, scenario, wait_typing};
use bingux_ui::snapshot::Snapshot;

/// A window that opens on a blank tab, with the address field focused: its
/// caret stays solid while the picture is taken.
fn blank(shot: &mut Snapshot, _: &Rig) {
    wait_typing(shot, 600);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![scenario("newtab", &[], blank)]
}
