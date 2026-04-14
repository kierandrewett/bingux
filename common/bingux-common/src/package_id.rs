use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{BinguxError, Result};

/// The supported target architectures.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Arch {
    #[serde(rename = "x86_64-linux")]
    X86_64Linux,
    #[serde(rename = "aarch64-linux")]
    Aarch64Linux,
}

impl Arch {
    pub fn as_str(&self) -> &'static str {
        match self {
            Arch::X86_64Linux => "x86_64-linux",
            Arch::Aarch64Linux => "aarch64-linux",
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Arch {
    type Err = BinguxError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "x86_64-linux" => Ok(Arch::X86_64Linux),
            "aarch64-linux" => Ok(Arch::Aarch64Linux),
            _ => Err(BinguxError::InvalidArch(s.to_string())),
        }
    }
}

/// A fully-qualified package identifier.
///
/// Format: `<name>-<version>-<arch>`
///
/// Examples:
/// - `firefox-128.0.1-x86_64-linux`
/// - `glibc-2.39-x86_64-linux`
/// - `my-tool-1.0.0-aarch64-linux`
///
/// The name may contain hyphens (e.g. `my-cool-tool`). Parsing splits from
/// the right: the last segment matching a known arch is the arch, the segment
/// before it is the version, and everything before that is the name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId {
    pub name: String,
    pub version: String,
    pub arch: Arch,
}

impl PackageId {
    /// Create a new PackageId, validating all components.
    pub fn new(name: impl Into<String>, version: impl Into<String>, arch: Arch) -> Result<Self> {
        let name = name.into();
        let version = version.into();

        if name.is_empty() {
            return Err(BinguxError::InvalidPackageId(
                "package name cannot be empty".into(),
            ));
        }

        if version.is_empty() {
            return Err(BinguxError::InvalidVersion(
                "version cannot be empty".into(),
            ));
        }

        // Name must be lowercase alphanumeric + hyphens, not starting/ending with hyphen
        if !is_valid_name(&name) {
            return Err(BinguxError::InvalidPackageId(format!(
                "invalid package name: '{name}' (must be lowercase alphanumeric and hyphens, \
                 not starting or ending with a hyphen)"
            )));
        }

        Ok(Self {
            name,
            version,
            arch,
        })
    }

    /// The directory name in the package store.
    /// e.g. `firefox-128.0.1-x86_64-linux`
    pub fn dir_name(&self) -> String {
        format!("{}-{}-{}", self.name, self.version, self.arch)
    }

    /// The `.bgx` archive filename.
    /// e.g. `firefox-128.0.1-x86_64-linux.bgx`
    pub fn bgx_filename(&self) -> String {
        format!("{}.bgx", self.dir_name())
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}-{}", self.name, self.version, self.arch)
    }
}

impl FromStr for PackageId {
    type Err = BinguxError;

    /// Parse a package ID string.
    ///
    /// Strategy: try each known arch suffix from the right. The arch suffix is
    /// always the last component(s) separated by hyphens. We try the longest
    /// arch strings first.
    fn from_str(s: &str) -> Result<Self> {
        let known_arches = ["x86_64-linux", "aarch64-linux"];

        for arch_str in &known_arches {
            if let Some(prefix) = s.strip_suffix(arch_str) {
                // The prefix should end with a hyphen separator
                let prefix = prefix
                    .strip_suffix('-')
                    .ok_or_else(|| BinguxError::InvalidPackageId(s.to_string()))?;

                // Now split prefix into name and version.
                // Version is the last hyphen-separated segment of the prefix.
                // But version can contain dots (128.0.1), not hyphens.
                // So we split on the last hyphen in prefix.
                let last_hyphen = prefix
                    .rfind('-')
                    .ok_or_else(|| BinguxError::InvalidPackageId(s.to_string()))?;

                let name = &prefix[..last_hyphen];
                let version = &prefix[last_hyphen + 1..];
                let arch: Arch = arch_str.parse()?;

                return PackageId::new(name, version, arch);
            }
        }

        Err(BinguxError::InvalidPackageId(format!(
            "no known architecture suffix in '{s}'"
        )))
    }
}

/// Validate a package name: lowercase ASCII alphanumeric + hyphens,
/// not starting or ending with a hyphen, no consecutive hyphens.
fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.ends_with('-') {
        return false;
    }
    if name.contains("--") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let id: PackageId = "firefox-128.0.1-x86_64-linux".parse().unwrap();
        assert_eq!(id.name, "firefox");
        assert_eq!(id.version, "128.0.1");
        assert_eq!(id.arch, Arch::X86_64Linux);
    }

    #[test]
    fn parse_hyphenated_name() {
        let id: PackageId = "my-cool-tool-1.0.0-x86_64-linux".parse().unwrap();
        assert_eq!(id.name, "my-cool-tool");
        assert_eq!(id.version, "1.0.0");
        assert_eq!(id.arch, Arch::X86_64Linux);
    }

    #[test]
    fn parse_aarch64() {
        let id: PackageId = "glibc-2.39-aarch64-linux".parse().unwrap();
        assert_eq!(id.name, "glibc");
        assert_eq!(id.version, "2.39");
        assert_eq!(id.arch, Arch::Aarch64Linux);
    }

    #[test]
    fn parse_single_char_version() {
        let id: PackageId = "bash-5-x86_64-linux".parse().unwrap();
        assert_eq!(id.name, "bash");
        assert_eq!(id.version, "5");
    }

    #[test]
    fn display_roundtrip() {
        let id = PackageId::new("firefox", "128.0.1", Arch::X86_64Linux).unwrap();
        let s = id.to_string();
        assert_eq!(s, "firefox-128.0.1-x86_64-linux");
        let parsed: PackageId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn dir_name_and_bgx() {
        let id = PackageId::new("ripgrep", "14.1", Arch::X86_64Linux).unwrap();
        assert_eq!(id.dir_name(), "ripgrep-14.1-x86_64-linux");
        assert_eq!(id.bgx_filename(), "ripgrep-14.1-x86_64-linux.bgx");
    }

    #[test]
    fn reject_empty_name() {
        assert!(PackageId::new("", "1.0", Arch::X86_64Linux).is_err());
    }

    #[test]
    fn reject_empty_version() {
        assert!(PackageId::new("foo", "", Arch::X86_64Linux).is_err());
    }

    #[test]
    fn reject_uppercase_name() {
        assert!(PackageId::new("Firefox", "1.0", Arch::X86_64Linux).is_err());
    }

    #[test]
    fn reject_leading_hyphen() {
        assert!(PackageId::new("-foo", "1.0", Arch::X86_64Linux).is_err());
    }

    #[test]
    fn reject_trailing_hyphen() {
        assert!(PackageId::new("foo-", "1.0", Arch::X86_64Linux).is_err());
    }

    #[test]
    fn reject_consecutive_hyphens() {
        assert!(PackageId::new("foo--bar", "1.0", Arch::X86_64Linux).is_err());
    }

    #[test]
    fn reject_no_arch_suffix() {
        assert!("firefox-128.0.1-windows".parse::<PackageId>().is_err());
    }

    #[test]
    fn reject_missing_version() {
        assert!("firefox-x86_64-linux".parse::<PackageId>().is_err());
    }
}
