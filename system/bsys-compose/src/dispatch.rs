use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use bingux_common::error::Result;
use bxc_sandbox::SandboxLevel;

/// The dispatch table maps binary names (argv[0]) to their backing package
/// and sandbox configuration.  Serialised as `.dispatch.toml` inside each
/// generation directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DispatchTable {
    /// Binary name → dispatch entry.
    #[serde(flatten)]
    pub entries: BTreeMap<String, DispatchEntry>,
}

/// A single entry in the dispatch table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchEntry {
    /// The package that owns this binary (e.g. `"firefox-129.0-x86_64-linux"`).
    pub package: String,
    /// Relative path to the binary within the package directory
    /// (e.g. `"bin/firefox"`).
    pub binary: String,
    /// Sandbox level to apply when launching this binary.
    pub sandbox: SandboxLevel,
}

impl DispatchTable {
    /// Create an empty dispatch table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an entry, keyed by the bare binary name (e.g. `"firefox"`).
    pub fn insert(&mut self, name: String, entry: DispatchEntry) {
        self.entries.insert(name, entry);
    }

    /// Look up a binary by name.
    pub fn get(&self, name: &str) -> Option<&DispatchEntry> {
        self.entries.get(name)
    }

    /// Serialise to TOML.
    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Deserialise from a TOML string.
    pub fn from_toml(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    /// Write to a file.
    pub fn write_to(&self, path: &Path) -> Result<()> {
        let content = self.to_toml()?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Read from a file.
    pub fn read_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_table_roundtrip() {
        let mut table = DispatchTable::new();
        table.insert(
            "firefox".into(),
            DispatchEntry {
                package: "firefox-129.0-x86_64-linux".into(),
                binary: "bin/firefox".into(),
                sandbox: SandboxLevel::Standard,
            },
        );
        table.insert(
            "bash".into(),
            DispatchEntry {
                package: "bash-5.2-x86_64-linux".into(),
                binary: "bin/bash".into(),
                sandbox: SandboxLevel::Minimal,
            },
        );
        table.insert(
            "ls".into(),
            DispatchEntry {
                package: "coreutils-9.5-x86_64-linux".into(),
                binary: "bin/ls".into(),
                sandbox: SandboxLevel::None,
            },
        );

        let toml_str = table.to_toml().unwrap();
        let parsed = DispatchTable::from_toml(&toml_str).unwrap();

        assert_eq!(parsed.entries.len(), 3);

        let ff = parsed.get("firefox").unwrap();
        assert_eq!(ff.package, "firefox-129.0-x86_64-linux");
        assert_eq!(ff.binary, "bin/firefox");
        assert_eq!(ff.sandbox, SandboxLevel::Standard);

        let ls = parsed.get("ls").unwrap();
        assert_eq!(ls.sandbox, SandboxLevel::None);
    }

    #[test]
    fn dispatch_table_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".dispatch.toml");

        let mut table = DispatchTable::new();
        table.insert(
            "cat".into(),
            DispatchEntry {
                package: "coreutils-9.5-x86_64-linux".into(),
                binary: "bin/cat".into(),
                sandbox: SandboxLevel::None,
            },
        );

        table.write_to(&path).unwrap();
        let loaded = DispatchTable::read_from(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.get("cat").unwrap().binary, "bin/cat");
    }
}
