//! What a typed location turns out to be, found without ever hanging the UI.
//!
//! `stat` on a network mount whose server is gone can block for minutes. The
//! check runs on its own thread and the caller waits for it with a deadline,
//! so the answer is "not responding" and not a frozen window.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use bingux_files::listing::Cancel;

/// How long a typed location may take to answer.
pub(super) const DEADLINE: Duration = Duration::from_secs(3);

/// Cancels a listing when it is dropped, so a task that is thrown away
/// (its list closed, its component gone) stops the worker's reading.
pub(super) struct StopOnDrop(pub Cancel);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Why a typed location cannot be opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Problem {
    NotFound,
    /// A folder on the way is a file.
    NotAFolder,
    NoPermission,
    NotResponding,
}

impl Problem {
    /// The reason in plain words, to put before the path.
    pub(super) fn reason(self) -> &'static str {
        match self {
            Problem::NotFound => "Not found",
            Problem::NotAFolder => "Not a folder",
            Problem::NoPermission => "No permission",
            Problem::NotResponding => "Not responding",
        }
    }

    fn of(error: &io::Error) -> Problem {
        match error.kind() {
            io::ErrorKind::PermissionDenied => Problem::NoPermission,
            io::ErrorKind::NotADirectory => Problem::NotAFolder,
            io::ErrorKind::TimedOut => Problem::NotResponding,
            _ => Problem::NotFound,
        }
    }
}

/// What a path is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Folder(PathBuf),
    File(PathBuf),
    Problem(Problem),
}

/// Look at `path`. Blocks for at most `deadline`; call it from a worker.
pub(super) fn check(path: &Path, deadline: Duration) -> Target {
    let (send, receive) = mpsc::channel();
    let asked = path.to_path_buf();
    // A thread of its own, not the shared pool: one that never returns must
    // not take a pool worker with it.
    let spawned = std::thread::Builder::new()
        .name("bingux-address-check".into())
        .spawn(move || {
            let _ = send.send(std::fs::metadata(&asked));
        });
    if spawned.is_err() {
        return Target::Problem(Problem::NotResponding);
    }
    match receive.recv_timeout(deadline) {
        Ok(Ok(meta)) if meta.is_dir() => Target::Folder(path.to_path_buf()),
        Ok(Ok(_)) => Target::File(path.to_path_buf()),
        Ok(Err(error)) => Target::Problem(Problem::of(&error)),
        Err(_) => Target::Problem(Problem::NotResponding),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn tells_folders_files_and_problems_apart() {
        let dir = std::env::temp_dir().join(format!("bingux-address-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("file.txt"), b"x").unwrap();
        let quick = Duration::from_secs(5);
        assert_eq!(
            check(&dir.join("sub"), quick),
            Target::Folder(dir.join("sub"))
        );
        assert_eq!(
            check(&dir.join("file.txt"), quick),
            Target::File(dir.join("file.txt"))
        );
        assert_eq!(
            check(&dir.join("gone"), quick),
            Target::Problem(Problem::NotFound)
        );
        assert_eq!(
            check(&dir.join("file.txt/inside"), quick),
            Target::Problem(Problem::NotAFolder)
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
