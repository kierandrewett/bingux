//! The snapshot scenarios of menus.

use crate::snapshot::{Rig, Scenario, ctx, scenario, wait};
use bingux_ui::snapshot::Snapshot;

fn menu(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    shot.click_name("Main menu");
    wait(shot, 500);
}

/// A right click on the link of the double's page.
fn context(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    let (x, y) = *ctx().chrome.page_origin.peek();
    shot.right_click(x + 100.0, y + 290.0);
    wait(shot, 600);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("menu", &["example.com"], menu),
        scenario("context", &["example.com"], context),
    ]
}
