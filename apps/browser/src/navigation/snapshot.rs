//! The snapshot scenarios of navigation.

use crate::snapshot::{Rig, Scenario, ctx, scenario, wait};
use crate::tabs::Tab;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::web_view::TabEvent;
use std::time::Duration;

fn loading(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 300);
    rig.tell(
        0,
        TabEvent::Load {
            started: false,
            loading: true,
            progress: 0.45,
            error: None,
        },
    );
    wait(shot, 400);
}

fn hover(shot: &mut Snapshot, _: &Rig) {
    let active = |predicate: fn(&Tab) -> bool| ctx().browser.peek().active().is_some_and(predicate);
    // The page is loaded first: a load that starts after the pointer arrived
    // would take the hovered link away again.
    shot.settle_until(Duration::from_secs(10), |_| {
        active(|tab| !tab.load.loading && !tab.title.is_empty())
    });
    wait(shot, 500);
    let (x, y) = *ctx().chrome.page_origin.peek();
    // The pointer keeps moving until the page says it is over the link: the
    // page view may not have taken the first move, and the engine answers a
    // change of what is under the pointer, not a move.
    let mut step = 0.0;
    shot.settle_until(Duration::from_secs(5), |shot| {
        if active(|tab| tab.hover.is_some()) {
            return true;
        }
        step = 1.0 - step;
        shot.hover(x + 100.0 + step, y + 290.0);
        false
    });
    wait(shot, 200);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            manual: true,
            ..scenario("loading", &["example.com"], loading)
        },
        scenario("hover", &["example.com"], hover),
    ]
}
