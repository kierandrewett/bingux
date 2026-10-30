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

/// Kill every process in the cgroup and its descendants (`cgroup.kill`, Linux 5.14).
pub fn kill(root: &Path, path: &str) -> io::Result<()> {
    std::fs::write(root.join(path.trim_start_matches('/')).join("cgroup.kill"), "1")
}

/// Move a process into another cgroup.
pub fn move_pid(root: &Path, dest: &str, pid: u32) -> io::Result<()> {
    std::fs::write(procs_file(root, dest), pid.to_string())
}

/// Parse `populated` out of `cgroup.events` contents.
pub fn parse_populated(text: &str) -> Option<bool> {
    text.lines().find_map(|l| l.strip_prefix("populated ")).map(|v| v.trim() == "1")
}

/// Whether the cgroup (or a descendant) has processes. `None` when it is gone.
pub fn populated(root: &Path, path: &str) -> Option<bool> {
    let text = std::fs::read_to_string(root.join(path.trim_start_matches('/')).join("cgroup.events")).ok()?;
    parse_populated(&text)
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

    #[test]
    fn populated_is_read_from_cgroup_events() {
        assert_eq!(parse_populated("populated 1\nfrozen 0\n"), Some(true));
        assert_eq!(parse_populated("populated 0\nfrozen 0\n"), Some(false));
        assert_eq!(parse_populated("frozen 0\n"), None);
    }

    #[test]
    fn missing_cgroups_read_as_gone_and_kill_reports_the_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(populated(tmp.path(), "/nope.scope"), None);
        assert!(kill(tmp.path(), "/nope.scope").is_err());
        std::fs::create_dir(tmp.path().join("a.scope")).unwrap();
        std::fs::write(tmp.path().join("a.scope/cgroup.procs"), "").unwrap();
        move_pid(tmp.path(), "/a.scope", 42).unwrap();
        assert_eq!(procs(tmp.path(), "/a.scope").unwrap(), vec![42]);
    }
}
