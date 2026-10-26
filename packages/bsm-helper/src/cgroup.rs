//! cgroup v2 queries used to find and stop everything in a session scope.

use std::io;
use std::path::{Path, PathBuf};

pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// Parse the unified-hierarchy path out of `/proc/<pid>/cgroup` contents.
pub fn parse_unified_path(text: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix("0::")).map(|p| p.trim().to_owned())
}

/// The unified cgroup path of `pid` (`0::/user.slice/user-1000.slice/session-3.scope`).
pub fn path_of(pid: u32) -> io::Result<String> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
    parse_unified_path(&text).ok_or_else(|| io::Error::other("no unified cgroup entry"))
}

/// The systemd scope unit a cgroup path names, if its last component is a `.scope`.
pub fn scope_unit(path: &str) -> Option<&str> {
    let last = path.rsplit('/').next()?;
    (last.ends_with(".scope") && last.len() > ".scope".len()).then_some(last)
}

fn procs_file(root: &Path, path: &str) -> PathBuf {
    root.join(path.trim_start_matches('/')).join("cgroup.procs")
}

/// Process ids currently in the cgroup (not its children).
pub fn procs(root: &Path, path: &str) -> io::Result<Vec<u32>> {
    let text = std::fs::read_to_string(procs_file(root, path))?;
    Ok(text.lines().filter_map(|l| l.trim().parse().ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_path_is_found_among_hybrid_lines() {
        let text = "12:cpu:/x\n0::/user.slice/user-1000.slice/session-3.scope\n";
        assert_eq!(parse_unified_path(text).as_deref(), Some("/user.slice/user-1000.slice/session-3.scope"));
        assert_eq!(parse_unified_path("1:name=systemd:/x\n"), None);
    }

    #[test]
    fn scope_unit_requires_a_named_scope() {
        assert_eq!(scope_unit("/user.slice/user-1000.slice/session-3.scope"), Some("session-3.scope"));
        assert_eq!(scope_unit("/system.slice/bsm-helper.service"), None);
        assert_eq!(scope_unit("/x/.scope"), None);
        assert_eq!(scope_unit("/"), None);
    }

    #[test]
    fn own_cgroup_is_readable() {
        let p = path_of(std::process::id()).unwrap();
        assert!(p.starts_with('/'));
    }
}
