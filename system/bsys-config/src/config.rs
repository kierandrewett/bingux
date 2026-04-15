use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use bingux_common::error::Result;

/// Top-level system configuration parsed from `system.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Core system settings (hostname, locale, timezone, keymap).
    pub system: SystemSection,
    /// Which packages to keep or remove.
    pub packages: PackagesSection,
    /// Which services to enable.
    pub services: ServicesSection,
    /// Per-service permission overrides.  Key is the service name.
    #[serde(default)]
    pub permissions: HashMap<String, ServicePermissions>,
    /// Optional network configuration.
    pub network: Option<NetworkSection>,
    /// Optional firewall configuration.
    pub firewall: Option<FirewallSection>,
    /// User accounts to create.
    #[serde(default)]
    pub users: Vec<UserConfig>,
}

/// A user account declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// Login name.
    pub name: String,
    /// Numeric user ID.
    pub uid: u32,
    /// Numeric primary group ID.
    pub gid: u32,
    /// Home directory path.
    pub home: String,
    /// Login shell path.
    pub shell: String,
    /// Additional group memberships.
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Core system identity and locale settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSection {
    /// Machine hostname.
    pub hostname: String,
    /// Locale string, e.g. `"en_GB.UTF-8"`.
    pub locale: String,
    /// IANA timezone, e.g. `"Europe/London"`.
    pub timezone: String,
    /// Console keymap, e.g. `"uk"`.
    pub keymap: String,
}

/// Package set specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagesSection {
    /// Packages that should be installed.
    pub keep: Vec<String>,
    /// Packages that should be explicitly removed.
    pub rm: Option<Vec<String>>,
}

/// Service management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesSection {
    /// Services to enable.
    pub enable: Vec<String>,
}

/// Per-service permission overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePermissions {
    /// Allowed capability/permission names.
    pub allow: Option<Vec<String>>,
    /// Denied capability/permission names.
    pub deny: Option<Vec<String>>,
    /// Additional bind-mount paths.
    pub mounts: Option<Vec<String>>,
}

/// Network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSection {
    /// DNS resolver addresses.
    pub dns: Option<Vec<String>>,
}

/// Firewall configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallSection {
    /// TCP/UDP ports to allow inbound.
    pub allow_ports: Option<Vec<u16>>,
}

/// Parse a `SystemConfig` from a file path.
pub fn parse_system_config(path: &Path) -> Result<SystemConfig> {
    let content = std::fs::read_to_string(path)?;
    parse_system_config_str(&content)
}

/// Parse a `SystemConfig` from a TOML string.
pub fn parse_system_config_str(content: &str) -> Result<SystemConfig> {
    Ok(toml::from_str(content)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_CONFIG: &str = r#"
[system]
hostname = "bingux-pc"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = ["firefox", "bash", "coreutils"]
rm = ["nano"]

[services]
enable = ["sshd", "NetworkManager"]

[permissions.sshd]
allow = ["net.bind"]
deny = ["fs.write"]
mounts = ["/etc/ssh"]

[network]
dns = ["1.1.1.1", "1.0.0.1"]

[firewall]
allow_ports = [22, 80, 443]
"#;

    const MINIMAL_CONFIG: &str = r#"
[system]
hostname = "minimal"
locale = "en_US.UTF-8"
timezone = "UTC"
keymap = "us"

[packages]
keep = ["bash"]

[services]
enable = []
"#;

    #[test]
    fn parse_full_config() {
        let config = parse_system_config_str(FULL_CONFIG).unwrap();

        assert_eq!(config.system.hostname, "bingux-pc");
        assert_eq!(config.system.locale, "en_GB.UTF-8");
        assert_eq!(config.system.timezone, "Europe/London");
        assert_eq!(config.system.keymap, "uk");

        assert_eq!(config.packages.keep, vec!["firefox", "bash", "coreutils"]);
        assert_eq!(config.packages.rm, Some(vec!["nano".to_string()]));

        assert_eq!(config.services.enable, vec!["sshd", "NetworkManager"]);

        let sshd = config.permissions.get("sshd").unwrap();
        assert_eq!(sshd.allow, Some(vec!["net.bind".to_string()]));
        assert_eq!(sshd.deny, Some(vec!["fs.write".to_string()]));
        assert_eq!(sshd.mounts, Some(vec!["/etc/ssh".to_string()]));

        let net = config.network.unwrap();
        assert_eq!(
            net.dns,
            Some(vec!["1.1.1.1".to_string(), "1.0.0.1".to_string()])
        );

        let fw = config.firewall.unwrap();
        assert_eq!(fw.allow_ports, Some(vec![22, 80, 443]));
    }

    #[test]
    fn parse_minimal_config() {
        let config = parse_system_config_str(MINIMAL_CONFIG).unwrap();

        assert_eq!(config.system.hostname, "minimal");
        assert_eq!(config.packages.keep, vec!["bash"]);
        assert_eq!(config.packages.rm, None);
        assert!(config.permissions.is_empty());
        assert!(config.network.is_none());
        assert!(config.firewall.is_none());
    }

    #[test]
    fn missing_hostname_is_error() {
        let bad = r#"
[system]
locale = "en_US.UTF-8"
timezone = "UTC"
keymap = "us"

[packages]
keep = []

[services]
enable = []
"#;
        assert!(parse_system_config_str(bad).is_err());
    }

    #[test]
    fn roundtrip_serialization() {
        let config = parse_system_config_str(FULL_CONFIG).unwrap();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let reparsed = parse_system_config_str(&serialized).unwrap();
        assert_eq!(reparsed.system.hostname, config.system.hostname);
        assert_eq!(reparsed.packages.keep, config.packages.keep);
    }
}
