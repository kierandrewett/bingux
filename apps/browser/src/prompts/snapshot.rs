//! The snapshot scenarios of prompts.

use crate::snapshot::{Rig, Scenario, scenario, wait};
use bingux_ui::snapshot::Snapshot;

fn permission(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 600);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("permission", &["perm.example.net"], permission),
        scenario("dialog", &["dialog.example.net"], permission),
        scenario("auth", &["auth.example.net"], permission),
    ]
}
