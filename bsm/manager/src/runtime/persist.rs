//! Atomic persistence of safe metadata: the last user and the registry of running
//! sessions. Never a secret, never a resumable grant.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::core::Persisted;

pub const FILE_NAME: &str = "state.json";

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// Load what the previous run left. A missing or damaged file is an empty state: the
/// manager then starts with nobody remembered, which is always safe.
pub fn load(dir: &Path) -> Persisted {
    match fs::read(path(dir)) {
        Ok(bytes) => match serde_json::from_slice::<Persisted>(&bytes) {
            Ok(p) if p.version <= 1 => p,
            Ok(_) | Err(_) => {
                tracing::warn!("state file is unreadable or from a newer version; starting empty");
                Persisted::default()
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Persisted::default(),
        Err(e) => {
            tracing::warn!("cannot read the state file: {e}");
            Persisted::default()
        }
    }
}

/// Write to a temporary file in the same directory, flush, rename over the old file and
/// flush the directory. Readers see the old or the new file, never half of one.
pub fn save(dir: &Path, state: &Persisted) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{FILE_NAME}.{}.tmp", std::process::id()));
    let mut f: File = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&tmp)?;
    let body = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    f.write_all(&body)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    fs::rename(&tmp, path(dir))?;
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PersistedSession;

    #[test]
    fn round_trips_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = Persisted {
            version: 1,
            last_used_uid: Some(1000),
            sessions: vec![PersistedSession { id: "s1".into(), generation: 9, uid: 1000, user_name: "a".into(), scope: "x.scope".into(), host_pid: 4 }],
        };
        save(dir.path(), &p).unwrap();
        assert_eq!(load(dir.path()), p);
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from(FILE_NAME)]);
        assert_eq!(fs::metadata(path(dir.path())).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn damaged_or_missing_state_is_empty_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), Persisted::default());
        fs::write(path(dir.path()), b"{not json").unwrap();
        assert_eq!(load(dir.path()), Persisted::default());
        fs::write(path(dir.path()), br#"{"version":99}"#).unwrap();
        assert_eq!(load(dir.path()), Persisted::default());
    }

    #[test]
    fn an_interrupted_write_never_replaces_the_good_file() {
        let dir = tempfile::tempdir().unwrap();
        let good = Persisted { version: 1, last_used_uid: Some(7), sessions: vec![] };
        save(dir.path(), &good).unwrap();
        // A crash after the temp file was written but before the rename leaves only a temp file.
        fs::write(dir.path().join(".state.json.1.tmp"), b"{\"version\":1,\"last_used_uid\":9").unwrap();
        assert_eq!(load(dir.path()), good);
    }
}
