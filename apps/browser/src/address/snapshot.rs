//! The snapshot scenarios of address.

use crate::commands::Command;
use crate::snapshot::{Rig, Scenario, run_command, scenario, wait};
use crate::target::Target;
use bingux_ui::snapshot::Snapshot;

fn suggestions(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::FocusAddress);
    shot.run_script("type ex", Target::SCHEME).expect("typing");
    wait(shot, 700);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![scenario(
        "suggestions",
        &["example.com", "news.bingux.dev", "docs.example.org"],
        suggestions,
    )]
}
