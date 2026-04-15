//! D-Bus message filter — decides allow/deny/prompt for each message.

use crate::policy::{DbusPolicy, PolicyAction};

/// Result of filtering a D-Bus message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterAction {
    /// Allow the message through to the real bus.
    Allow,
    /// Deny the message — return an error to the caller.
    Deny { reason: String },
    /// Prompt the user — pause until they respond.
    Prompt { description: String },
}

/// Filters D-Bus messages based on a package policy.
pub struct DbusFilter {
    policy: DbusPolicy,
}

impl DbusFilter {
    pub fn new(policy: DbusPolicy) -> Self {
        Self { policy }
    }

    /// Filter a method call.
    pub fn filter_method_call(
        &self,
        interface: &str,
        path: &str,
        member: &str,
    ) -> FilterAction {
        match self.policy.check(interface, path, member) {
            PolicyAction::Allow => FilterAction::Allow,
            PolicyAction::Deny => FilterAction::Deny {
                reason: format!(
                    "{} denied access to {interface}.{member}",
                    self.policy.package
                ),
            },
            PolicyAction::Prompt => FilterAction::Prompt {
                description: format!(
                    "{} wants to call {interface}.{member}",
                    self.policy.package
                ),
            },
        }
    }

    /// Check if this package is allowed to own a bus name.
    pub fn filter_name_acquisition(&self, name: &str) -> FilterAction {
        if self.policy.own_names.iter().any(|n| n == name || n == "*") {
            FilterAction::Allow
        } else {
            FilterAction::Deny {
                reason: format!("{} cannot own bus name: {name}", self.policy.package),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::DbusPolicy;

    #[test]
    fn filter_allows_portals() {
        let mut policy = DbusPolicy::standard_session();
        policy.package = "firefox".into();
        let filter = DbusFilter::new(policy);

        assert_eq!(
            filter.filter_method_call(
                "org.freedesktop.portal.FileChooser",
                "/org/freedesktop/portal/desktop",
                "OpenFile"
            ),
            FilterAction::Allow
        );
    }

    #[test]
    fn filter_denies_systemd() {
        let mut policy = DbusPolicy::standard_session();
        policy.package = "firefox".into();
        let filter = DbusFilter::new(policy);

        match filter.filter_method_call(
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "StartUnit",
        ) {
            FilterAction::Deny { reason } => assert!(reason.contains("firefox")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn filter_prompts_unknown() {
        let mut policy = DbusPolicy::standard_session();
        policy.package = "my-app".into();
        let filter = DbusFilter::new(policy);

        match filter.filter_method_call("com.example.CustomAPI", "/", "DoThing") {
            FilterAction::Prompt { description } => assert!(description.contains("my-app")),
            other => panic!("expected Prompt, got {other:?}"),
        }
    }

    #[test]
    fn name_acquisition_denied_by_default() {
        let mut policy = DbusPolicy::standard_session();
        policy.package = "firefox".into();
        let filter = DbusFilter::new(policy);

        match filter.filter_name_acquisition("org.mozilla.Firefox") {
            FilterAction::Deny { .. } => {}
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn name_acquisition_allowed_when_declared() {
        let mut policy = DbusPolicy::standard_session();
        policy.package = "firefox".into();
        policy.own_names.push("org.mozilla.Firefox".into());
        let filter = DbusFilter::new(policy);

        assert_eq!(
            filter.filter_name_acquisition("org.mozilla.Firefox"),
            FilterAction::Allow
        );
    }
}
