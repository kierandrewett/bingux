//! Data model for the bingux-settings UI.
//!
//! Loads per-package permission TOML files from the user's permission
//! directory and presents them as a flat, UI-friendly structure.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use walkdir::WalkDir;

use crate::actions::SettingsError;

// ── Public types ──────────────────────────────────────────────────

/// Top-level model: a summary of every package's permissions.
#[derive(Debug, Clone, Default)]
pub struct SettingsModel {
    pub packages: Vec<PackagePermissionSummary>,
}

/// Summary of a single package's granted/denied permissions.
#[derive(Debug, Clone)]
pub struct PackagePermissionSummary {
    pub name: String,
    pub capabilities: Vec<CapabilitySummary>,
    pub mounts: Vec<MountSummary>,
    pub file_grants: Vec<FileGrant>,
}

/// One capability entry.
#[derive(Debug, Clone)]
pub struct CapabilitySummary {
    pub name: String,
    pub status: PermissionStatus,
}

/// Three-state permission status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionStatus {
    Allowed,
    Denied,
    NotSet,
}

/// A mount path and its permission string.
#[derive(Debug, Clone)]
pub struct MountSummary {
    pub path: String,
    pub grants: String,
}

/// A single file-level permission.
#[derive(Debug, Clone)]
pub struct FileGrant {
    pub path: String,
    pub permission: String,
}

// ── On-disk TOML format ───────────────────────────────────────────

/// Mirrors the permission file format used by bingux-gated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionFile {
    #[serde(default)]
    pub meta: PermissionMeta,
    #[serde(default)]
    pub capabilities: HashMap<String, String>,
    #[serde(default)]
    pub mounts: HashMap<String, String>,
    #[serde(default)]
    pub files: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionMeta {
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub first_prompted: String,
}

// ── Implementation ────────────────────────────────────────────────

impl SettingsModel {
    /// Load permissions from all `*.toml` files in `permissions_dir`.
    pub fn load_from_dir(permissions_dir: &Path) -> Result<Self, SettingsError> {
        let mut packages = Vec::new();

        if !permissions_dir.exists() {
            debug!(path = %permissions_dir.display(), "permissions directory does not exist");
            return Ok(Self { packages });
        }

        for entry in WalkDir::new(permissions_dir)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }

            match load_permission_file(path) {
                Ok(summary) => packages.push(summary),
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "skipping malformed permission file");
                }
            }
        }

        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { packages })
    }

    /// Serialise all permissions to a single TOML document suitable for
    /// export / backup.
    pub fn export_toml(&self) -> Result<String, SettingsError> {
        let mut files: HashMap<String, PermissionFile> = HashMap::new();

        for pkg in &self.packages {
            let mut caps = HashMap::new();
            for cap in &pkg.capabilities {
                let val = match cap.status {
                    PermissionStatus::Allowed => "allow",
                    PermissionStatus::Denied => "deny",
                    PermissionStatus::NotSet => continue,
                };
                caps.insert(cap.name.clone(), val.to_string());
            }

            let mut mounts = HashMap::new();
            for m in &pkg.mounts {
                mounts.insert(m.path.clone(), m.grants.clone());
            }

            let mut file_grants = HashMap::new();
            for f in &pkg.file_grants {
                file_grants.insert(f.path.clone(), f.permission.clone());
            }

            files.insert(
                pkg.name.clone(),
                PermissionFile {
                    meta: PermissionMeta {
                        package: pkg.name.clone(),
                        first_prompted: String::new(),
                    },
                    capabilities: caps,
                    mounts,
                    files: file_grants,
                },
            );
        }

        toml::to_string_pretty(&files).map_err(|e| SettingsError::Serialize(e.to_string()))
    }

    /// Import permissions from a TOML document previously produced by
    /// [`export_toml`].
    pub fn import_toml(content: &str) -> Result<Self, SettingsError> {
        let files: HashMap<String, PermissionFile> =
            toml::from_str(content).map_err(|e| SettingsError::Parse(e.to_string()))?;

        let mut packages: Vec<PackagePermissionSummary> = files
            .into_iter()
            .map(|(name, pf)| permission_file_to_summary(&name, &pf))
            .collect();
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { packages })
    }
}

// ── Helpers ───────────────────────────────────────────────────────

fn load_permission_file(path: &Path) -> Result<PackagePermissionSummary, SettingsError> {
    let content = fs::read_to_string(path)?;
    let pf: PermissionFile =
        toml::from_str(&content).map_err(|e| SettingsError::Parse(e.to_string()))?;

    let name = if pf.meta.package.is_empty() {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        pf.meta.package.clone()
    };

    Ok(permission_file_to_summary(&name, &pf))
}

fn permission_file_to_summary(name: &str, pf: &PermissionFile) -> PackagePermissionSummary {
    let capabilities = pf
        .capabilities
        .iter()
        .map(|(k, v)| CapabilitySummary {
            name: k.clone(),
            status: match v.as_str() {
                "allow" => PermissionStatus::Allowed,
                "deny" => PermissionStatus::Denied,
                _ => PermissionStatus::NotSet,
            },
        })
        .collect();

    let mounts = pf
        .mounts
        .iter()
        .map(|(k, v)| MountSummary {
            path: k.clone(),
            grants: v.clone(),
        })
        .collect();

    let file_grants = pf
        .files
        .iter()
        .map(|(k, v)| FileGrant {
            path: k.clone(),
            permission: v.clone(),
        })
        .collect();

    PackagePermissionSummary {
        name: name.to_string(),
        capabilities,
        mounts,
        file_grants,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_firefox_toml(dir: &Path) {
        let content = r#"
[meta]
package = "firefox"
first_prompted = "2025-01-15T14:30:00Z"

[capabilities]
gpu = "allow"
audio = "allow"
camera = "deny"

[mounts]
"~/Downloads" = "list,w"
"~/.mozilla" = "rw"

[files]
"~/.ssh/id_rsa.pub" = "r"
"~/.ssh/id_rsa" = "deny(r)"
"#;
        fs::write(dir.join("firefox.toml"), content).unwrap();
    }

    fn write_vlc_toml(dir: &Path) {
        let content = r#"
[meta]
package = "vlc"
first_prompted = "2025-02-01T10:00:00Z"

[capabilities]
gpu = "allow"
audio = "allow"

[mounts]
"~/Videos" = "ro"
"#;
        fs::write(dir.join("vlc.toml"), content).unwrap();
    }

    #[test]
    fn load_from_dir_reads_all_toml_files() {
        let dir = tempfile::tempdir().unwrap();
        write_firefox_toml(dir.path());
        write_vlc_toml(dir.path());

        let model = SettingsModel::load_from_dir(dir.path()).unwrap();
        assert_eq!(model.packages.len(), 2);
        // Sorted by name
        assert_eq!(model.packages[0].name, "firefox");
        assert_eq!(model.packages[1].name, "vlc");
    }

    #[test]
    fn load_from_dir_parses_capabilities() {
        let dir = tempfile::tempdir().unwrap();
        write_firefox_toml(dir.path());

        let model = SettingsModel::load_from_dir(dir.path()).unwrap();
        let firefox = &model.packages[0];

        let gpu = firefox.capabilities.iter().find(|c| c.name == "gpu").unwrap();
        assert_eq!(gpu.status, PermissionStatus::Allowed);

        let camera = firefox.capabilities.iter().find(|c| c.name == "camera").unwrap();
        assert_eq!(camera.status, PermissionStatus::Denied);
    }

    #[test]
    fn load_from_dir_parses_mounts_and_files() {
        let dir = tempfile::tempdir().unwrap();
        write_firefox_toml(dir.path());

        let model = SettingsModel::load_from_dir(dir.path()).unwrap();
        let firefox = &model.packages[0];

        assert_eq!(firefox.mounts.len(), 2);
        assert_eq!(firefox.file_grants.len(), 2);
    }

    #[test]
    fn load_from_nonexistent_dir_returns_empty() {
        let model = SettingsModel::load_from_dir(Path::new("/tmp/nonexistent-bingux-test-dir")).unwrap();
        assert!(model.packages.is_empty());
    }

    #[test]
    fn export_import_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        write_firefox_toml(dir.path());
        write_vlc_toml(dir.path());

        let original = SettingsModel::load_from_dir(dir.path()).unwrap();
        let exported = original.export_toml().unwrap();
        let imported = SettingsModel::import_toml(&exported).unwrap();

        assert_eq!(original.packages.len(), imported.packages.len());
        for (orig, imp) in original.packages.iter().zip(imported.packages.iter()) {
            assert_eq!(orig.name, imp.name);
            // Capability counts should match (NotSet ones are skipped in export)
            let orig_set: Vec<_> = orig
                .capabilities
                .iter()
                .filter(|c| c.status != PermissionStatus::NotSet)
                .collect();
            let imp_set: Vec<_> = imp
                .capabilities
                .iter()
                .filter(|c| c.status != PermissionStatus::NotSet)
                .collect();
            assert_eq!(orig_set.len(), imp_set.len());
        }
    }
}
