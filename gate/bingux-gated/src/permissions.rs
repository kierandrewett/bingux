//! Per-user per-package permission database backed by TOML files.
//!
//! Each sandboxed package has a file at
//! `~/.config/bingux/permissions/<pkg>.toml` that records the user's
//! permission grants for capabilities, mount paths, and individual files.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::error::{GatedError, Result};

// ── Permission grant enum ─────────────────────────────────────────

/// The three-state permission model.
///
/// `Prompt` is never persisted — it is the implied default when a key is
/// absent from the TOML file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionGrant {
    Allow,
    Deny,
    /// Not persisted; inferred from absence.
    Prompt,
}

/// Serde helper: serialise Allow/Deny as `"allow"` / `"deny"`, and skip
/// `Prompt` entirely (it is represented by the key being absent).
impl Serialize for PermissionGrant {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            PermissionGrant::Allow => serializer.serialize_str("allow"),
            PermissionGrant::Deny => serializer.serialize_str("deny"),
            PermissionGrant::Prompt => serializer.serialize_str("prompt"),
        }
    }
}

impl<'de> Deserialize<'de> for PermissionGrant {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "allow" => Ok(PermissionGrant::Allow),
            "deny" => Ok(PermissionGrant::Deny),
            "prompt" => Ok(PermissionGrant::Prompt),
            other => Err(serde::de::Error::unknown_variant(other, &["allow", "deny", "prompt"])),
        }
    }
}

// ── TOML-serialisable permission file ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionMeta {
    pub package: String,
    pub first_prompted: String,
}

/// The full permission state for one package, as stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagePermissions {
    pub meta: PermissionMeta,
    /// Capability name → Allow / Deny.
    #[serde(default)]
    pub capabilities: HashMap<String, PermissionGrant>,
    /// Mount path → comma-separated permission string (e.g. `"rw"`, `"list,w"`).
    #[serde(default)]
    pub mounts: HashMap<String, String>,
    /// File path → permission string (e.g. `"r"`, `"rw"`, `"deny(r)"`).
    #[serde(default)]
    pub files: HashMap<String, String>,
}

impl PackagePermissions {
    /// Create a fresh, empty permission set for a package.
    pub fn new_empty(package: &str) -> Self {
        Self {
            meta: PermissionMeta {
                package: package.to_string(),
                first_prompted: chrono_now(),
            },
            capabilities: HashMap::new(),
            mounts: HashMap::new(),
            files: HashMap::new(),
        }
    }
}

// ── Permission database ───────────────────────────────────────────

/// Manages the on-disk TOML permission files for a single user.
pub struct PermissionDb {
    user: String,
    base_path: PathBuf,
    /// In-memory cache of loaded permission files.
    cache: HashMap<String, PackagePermissions>,
}

impl PermissionDb {
    pub fn new(user: &str, base_path: PathBuf) -> Self {
        Self {
            user: user.to_string(),
            base_path,
            cache: HashMap::new(),
        }
    }

    /// The user this database belongs to.
    pub fn user(&self) -> &str {
        &self.user
    }

    /// Path to the TOML file for `package`.
    fn file_path(&self, package: &str) -> PathBuf {
        self.base_path.join(format!("{package}.toml"))
    }

    // ── Load / save ───────────────────────────────────────────

    /// Load a package's permissions from disk. Returns an error if the
    /// file exists but is malformed; returns a fresh empty set if the
    /// file does not exist.
    pub fn load(&mut self, package: &str) -> Result<PackagePermissions> {
        let path = self.file_path(package);
        let perms = if path.exists() {
            let content = fs::read_to_string(&path)?;
            let perms: PackagePermissions = toml::from_str(&content)
                .map_err(|e| GatedError::PermissionParse {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
            debug!(package, path = %path.display(), "loaded permissions from disk");
            perms
        } else {
            debug!(package, "no permission file, using empty defaults");
            PackagePermissions::new_empty(package)
        };
        self.cache.insert(package.to_string(), perms.clone());
        Ok(perms)
    }

    /// Write a package's permissions to disk.
    pub fn save(&self, package: &str, perms: &PackagePermissions) -> Result<()> {
        let path = self.file_path(package);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(perms)
            .map_err(|e| GatedError::PermissionSerialize(e.to_string()))?;
        fs::write(&path, content)?;
        debug!(package, path = %path.display(), "saved permissions to disk");
        Ok(())
    }

    // ── Query helpers ─────────────────────────────────────────

    /// Ensure the package is cached; load from disk if needed.
    fn ensure_loaded(&mut self, package: &str) -> Result<()> {
        if !self.cache.contains_key(package) {
            self.load(package)?;
        }
        Ok(())
    }

    /// Check a single capability. Returns `Prompt` if the capability is
    /// not mentioned (i.e. the user has never been asked).
    pub fn check_capability(&mut self, package: &str, capability: &str) -> PermissionGrant {
        if let Err(e) = self.ensure_loaded(package) {
            warn!(package, %e, "failed to load permissions, defaulting to Prompt");
            return PermissionGrant::Prompt;
        }
        self.cache
            .get(package)
            .and_then(|p| p.capabilities.get(capability))
            .cloned()
            .unwrap_or(PermissionGrant::Prompt)
    }

    /// Check mount permissions. Returns `None` if the mount is not listed.
    pub fn check_mount(&mut self, package: &str, mount_path: &str) -> Option<String> {
        let _ = self.ensure_loaded(package);
        self.cache
            .get(package)
            .and_then(|p| p.mounts.get(mount_path))
            .cloned()
    }

    /// Check file permissions.
    ///
    /// Returns:
    /// - `Allow` if the file is listed with a positive permission (e.g. `"r"`, `"rw"`)
    /// - `Deny` if the file is listed with a deny marker (e.g. `"deny(r)"`)
    /// - `Prompt` if the file is not listed at all
    pub fn check_file(&mut self, package: &str, file_path: &str) -> PermissionGrant {
        let _ = self.ensure_loaded(package);
        match self.cache.get(package).and_then(|p| p.files.get(file_path)) {
            None => PermissionGrant::Prompt,
            Some(perm) if perm.starts_with("deny") => PermissionGrant::Deny,
            Some(_) => PermissionGrant::Allow,
        }
    }

    // ── Mutation helpers ──────────────────────────────────────

    /// Grant a capability and persist to disk.
    pub fn grant_capability(&mut self, package: &str, capability: &str) -> Result<()> {
        self.ensure_loaded(package)?;
        let perms = self.cache.entry(package.to_string())
            .or_insert_with(|| PackagePermissions::new_empty(package));
        perms.capabilities.insert(capability.to_string(), PermissionGrant::Allow);
        let perms = perms.clone();
        self.save(package, &perms)
    }

    /// Deny a capability and persist to disk.
    pub fn deny_capability(&mut self, package: &str, capability: &str) -> Result<()> {
        self.ensure_loaded(package)?;
        let perms = self.cache.entry(package.to_string())
            .or_insert_with(|| PackagePermissions::new_empty(package));
        perms.capabilities.insert(capability.to_string(), PermissionGrant::Deny);
        let perms = perms.clone();
        self.save(package, &perms)
    }

    /// Grant a mount path with the specified permission string.
    pub fn grant_mount(&mut self, package: &str, path: &str, permissions: &str) -> Result<()> {
        self.ensure_loaded(package)?;
        let perms = self.cache.entry(package.to_string())
            .or_insert_with(|| PackagePermissions::new_empty(package));
        perms.mounts.insert(path.to_string(), permissions.to_string());
        let perms = perms.clone();
        self.save(package, &perms)
    }

    /// Grant or set a file permission.
    pub fn grant_file(&mut self, package: &str, path: &str, permission: &str) -> Result<()> {
        self.ensure_loaded(package)?;
        let perms = self.cache.entry(package.to_string())
            .or_insert_with(|| PackagePermissions::new_empty(package));
        perms.files.insert(path.to_string(), permission.to_string());
        let perms = perms.clone();
        self.save(package, &perms)
    }
}

// ── Helpers ───────────────────────────────────────────────────────

/// Produce an RFC 3339-ish timestamp without pulling in the `chrono`
/// crate.  Falls back to epoch if the system clock is unavailable.
fn chrono_now() -> String {
    use std::time::SystemTime;
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => {
            // Good-enough UTC timestamp from epoch seconds.
            let secs = d.as_secs();
            // 86400 = seconds per day
            let days = secs / 86400;
            let day_secs = secs % 86400;
            let hours = day_secs / 3600;
            let minutes = (day_secs % 3600) / 60;
            let seconds = day_secs % 60;

            // Zeller-ish days-since-epoch → Y/M/D.
            // Algorithm from Howard Hinnant's `chrono`-compatible date lib.
            let z = days as i64 + 719468;
            let era = if z >= 0 { z } else { z - 146096 } / 146097;
            let doe = (z - era * 146097) as u64;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let y = yoe as i64 + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = if m <= 2 { y + 1 } else { y };

            format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
        }
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TOML: &str = r#"
[meta]
package = "firefox"
first_prompted = "2025-01-15T14:30:00Z"

[capabilities]
gpu = "allow"
audio = "allow"
display = "allow"
net_outbound = "allow"
camera = "deny"
clipboard = "allow"

[mounts]
"~/Downloads" = "list,w"
"~/.mozilla" = "rw"

[files]
"~/.ssh/id_rsa.pub" = "r"
"~/.ssh/id_rsa" = "deny(r)"
"#;

    #[test]
    fn parse_sample_toml() {
        let perms: PackagePermissions = toml::from_str(SAMPLE_TOML).unwrap();
        assert_eq!(perms.meta.package, "firefox");
        assert_eq!(perms.capabilities.get("gpu"), Some(&PermissionGrant::Allow));
        assert_eq!(perms.capabilities.get("camera"), Some(&PermissionGrant::Deny));
        assert_eq!(perms.mounts.get("~/Downloads"), Some(&"list,w".to_string()));
        assert_eq!(perms.files.get("~/.ssh/id_rsa"), Some(&"deny(r)".to_string()));
    }

    #[test]
    fn roundtrip_toml() {
        let perms: PackagePermissions = toml::from_str(SAMPLE_TOML).unwrap();
        let serialized = toml::to_string_pretty(&perms).unwrap();
        let perms2: PackagePermissions = toml::from_str(&serialized).unwrap();
        assert_eq!(perms.meta.package, perms2.meta.package);
        assert_eq!(perms.capabilities, perms2.capabilities);
        assert_eq!(perms.mounts, perms2.mounts);
        assert_eq!(perms.files, perms2.files);
    }

    #[test]
    fn new_empty_has_no_grants() {
        let perms = PackagePermissions::new_empty("test-pkg");
        assert_eq!(perms.meta.package, "test-pkg");
        assert!(perms.capabilities.is_empty());
        assert!(perms.mounts.is_empty());
        assert!(perms.files.is_empty());
    }

    // ── PermissionDb on-disk tests ────────────────────────────

    #[test]
    fn db_load_save_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = PermissionDb::new("alice", dir.path().to_path_buf());

        let perms = {
            let mut p = PackagePermissions::new_empty("firefox");
            p.capabilities.insert("gpu".into(), PermissionGrant::Allow);
            p.capabilities.insert("camera".into(), PermissionGrant::Deny);
            p.mounts.insert("~/Downloads".into(), "list,w".into());
            p.files.insert("~/.ssh/id_rsa".into(), "deny(r)".into());
            p
        };

        db.save("firefox", &perms).unwrap();

        // Fresh DB instance to prove it reads from disk, not cache
        let mut db2 = PermissionDb::new("alice", dir.path().to_path_buf());
        let loaded = db2.load("firefox").unwrap();
        assert_eq!(loaded.meta.package, "firefox");
        assert_eq!(loaded.capabilities.get("gpu"), Some(&PermissionGrant::Allow));
        assert_eq!(loaded.capabilities.get("camera"), Some(&PermissionGrant::Deny));
        assert_eq!(loaded.mounts.get("~/Downloads"), Some(&"list,w".to_string()));
        assert_eq!(loaded.files.get("~/.ssh/id_rsa"), Some(&"deny(r)".to_string()));
    }

    #[test]
    fn check_capability_allow_deny_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = PermissionDb::new("alice", dir.path().to_path_buf());

        db.grant_capability("firefox", "gpu").unwrap();
        db.deny_capability("firefox", "camera").unwrap();

        assert_eq!(db.check_capability("firefox", "gpu"), PermissionGrant::Allow);
        assert_eq!(db.check_capability("firefox", "camera"), PermissionGrant::Deny);
        // Missing capability → Prompt
        assert_eq!(db.check_capability("firefox", "microphone"), PermissionGrant::Prompt);
    }

    #[test]
    fn check_mount_present_and_absent() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = PermissionDb::new("alice", dir.path().to_path_buf());

        db.grant_mount("firefox", "~/Downloads", "list,w").unwrap();

        assert_eq!(db.check_mount("firefox", "~/Downloads"), Some("list,w".to_string()));
        assert_eq!(db.check_mount("firefox", "~/Documents"), None);
    }

    #[test]
    fn check_file_allow_deny_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = PermissionDb::new("alice", dir.path().to_path_buf());

        db.grant_file("firefox", "~/.ssh/id_rsa.pub", "r").unwrap();
        db.grant_file("firefox", "~/.ssh/id_rsa", "deny(r)").unwrap();

        assert_eq!(db.check_file("firefox", "~/.ssh/id_rsa.pub"), PermissionGrant::Allow);
        assert_eq!(db.check_file("firefox", "~/.ssh/id_rsa"), PermissionGrant::Deny);
        assert_eq!(db.check_file("firefox", "/tmp/unknown"), PermissionGrant::Prompt);
    }

    #[test]
    fn grant_capability_then_recheck() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = PermissionDb::new("alice", dir.path().to_path_buf());

        // Initially Prompt
        assert_eq!(db.check_capability("vlc", "audio"), PermissionGrant::Prompt);

        // Grant it
        db.grant_capability("vlc", "audio").unwrap();
        assert_eq!(db.check_capability("vlc", "audio"), PermissionGrant::Allow);

        // Verify it's on disk too
        let mut db2 = PermissionDb::new("alice", dir.path().to_path_buf());
        assert_eq!(db2.check_capability("vlc", "audio"), PermissionGrant::Allow);
    }

    #[test]
    fn load_nonexistent_gives_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = PermissionDb::new("alice", dir.path().to_path_buf());
        let perms = db.load("nonexistent-pkg").unwrap();
        assert_eq!(perms.meta.package, "nonexistent-pkg");
        assert!(perms.capabilities.is_empty());
    }
}
