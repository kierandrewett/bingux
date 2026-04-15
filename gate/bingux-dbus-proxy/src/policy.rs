//! D-Bus access policy per package.
//!
//! Defines what a sandboxed package is allowed to do on the D-Bus bus.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A complete D-Bus policy for a single package.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DbusPolicy {
    /// Package this policy applies to.
    pub package: String,
    /// Which bus this policy covers.
    pub bus: BusType,
    /// Rules governing access.
    pub rules: Vec<PolicyRule>,
    /// Whether the package can own bus names.
    pub own_names: Vec<String>,
    /// Whether to log filtered (denied) calls for debugging.
    pub log_denied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BusType {
    #[default]
    Session,
    System,
}

/// A single access rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// D-Bus interface pattern (e.g. "org.freedesktop.Notifications")
    pub interface: Option<String>,
    /// Object path pattern (e.g. "/org/freedesktop/portal/*")
    pub path: Option<String>,
    /// Member (method/signal) name pattern
    pub member: Option<String>,
    /// Action to take when this rule matches
    pub action: PolicyAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyAction {
    Allow,
    Deny,
    /// Prompt the user via bingux-gated
    Prompt,
}

impl DbusPolicy {
    pub fn new(package: &str, bus: BusType) -> Self {
        Self {
            package: package.to_string(),
            bus,
            rules: Vec::new(),
            own_names: Vec::new(),
            log_denied: false,
        }
    }

    /// Add a rule allowing access to a specific interface.
    pub fn allow_interface(&mut self, interface: &str) {
        self.rules.push(PolicyRule {
            interface: Some(interface.to_string()),
            path: None,
            member: None,
            action: PolicyAction::Allow,
        });
    }

    /// Add a rule denying access to a specific interface.
    pub fn deny_interface(&mut self, interface: &str) {
        self.rules.push(PolicyRule {
            interface: Some(interface.to_string()),
            path: None,
            member: None,
            action: PolicyAction::Deny,
        });
    }

    /// Add a rule allowing access to XDG portals.
    pub fn allow_portals(&mut self) {
        self.allow_interface("org.freedesktop.portal.*");
    }

    /// Add a rule allowing desktop notifications.
    pub fn allow_notifications(&mut self) {
        self.allow_interface("org.freedesktop.Notifications");
    }

    /// Build a default policy for a "standard" sandbox level.
    /// Allows: portals, notifications, StatusNotifierItem (tray).
    /// Denies: systemd, PackageKit, polkit, Accounts.
    pub fn standard_session() -> Self {
        let mut policy = Self::new("", BusType::Session);
        // Allow safe interfaces
        policy.allow_portals();
        policy.allow_notifications();
        policy.allow_interface("org.freedesktop.ScreenSaver");
        policy.allow_interface("org.kde.StatusNotifierItem");
        policy.allow_interface("org.kde.StatusNotifierWatcher");
        // Deny dangerous interfaces
        policy.deny_interface("org.freedesktop.systemd1");
        policy.deny_interface("org.freedesktop.PackageKit");
        policy.deny_interface("org.freedesktop.PolicyKit1");
        policy.deny_interface("org.freedesktop.Accounts");
        policy.deny_interface("org.freedesktop.login1");
        policy
    }

    /// Build a default policy for system bus access.
    /// Most apps should NOT have system bus access at all.
    pub fn standard_system() -> Self {
        let mut policy = Self::new("", BusType::System);
        // Deny everything by default — only pre-granted services get through
        policy.deny_interface("*");
        policy
    }

    /// Check if a message is allowed by this policy.
    pub fn check(&self, interface: &str, path: &str, member: &str) -> PolicyAction {
        // Rules are checked in order — first match wins
        for rule in &self.rules {
            if rule.matches(interface, path, member) {
                return rule.action;
            }
        }
        // Default: prompt (not auto-deny, let the user decide)
        PolicyAction::Prompt
    }
}

impl PolicyRule {
    fn matches(&self, interface: &str, path: &str, member: &str) -> bool {
        let iface_match = match &self.interface {
            None => true,
            Some(pattern) if pattern == "*" => true,
            Some(pattern) if pattern.ends_with(".*") => {
                let prefix = &pattern[..pattern.len() - 2];
                interface.starts_with(prefix)
            }
            Some(pattern) => interface == pattern,
        };

        let path_match = match &self.path {
            None => true,
            Some(pattern) if pattern == "*" => true,
            Some(pattern) if pattern.ends_with("/*") => {
                let prefix = &pattern[..pattern.len() - 2];
                path.starts_with(prefix)
            }
            Some(pattern) => path == pattern,
        };

        let member_match = match &self.member {
            None => true,
            Some(pattern) => member == pattern,
        };

        iface_match && path_match && member_match
    }
}

/// Load per-package D-Bus policies from a permissions directory.
pub fn load_policies(permissions_dir: &Path) -> HashMap<String, DbusPolicy> {
    let mut policies = HashMap::new();

    if let Ok(entries) = std::fs::read_dir(permissions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Try to extract dbus policy from the permission file
                    if let Some(pkg_name) = path.file_stem().and_then(|s| s.to_str()) {
                        let mut policy = DbusPolicy::standard_session();
                        policy.package = pkg_name.to_string();
                        policies.insert(pkg_name.to_string(), policy);
                    }
                }
            }
        }
    }

    policies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_session_allows_portals() {
        let policy = DbusPolicy::standard_session();
        assert_eq!(
            policy.check("org.freedesktop.portal.FileChooser", "/org/freedesktop/portal/desktop", "OpenFile"),
            PolicyAction::Allow,
        );
    }

    #[test]
    fn standard_session_allows_notifications() {
        let policy = DbusPolicy::standard_session();
        assert_eq!(
            policy.check("org.freedesktop.Notifications", "/org/freedesktop/Notifications", "Notify"),
            PolicyAction::Allow,
        );
    }

    #[test]
    fn standard_session_denies_systemd() {
        let policy = DbusPolicy::standard_session();
        assert_eq!(
            policy.check("org.freedesktop.systemd1", "/org/freedesktop/systemd1", "StartUnit"),
            PolicyAction::Deny,
        );
    }

    #[test]
    fn standard_session_denies_polkit() {
        let policy = DbusPolicy::standard_session();
        assert_eq!(
            policy.check("org.freedesktop.PolicyKit1", "/org/freedesktop/PolicyKit1/Authority", "CheckAuthorization"),
            PolicyAction::Deny,
        );
    }

    #[test]
    fn standard_session_prompts_unknown() {
        let policy = DbusPolicy::standard_session();
        assert_eq!(
            policy.check("com.example.SomeApp", "/com/example/SomeApp", "DoSomething"),
            PolicyAction::Prompt,
        );
    }

    #[test]
    fn standard_system_denies_everything() {
        let policy = DbusPolicy::standard_system();
        assert_eq!(
            policy.check("org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "Enable"),
            PolicyAction::Deny,
        );
    }

    #[test]
    fn custom_rules_override_defaults() {
        let mut policy = DbusPolicy::standard_session();
        // Add a custom allow for NetworkManager (before the deny-all)
        policy.rules.insert(0, PolicyRule {
            interface: Some("org.freedesktop.NetworkManager".into()),
            path: None,
            member: None,
            action: PolicyAction::Allow,
        });
        assert_eq!(
            policy.check("org.freedesktop.NetworkManager", "/", "GetDevices"),
            PolicyAction::Allow,
        );
    }

    #[test]
    fn wildcard_interface_matching() {
        let rule = PolicyRule {
            interface: Some("org.freedesktop.portal.*".into()),
            path: None,
            member: None,
            action: PolicyAction::Allow,
        };
        assert!(rule.matches("org.freedesktop.portal.FileChooser", "/", "Open"));
        assert!(rule.matches("org.freedesktop.portal.OpenURI", "/", "Open"));
        assert!(!rule.matches("org.freedesktop.Notifications", "/", "Notify"));
    }

    #[test]
    fn path_wildcard_matching() {
        let rule = PolicyRule {
            interface: None,
            path: Some("/org/freedesktop/portal/*".into()),
            member: None,
            action: PolicyAction::Allow,
        };
        assert!(rule.matches("any", "/org/freedesktop/portal/desktop", "any"));
        assert!(!rule.matches("any", "/org/freedesktop/systemd1", "any"));
    }
}
