//! The snapshot scenarios of downloads.

use crate::commands::Command;
use crate::snapshot::{Rig, Scenario, run_command, scenario, wait};
use bingux_ui::snapshot::Snapshot;

fn downloads(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    rig.double.advance_downloads(1_000_000);
    wait(shot, 300);
    run_command(shot, Command::Downloads);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![scenario("downloads", &["files.example.net"], downloads)]
}
