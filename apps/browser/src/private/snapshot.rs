//! The snapshot scenarios of private.

use crate::snapshot::{Scenario, blank, scenario};

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![Scenario {
        private: true,
        ..scenario("private", &[], blank)
    }]
}
