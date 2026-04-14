use serde::{Deserialize, Serialize};

use bingux_common::PackageId;
use bxc_sandbox::SandboxLevel;

/// A single package entry within a generation, describing what it exports
/// and what sandbox level to apply when launching its binaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageEntry {
    /// The fully-qualified package identifier.
    pub package_id: PackageId,
    /// Sandbox level for launched binaries from this package.
    pub sandbox_level: SandboxLevel,
    /// The files this package exports into the generation profile.
    pub exports: ExportedItems,
}

/// The set of files a package exports into the generation's merged view.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportedItems {
    /// Relative paths to executables, e.g. `"bin/firefox"`.
    pub binaries: Vec<String>,
    /// Relative paths to shared libraries, e.g. `"lib/libxul.so"`.
    pub libraries: Vec<String>,
    /// Relative paths to data files, e.g. `"share/applications/firefox.desktop"`.
    pub data: Vec<String>,
}

/// A fully materialised generation — the result of building from a list
/// of package entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generation {
    /// Monotonically increasing generation number.
    pub id: u64,
    /// Unix timestamp (seconds since epoch) when this generation was built.
    pub timestamp: u64,
    /// The packages that make up this generation.
    pub packages: Vec<GenerationPackage>,
    /// SHA-256 hash of the configuration that produced this generation.
    pub config_hash: String,
}

/// Compact per-package record stored in `generation.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationPackage {
    /// Package identifier string (e.g. `"firefox-129.0-x86_64-linux"`).
    pub id: String,
    /// Sandbox level applied to this package's binaries.
    pub sandbox: SandboxLevel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_common::package_id::Arch;

    #[test]
    fn generation_metadata_roundtrip() {
        let generation = Generation {
            id: 42,
            timestamp: 1706000000,
            packages: vec![
                GenerationPackage {
                    id: "firefox-129.0-x86_64-linux".into(),
                    sandbox: SandboxLevel::Standard,
                },
                GenerationPackage {
                    id: "bash-5.2-x86_64-linux".into(),
                    sandbox: SandboxLevel::Minimal,
                },
            ],
            config_hash: "sha256:abc123".into(),
        };

        let toml_str = toml::to_string_pretty(&generation).unwrap();
        let parsed: Generation = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.id, 42);
        assert_eq!(parsed.timestamp, 1706000000);
        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(parsed.packages[0].id, "firefox-129.0-x86_64-linux");
        assert_eq!(parsed.packages[0].sandbox, SandboxLevel::Standard);
        assert_eq!(parsed.config_hash, "sha256:abc123");
    }

    #[test]
    fn package_entry_roundtrip() {
        let entry = PackageEntry {
            package_id: PackageId::new("firefox", "129.0", Arch::X86_64Linux).unwrap(),
            sandbox_level: SandboxLevel::Standard,
            exports: ExportedItems {
                binaries: vec!["bin/firefox".into()],
                libraries: vec!["lib/libxul.so".into()],
                data: vec!["share/applications/firefox.desktop".into()],
            },
        };

        let toml_str = toml::to_string_pretty(&entry).unwrap();
        let parsed: PackageEntry = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.package_id.name, "firefox");
        assert_eq!(parsed.sandbox_level, SandboxLevel::Standard);
        assert_eq!(parsed.exports.binaries, vec!["bin/firefox"]);
    }
}
