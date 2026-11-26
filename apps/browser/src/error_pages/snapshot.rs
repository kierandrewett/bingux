//! The snapshot scenarios of error pages.

use crate::snapshot::{Rig, Scenario, nothing, scenario, wait};
use bingux_ui::snapshot::Snapshot;

fn cert_details(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    shot.click_text("Details\u{2026}");
    wait(shot, 700);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("cert", &["expired.example.net"], nothing),
        scenario("cert-details", &["expired.example.net"], cert_details),
        scenario("cert-pinned", &["pinned.example.net"], nothing),
        scenario("error", &["missing.example.net"], nothing),
        scenario("crash", &["crash.example.net"], nothing),
    ]
}
