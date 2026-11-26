//! The snapshot scenarios of site info.

use crate::commands::Command;
use crate::snapshot::{Rig, Scenario, run_command, scenario, wait};
use bingux_ui::snapshot::Snapshot;

fn info(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::PageInfo);
}

/// The settings page with the engine's own account of its security, in a window
/// tall enough to show all of it.
fn security(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::Settings);
    wait(shot, 600);
}

/// The same before the engine has produced a report (`NOT_READY`).
fn security_pending(shot: &mut Snapshot, rig: &Rig) {
    rig.double.set_report(None);
    security(shot, rig);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("info", &["example.com"], info),
        scenario("info-plain", &["plain.example.net"], info),
        Scenario {
            tall: 3.2,
            ..scenario("security", &["example.com"], security)
        },
        Scenario {
            tall: 3.2,
            ..scenario("security-pending", &["example.com"], security_pending)
        },
    ]
}
