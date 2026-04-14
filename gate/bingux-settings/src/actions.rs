//! Actions that the bingux-settings UI can trigger.
//!
//! Each action modifies the on-disk permission files in the user's
//! permission directory.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tracing::{debug, info};

use crate::model::{PermissionFile, PermissionMeta, SettingsModel};

// ── Error type ────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Parse(String),

    #[error("TOML serialization error: {0}")]
    Serialize(String),

    #[error("package not found: {0}")]
    PackageNotFound(String),
}

// ── Action enum ───────────────────────────────────────────────────

/// An action the settings UI can dispatch.
#[derive(Debug, Clone)]
pub enum SettingsAction {
    /// Revoke a single capability from a package.
    RevokeCapability {
        package: String,
        capability: String,
    },
    /// Revoke a single mount from a package.
    RevokeMount {
        package: String,
        path: String,
    },
    /// Revoke all permissions for a package (delete the file).
    RevokeAllForPackage {
        package: String,
    },
    /// Delete all permission files (reset everything).
    ResetAllPermissions,
    /// Export all permissions to a TOML file.
    ExportPermissions {
        output: PathBuf,
    },
    /// Import permissions from a TOML file, merging into the directory.
    ImportPermissions {
        input: PathBuf,
    },
}

// ── Execution ─────────────────────────────────────────────────────

/// Execute a settings action against the permission directory.
///
/// Returns a human-readable status message on success.
pub fn execute_action(
    action: &SettingsAction,
    permissions_dir: &Path,
) -> Result<String, SettingsError> {
    match action {
        SettingsAction::RevokeCapability {
            package,
            capability,
        } => revoke_capability(permissions_dir, package, capability),

        SettingsAction::RevokeMount { package, path } => {
            revoke_mount(permissions_dir, package, path)
        }

        SettingsAction::RevokeAllForPackage { package } => {
            revoke_all_for_package(permissions_dir, package)
        }

        SettingsAction::ResetAllPermissions => reset_all(permissions_dir),

        SettingsAction::ExportPermissions { output } => export(permissions_dir, output),

        SettingsAction::ImportPermissions { input } => import(permissions_dir, input),
    }
}

// ── Helpers ───────────────────────────────────────────────────────

fn permission_path(dir: &Path, package: &str) -> PathBuf {
    dir.join(format!("{package}.toml"))
}

fn load_file(path: &Path) -> Result<PermissionFile, SettingsError> {
    let content = fs::read_to_string(path)?;
    toml::from_str(&content).map_err(|e| SettingsError::Parse(e.to_string()))
}

fn save_file(path: &Path, pf: &PermissionFile) -> Result<(), SettingsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(pf).map_err(|e| SettingsError::Serialize(e.to_string()))?;
    fs::write(path, content)?;
    Ok(())
}

fn revoke_capability(
    dir: &Path,
    package: &str,
    capability: &str,
) -> Result<String, SettingsError> {
    let path = permission_path(dir, package);
    if !path.exists() {
        return Err(SettingsError::PackageNotFound(package.to_string()));
    }

    let mut pf = load_file(&path)?;
    pf.capabilities.remove(capability);
    save_file(&path, &pf)?;

    info!(package, capability, "revoked capability");
    Ok(format!("Revoked {capability} from {package}"))
}

fn revoke_mount(dir: &Path, package: &str, mount_path: &str) -> Result<String, SettingsError> {
    let path = permission_path(dir, package);
    if !path.exists() {
        return Err(SettingsError::PackageNotFound(package.to_string()));
    }

    let mut pf = load_file(&path)?;
    pf.mounts.remove(mount_path);
    save_file(&path, &pf)?;

    info!(package, mount_path, "revoked mount");
    Ok(format!("Revoked mount {mount_path} from {package}"))
}

fn revoke_all_for_package(dir: &Path, package: &str) -> Result<String, SettingsError> {
    let path = permission_path(dir, package);
    if path.exists() {
        fs::remove_file(&path)?;
        info!(package, "revoked all permissions (file deleted)");
    } else {
        debug!(package, "no permission file to delete");
    }
    Ok(format!("Revoked all permissions for {package}"))
}

fn reset_all(dir: &Path) -> Result<String, SettingsError> {
    if !dir.exists() {
        return Ok("No permissions directory to reset".to_string());
    }

    let mut count = 0u32;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            fs::remove_file(&path)?;
            count += 1;
        }
    }

    info!(count, "reset all permissions");
    Ok(format!("Deleted {count} permission file(s)"))
}

fn export(dir: &Path, output: &Path) -> Result<String, SettingsError> {
    let model = SettingsModel::load_from_dir(dir)?;
    let content = model.export_toml()?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, &content)?;
    info!(path = %output.display(), "exported permissions");
    Ok(format!("Exported permissions to {}", output.display()))
}

fn import(dir: &Path, input: &Path) -> Result<String, SettingsError> {
    let content = fs::read_to_string(input)?;
    let files: HashMap<String, PermissionFile> =
        toml::from_str(&content).map_err(|e| SettingsError::Parse(e.to_string()))?;

    fs::create_dir_all(dir)?;

    let mut count = 0u32;
    for (name, pf) in &files {
        let mut pf = pf.clone();
        if pf.meta.package.is_empty() {
            pf.meta = PermissionMeta {
                package: name.clone(),
                first_prompted: String::new(),
            };
        }
        let path = permission_path(dir, name);
        save_file(&path, &pf)?;
        count += 1;
    }

    info!(count, path = %input.display(), "imported permissions");
    Ok(format!("Imported {count} package permission(s) from {}", input.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_permission(dir: &Path, package: &str) {
        let pf = PermissionFile {
            meta: PermissionMeta {
                package: package.to_string(),
                first_prompted: "2025-01-01T00:00:00Z".to_string(),
            },
            capabilities: {
                let mut m = HashMap::new();
                m.insert("gpu".to_string(), "allow".to_string());
                m.insert("audio".to_string(), "allow".to_string());
                m.insert("camera".to_string(), "deny".to_string());
                m
            },
            mounts: {
                let mut m = HashMap::new();
                m.insert("~/Downloads".to_string(), "list,w".to_string());
                m
            },
            files: HashMap::new(),
        };
        save_file(&permission_path(dir, package), &pf).unwrap();
    }

    #[test]
    fn revoke_capability_removes_key() {
        let dir = tempfile::tempdir().unwrap();
        write_test_permission(dir.path(), "firefox");

        let action = SettingsAction::RevokeCapability {
            package: "firefox".to_string(),
            capability: "gpu".to_string(),
        };
        let msg = execute_action(&action, dir.path()).unwrap();
        assert!(msg.contains("gpu"));

        // Reload and verify
        let pf = load_file(&permission_path(dir.path(), "firefox")).unwrap();
        assert!(!pf.capabilities.contains_key("gpu"));
        assert!(pf.capabilities.contains_key("audio")); // others untouched
    }

    #[test]
    fn revoke_mount_removes_entry() {
        let dir = tempfile::tempdir().unwrap();
        write_test_permission(dir.path(), "firefox");

        let action = SettingsAction::RevokeMount {
            package: "firefox".to_string(),
            path: "~/Downloads".to_string(),
        };
        let msg = execute_action(&action, dir.path()).unwrap();
        assert!(msg.contains("~/Downloads"));

        let pf = load_file(&permission_path(dir.path(), "firefox")).unwrap();
        assert!(pf.mounts.is_empty());
    }

    #[test]
    fn revoke_all_for_package_deletes_file() {
        let dir = tempfile::tempdir().unwrap();
        write_test_permission(dir.path(), "firefox");
        assert!(permission_path(dir.path(), "firefox").exists());

        let action = SettingsAction::RevokeAllForPackage {
            package: "firefox".to_string(),
        };
        execute_action(&action, dir.path()).unwrap();
        assert!(!permission_path(dir.path(), "firefox").exists());
    }

    #[test]
    fn reset_all_permissions_clears_directory() {
        let dir = tempfile::tempdir().unwrap();
        write_test_permission(dir.path(), "firefox");
        write_test_permission(dir.path(), "vlc");

        let action = SettingsAction::ResetAllPermissions;
        let msg = execute_action(&action, dir.path()).unwrap();
        assert!(msg.contains("2"));

        let remaining: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(remaining.is_empty());
    }

    #[test]
    fn export_and_import_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        write_test_permission(dir.path(), "firefox");
        write_test_permission(dir.path(), "vlc");

        let export_path = dir.path().join("export.toml");

        // Export
        let action = SettingsAction::ExportPermissions {
            output: export_path.clone(),
        };
        execute_action(&action, dir.path()).unwrap();
        assert!(export_path.exists());

        // Import into a fresh directory
        let import_dir = tempfile::tempdir().unwrap();
        let action = SettingsAction::ImportPermissions {
            input: export_path,
        };
        let msg = execute_action(&action, import_dir.path()).unwrap();
        assert!(msg.contains("2"));

        // Verify
        assert!(permission_path(import_dir.path(), "firefox").exists());
        assert!(permission_path(import_dir.path(), "vlc").exists());
    }

    #[test]
    fn revoke_capability_for_missing_package_errors() {
        let dir = tempfile::tempdir().unwrap();
        let action = SettingsAction::RevokeCapability {
            package: "nonexistent".to_string(),
            capability: "gpu".to_string(),
        };
        let result = execute_action(&action, dir.path());
        assert!(result.is_err());
    }
}
