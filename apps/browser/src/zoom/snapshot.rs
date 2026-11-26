//! The snapshot scenarios of zoom.

use crate::commands::Command;
use crate::snapshot::{Rig, Scenario, run_command, scenario, wait};
use bingux_ui::snapshot::Snapshot;

fn zoomed(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::ZoomIn);
    run_command(shot, Command::ZoomIn);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![scenario("zoom", &["example.com"], zoomed)]
}
