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

/// The owner uid of `/proc/<pid>`.
fn proc_owner(proc_root: &Path, pid: u32) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(proc_root.join(pid.to_string())).ok().map(|m| m.uid())
}

/// Is this a record bsm itself could have written? `state.json` is trusted only after the
/// same checks that D-Bus input gets (F15): the id becomes part of a socket path, the scope and
/// pid are matched against `/proc`.
pub fn is_valid(s: &PersistedSession) -> bool {
    let id_ok = !s.id.is_empty() && s.id.len() <= 64 && s.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    let scope_ok = !s.scope.is_empty() && s.scope.len() <= 256 && !s.scope.bytes().any(|b| b < 0x20 || b == 0x7f);
    id_ok && scope_ok && s.generation != 0 && s.uid != 0 && s.uid != u32::MAX
}

/// Alive: a valid record whose bridge process still exists, belongs to the session's uid and
/// is still inside the recorded scope. A reused pid of another uid or scope is not the session.
pub fn is_alive(proc_root: &Path, s: &PersistedSession) -> bool {
    is_valid(s)
        && s.host_pid != 0
        && proc_owner(proc_root, s.host_pid) == Some(s.uid)
        && cgroup_of(proc_root, s.host_pid).is_some_and(|cg| cgroup_in_scope(&cg, &s.scope))
}

/// Probe the persisted registry. Invalid records are dropped here (and logged), so nothing
/// downstream sees an id or scope it did not validate.
pub fn probe(proc_root: &Path, sessions: Vec<PersistedSession>) -> Vec<RecoveredSession> {
    sessions
        .into_iter()
        .filter(|s| {
            let ok = is_valid(s);
            if !ok {
                tracing::warn!("a persisted session record is invalid and was dropped");
            }
            ok
        })
        .map(|session| RecoveredSession { alive: is_alive(proc_root, &session), session })
        .collect()
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

    fn me() -> u32 {
        unsafe { libc::getuid() }
    }

    fn sess(pid: u32, scope: &str) -> PersistedSession {
        PersistedSession { id: "s1".into(), generation: 1, uid: me(), user_name: "a".into(), scope: scope.into(), host_pid: pid }
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

    #[test]
    fn f15_records_are_validated_before_they_are_used() {
        let d = fake_proc(&[(10, "/user.slice/session-4.scope")]);
        let mut s = sess(10, "session-4.scope");
        assert!(is_alive(d.path(), &s));
        for bad in [
            PersistedSession { id: "../../etc/x".into(), ..s.clone() },
            PersistedSession { id: String::new(), ..s.clone() },
            PersistedSession { id: "a".repeat(65), ..s.clone() },
            PersistedSession { scope: String::new(), ..s.clone() },
            PersistedSession { scope: "x\ny".into(), ..s.clone() },
            PersistedSession { generation: 0, ..s.clone() },
            PersistedSession { uid: 0, ..s.clone() },
        ] {
            assert!(!is_valid(&bad), "{bad:?}");
            assert!(probe(d.path(), vec![bad]).is_empty(), "invalid records are dropped");
        }
        // The pid belongs to somebody else than the session's uid.
        s.uid = me() + 1;
        assert!(!is_alive(d.path(), &s));
        assert!(!probe(d.path(), vec![s])[0].alive);
    }
}
