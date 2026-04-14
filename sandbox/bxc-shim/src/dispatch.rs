use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::ShimError;

#[derive(Debug, Deserialize)]
pub struct DispatchTable {
    #[serde(flatten)]
    pub entries: HashMap<String, DispatchEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchEntry {
    pub package: String,
    pub binary: String,
    pub sandbox: String,
}

impl DispatchTable {
    /// Load and parse a dispatch table from a TOML file.
    pub fn load(path: &Path) -> Result<Self, ShimError> {
        let content = std::fs::read_to_string(path).map_err(|_| {
            ShimError::DispatchTableNotFound(path.to_path_buf())
        })?;
        Self::from_str(&content)
    }

    /// Parse a dispatch table from a TOML string.
    pub fn from_str(content: &str) -> Result<Self, ShimError> {
        toml::from_str(content).map_err(|e| ShimError::DispatchTableParse(e.to_string()))
    }

    /// Look up a binary name in the dispatch table.
    pub fn lookup(&self, binary_name: &str) -> Option<&DispatchEntry> {
        self.entries.get(binary_name)
    }
}

/// Resolve dispatch table with user -> system fallback.
///
/// 1. Try user table: ~/.config/bingux/profiles/current/.dispatch.toml
/// 2. Fall back to system table: /system/profiles/current/.dispatch.toml
pub fn resolve_dispatch(binary_name: &str, uid: u32) -> Result<DispatchEntry, ShimError> {
    resolve_dispatch_with_roots(binary_name, uid, "/system", "/home")
}

/// Internal resolver that accepts configurable root paths (for testing).
pub fn resolve_dispatch_with_roots(
    binary_name: &str,
    uid: u32,
    system_root: &str,
    home_root: &str,
) -> Result<DispatchEntry, ShimError> {
    // Try user table first (only for non-root users)
    if uid != 0 {
        let user_table_path = user_dispatch_path(uid, home_root);
        if user_table_path.exists() {
            let table = DispatchTable::load(&user_table_path)?;
            if let Some(entry) = table.lookup(binary_name) {
                return Ok(entry.clone());
            }
        }
    }

    // Fall back to system table
    let system_table_path = system_dispatch_path(system_root);
    if system_table_path.exists() {
        let table = DispatchTable::load(&system_table_path)?;
        if let Some(entry) = table.lookup(binary_name) {
            return Ok(entry.clone());
        }
    }

    Err(ShimError::BinaryNotFound(binary_name.to_string()))
}

/// Path to the user dispatch table.
fn user_dispatch_path(uid: u32, home_root: &str) -> PathBuf {
    // In Bingux, user homes are under /users/<uid>
    PathBuf::from(home_root)
        .join(uid.to_string())
        .join(".config/bingux/profiles/current/.dispatch.toml")
}

/// Path to the system dispatch table.
fn system_dispatch_path(system_root: &str) -> PathBuf {
    PathBuf::from(system_root).join("profiles/current/.dispatch.toml")
}
