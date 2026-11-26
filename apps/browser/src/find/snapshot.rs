//! The snapshot scenarios of find.

use crate::commands::Command;
use crate::snapshot::{Rig, Scenario, run_command, scenario, wait, wait_typing};
use crate::target::Target;
use bingux_ui::snapshot::Snapshot;

fn find(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 400);
    run_command(shot, Command::Find);
    shot.run_script("type exam", Target::SCHEME)
        .expect("typing");
    wait_typing(shot, 600);
}

fn find_missing(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 400);
    run_command(shot, Command::Find);
    shot.run_script("type ab", Target::SCHEME).expect("typing");
    wait_typing(shot, 600);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("find", &["example.com"], find),
        scenario("find-missing", &["example.com"], find_missing),
    ]
}
