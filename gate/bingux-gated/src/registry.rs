//! PID registry — tracks which sandbox PIDs belong to which packages.
//!
//! When a sandbox is launched the shim registers the child PID here so
//! the daemon can map incoming seccomp notifications back to a package
//! identity and the owning user.

use std::collections::HashMap;
use std::time::SystemTime;

use bingux_common::PackageId;

// ── Types ─────────────────────────────────────────────────────────

/// Metadata about a single running sandbox process.
#[derive(Debug, Clone)]
pub struct SandboxEntry {
    pub package_name: String,
    pub package_id: PackageId,
    pub user: String,
    pub uid: u32,
    /// The seccomp listener fd, if this sandbox uses seccomp-notify.
    pub listener_fd: Option<i32>,
    pub started_at: SystemTime,
}

/// An in-memory map from PID → sandbox metadata.
///
/// Thread-safety is left to the caller (e.g. wrap in `Arc<Mutex<_>>`
/// or use from a single-threaded async runtime).
#[derive(Debug, Default)]
pub struct PidRegistry {
    entries: HashMap<u32, SandboxEntry>,
}

impl PidRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a sandbox PID.  Overwrites any previous entry for the
    /// same PID (e.g. PID reuse).
    pub fn register(&mut self, pid: u32, entry: SandboxEntry) {
        self.entries.insert(pid, entry);
    }

    /// Remove a sandbox PID, returning its entry if it existed.
    pub fn unregister(&mut self, pid: u32) -> Option<SandboxEntry> {
        self.entries.remove(&pid)
    }

    /// Look up metadata for a PID.
    pub fn lookup(&self, pid: u32) -> Option<&SandboxEntry> {
        self.entries.get(&pid)
    }

    /// Find all sandbox PIDs belonging to a given package name.
    pub fn lookup_by_package(&self, package: &str) -> Vec<(u32, &SandboxEntry)> {
        self.entries
            .iter()
            .filter(|(_, e)| e.package_name == package)
            .map(|(&pid, entry)| (pid, entry))
            .collect()
    }

    /// List every registered sandbox.
    pub fn list(&self) -> Vec<(u32, &SandboxEntry)> {
        self.entries.iter().map(|(&pid, entry)| (pid, entry)).collect()
    }

    /// How many sandboxes are currently tracked.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_common::package_id::Arch;

    fn make_entry(name: &str, user: &str) -> SandboxEntry {
        SandboxEntry {
            package_name: name.to_string(),
            package_id: PackageId::new(name, "1.0", Arch::X86_64Linux).unwrap(),
            user: user.to_string(),
            uid: 1000,
            listener_fd: None,
            started_at: SystemTime::now(),
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut reg = PidRegistry::new();
        reg.register(100, make_entry("firefox", "alice"));
        let entry = reg.lookup(100).unwrap();
        assert_eq!(entry.package_name, "firefox");
        assert_eq!(entry.user, "alice");
    }

    #[test]
    fn lookup_missing() {
        let reg = PidRegistry::new();
        assert!(reg.lookup(999).is_none());
    }

    #[test]
    fn unregister() {
        let mut reg = PidRegistry::new();
        reg.register(100, make_entry("firefox", "alice"));
        let removed = reg.unregister(100);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().package_name, "firefox");
        assert!(reg.lookup(100).is_none());
    }

    #[test]
    fn unregister_missing() {
        let mut reg = PidRegistry::new();
        assert!(reg.unregister(999).is_none());
    }

    #[test]
    fn lookup_by_package() {
        let mut reg = PidRegistry::new();
        reg.register(100, make_entry("firefox", "alice"));
        reg.register(200, make_entry("firefox", "bob"));
        reg.register(300, make_entry("vlc", "alice"));

        let ff = reg.lookup_by_package("firefox");
        assert_eq!(ff.len(), 2);

        let vlc = reg.lookup_by_package("vlc");
        assert_eq!(vlc.len(), 1);

        let empty = reg.lookup_by_package("nonexistent");
        assert!(empty.is_empty());
    }

    #[test]
    fn list_all() {
        let mut reg = PidRegistry::new();
        reg.register(100, make_entry("firefox", "alice"));
        reg.register(200, make_entry("vlc", "alice"));
        assert_eq!(reg.list().len(), 2);
        assert_eq!(reg.len(), 2);
        assert!(!reg.is_empty());
    }

    #[test]
    fn empty_registry() {
        let reg = PidRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.list().is_empty());
    }

    #[test]
    fn pid_reuse_overwrites() {
        let mut reg = PidRegistry::new();
        reg.register(100, make_entry("firefox", "alice"));
        reg.register(100, make_entry("vlc", "bob"));
        let entry = reg.lookup(100).unwrap();
        assert_eq!(entry.package_name, "vlc");
        assert_eq!(entry.user, "bob");
    }
}
