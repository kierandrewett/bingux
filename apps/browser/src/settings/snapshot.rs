//! The snapshot scenarios of settings.

use crate::commands::Command;
use crate::snapshot::{Rig, Scenario, run_command, scenario, wait};
use bingux_ui::snapshot::Snapshot;

fn settings(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::Settings);
    wait(shot, 500);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![scenario("settings", &["example.com"], settings)]
}
