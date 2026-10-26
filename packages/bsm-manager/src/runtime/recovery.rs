//! Liveness probe for sessions found in the persisted registry after a restart.
//!
//! A registry entry is alive only if its bridge process still exists and that process
//! is still inside the recorded scope. Nothing else (a pid alone, a file that exists) is
//! accepted, so a reused pid or a stale record never resurrects a session.

use std::fs;
use std::path::Path;

use crate::core::{PersistedSession, RecoveredSession, cgroup_in_scope};

/// The unified cgroup path of `pid` from `/proc/<pid>/cgroup`, if the process exists.
pub fn cgroup_of(proc_root: &Path, pid: u32) -> Option<String> {
    let text = fs::read_to_string(proc_root.join(pid.to_string()).join("cgroup")).ok()?;
    text.lines().find_map(|l| l.strip_prefix("0::").map(str::to_owned))
}

pub fn is_alive(proc_root: &Path, s: &PersistedSession) -> bool {
    s.host_pid != 0 && cgroup_of(proc_root, s.host_pid).is_some_and(|cg| cgroup_in_scope(&cg, &s.scope))
}

pub fn probe(proc_root: &Path, sessions: Vec<PersistedSession>) -> Vec<RecoveredSession> {
    sessions.into_iter().map(|session| RecoveredSession { alive: is_alive(proc_root, &session), session }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_proc(entries: &[(u32, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (pid, cg) in entries {
            let p = d.path().join(pid.to_string());
            fs::create_dir_all(&p).unwrap();
            fs::write(p.join("cgroup"), format!("0::{cg}\n")).unwrap();
        }
        d
    }

    fn sess(pid: u32, scope: &str) -> PersistedSession {
        PersistedSession { id: "s1".into(), generation: 1, uid: 1000, user_name: "a".into(), scope: scope.into(), host_pid: pid }
    }

    #[test]
    fn only_a_live_process_in_the_recorded_scope_counts() {
        let d = fake_proc(&[(10, "/user.slice/user-1000.slice/session-4.scope/bridge"), (11, "/user.slice/user-1001.slice/session-9.scope")]);
        assert!(is_alive(d.path(), &sess(10, "session-4.scope")));
        // The pid exists but belongs to another scope: a reused pid is not the session.
        assert!(!is_alive(d.path(), &sess(11, "session-4.scope")));
        // The pid is gone.
        assert!(!is_alive(d.path(), &sess(12, "session-4.scope")));
        assert!(!is_alive(d.path(), &sess(0, "session-4.scope")));
    }
}
