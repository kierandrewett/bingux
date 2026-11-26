//! One copy of an application: the socket a running copy listens on, and
//! what a second launch hands it.
//!
//! A launch sends one line per connection and waits for the copy's `ok`: a
//! copy that is closing (it still has the socket, but takes nothing) answers
//! nothing, so the launch starts a copy of its own instead of exiting with
//! its address lost. A connection that sends nothing is a probe and raises
//! nothing. The copy removes its socket when it ends, unless a newer copy
//! has replaced the file; a socket left by a crash is found unanswered and
//! replaced.

use std::io::{self, BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long either side waits for the other.
const PATIENCE: Duration = Duration::from_secs(2);

/// What the copy answers once it has a line.
const TAKEN: &str = "ok";

/// Sends `line` on a new connection and waits for the copy to take it.
fn send(socket: &Path, line: &str) -> io::Result<bool> {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return Ok(false);
    };
    stream.set_read_timeout(Some(PATIENCE))?;
    writeln!(stream, "{line}")?;
    stream.shutdown(Shutdown::Write)?;
    let mut answer = String::new();
    // A copy that closed before it read the line resets the connection or
    // ends it without an answer: nobody took it.
    let taken = BufReader::new(stream).read_line(&mut answer).is_ok() && answer.trim_end() == TAKEN;
    Ok(taken)
}

/// Hands `lines` to the copy listening on `socket`, one connection each.
/// `Ok(true)` when a running copy took them, `Ok(false)` when none took the
/// first (the launch runs a copy of its own), an error when a copy took
/// some and then stopped answering.
pub(super) fn hand_off(socket: &Path, lines: &[String]) -> io::Result<bool> {
    for (at, line) in lines.iter().enumerate() {
        if !send(socket, line)? {
            if at == 0 {
                return Ok(false);
            }
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "the running copy stopped answering",
            ));
        }
    }
    Ok(true)
}

/// The socket this copy listens on. Dropping it removes the socket file,
/// if it is still this copy's.
pub(super) struct Owner {
    listener: Option<UnixListener>,
    path: PathBuf,
    /// The file's device and inode when it was bound: which file is ours.
    file: (u64, u64),
}

impl Owner {
    /// Listens on `socket`. A socket file nobody answers on is left over
    /// from a copy that is gone and is replaced; one a copy answers on is
    /// refused (`AddrInUse`) and left alone.
    pub(super) fn claim(socket: &Path) -> io::Result<Self> {
        let listener = match UnixListener::bind(socket) {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                if UnixStream::connect(socket).is_ok() {
                    return Err(error);
                }
                std::fs::remove_file(socket)?;
                UnixListener::bind(socket)?
            }
            Err(error) => return Err(error),
        };
        let meta = std::fs::symlink_metadata(socket)?;
        Ok(Self {
            listener: Some(listener),
            path: socket.to_path_buf(),
            file: (meta.dev(), meta.ino()),
        })
    }

    /// The listener, for the thread that accepts connections.
    pub(super) fn take_listener(&mut self) -> Option<UnixListener> {
        self.listener.take()
    }

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        let ours = std::fs::symlink_metadata(&self.path)
            .is_ok_and(|meta| (meta.dev(), meta.ino()) == self.file);
        if ours {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Accepts connections and passes each line to `push`, answering `ok`. A
/// client sends one line per connection; an empty line only raises the
/// window, and a connection that sends nothing (a probe) is ignored.
pub(super) fn listen(listener: UnixListener, push: impl Fn(String)) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        // A client that connects and says nothing must not hold the listener.
        let _ = stream.set_read_timeout(Some(PATIENCE));
        let _ = stream.set_write_timeout(Some(PATIENCE));
        let mut line = String::new();
        let read = stream
            .try_clone()
            .and_then(|reader| BufReader::new(reader).read_line(&mut line));
        if matches!(read, Ok(read) if read > 0) {
            push(line.trim_end().to_string());
            let _ = writeln!(stream, "{TAKEN}");
        }
    }
}

#[cfg(test)]
mod tests {
    //! Failures these guard against:
    //!   1. a normal exit leaves `<app-id>.sock` behind (seen after a private
    //!      browser window closed);
    //!   2. a window that closes while a second launch connects swallows the
    //!      launch: the second copy exits and nothing opens;
    //!   3. a socket left by a crash (panic, abort, kill) blocks the next start;
    //!   4. a second launch of a running copy (a second private window) opens a
    //!      second copy instead of handing its address over;
    //!   5. a copy that exits removes the socket of a newer copy that replaced
    //!      it;
    //!   6. a probe or a client that says nothing is taken as "raise".

    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    use super::*;

    fn temp_socket(name: &str) -> (PathBuf, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("bingux-ui-instance-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("app.sock");
        (dir, socket)
    }

    /// A running copy: claims `socket` and collects what it is handed.
    fn running(socket: &Path) -> (Owner, Arc<Mutex<Vec<String>>>) {
        let mut owner = Owner::claim(socket).unwrap();
        let got = Arc::new(Mutex::new(Vec::new()));
        let listener = owner.take_listener().unwrap();
        let sink = Arc::clone(&got);
        std::thread::spawn(move || listen(listener, |line| sink.lock().unwrap().push(line)));
        (owner, got)
    }

    fn wait_for(got: &Arc<Mutex<Vec<String>>>, count: usize) -> Vec<String> {
        for _ in 0..200 {
            if got.lock().unwrap().len() >= count {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        got.lock().unwrap().clone()
    }

    #[test]
    fn a_normal_exit_removes_the_socket() {
        let (dir, socket) = temp_socket("exit");
        let (owner, _) = running(&socket);
        assert!(socket.exists());
        drop(owner);
        assert!(!socket.exists(), "the socket goes with the copy");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_second_launch_is_handed_to_the_running_copy() {
        let (dir, socket) = temp_socket("second");
        let (owner, got) = running(&socket);
        let lines = vec!["bingux-browser://a".to_string(), String::new()];
        assert!(hand_off(&socket, &lines).unwrap());
        assert_eq!(wait_for(&got, 2), lines);
        drop(owner);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_socket_left_by_a_crash_does_not_block_the_next_start() {
        let (dir, socket) = temp_socket("crash");
        // What a panic=abort or a kill leaves: the file, and nobody on it.
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        assert!(!hand_off(&socket, &["x".to_string()]).unwrap());
        let (owner, got) = running(&socket);
        assert!(hand_off(&socket, &["y".to_string()]).unwrap());
        assert_eq!(wait_for(&got, 1), ["y"]);
        drop(owner);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_copy_that_closes_while_a_launch_connects_does_not_swallow_it() {
        let (dir, socket) = temp_socket("closing");
        // A copy that is closing: it still has the socket, but accepts nothing.
        let owner = Owner::claim(&socket).unwrap();
        let (sent, result) = mpsc::channel();
        let path = socket.clone();
        let client = std::thread::spawn(move || {
            sent.send(hand_off(&path, &["late".to_string()]).unwrap())
                .unwrap();
        });
        std::thread::sleep(Duration::from_millis(100));
        drop(owner);
        client.join().unwrap();
        assert!(
            !result.recv().unwrap(),
            "nobody took it, so the launch starts a copy of its own"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_old_copy_leaves_a_newer_copys_socket_alone() {
        let (dir, socket) = temp_socket("newer");
        let old = Owner::claim(&socket).unwrap();
        // The old copy hung; a newer one replaced its socket file.
        std::fs::remove_file(&socket).unwrap();
        let (newer, got) = running(&socket);
        drop(old);
        assert!(socket.exists(), "the newer copy's socket stays");
        assert!(hand_off(&socket, &["z".to_string()]).unwrap());
        assert_eq!(wait_for(&got, 1), ["z"]);
        drop(newer);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_probe_that_says_nothing_is_not_a_launch() {
        let (dir, socket) = temp_socket("probe");
        let (owner, got) = running(&socket);
        drop(UnixStream::connect(&socket).unwrap());
        assert!(hand_off(&socket, &["after".to_string()]).unwrap());
        assert_eq!(wait_for(&got, 1), ["after"], "the probe raised nothing");
        assert_eq!(owner.path(), socket);
        drop(owner);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
