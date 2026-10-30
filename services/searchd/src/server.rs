use std::{
    io::{self, BufRead},
    os::unix::net::UnixListener,
    path::Path,
};

/// Binds the shell socket: the framing and permissions (a 0700 directory, a
/// 0600 socket, a stale socket replaced) are `bingux_search::ipc`'s, shared
/// with the app search sockets.
pub fn bind_listener(socket_path: &Path) -> io::Result<UnixListener> {
    bingux_search::ipc::bind_listener(socket_path)
}

/// Reads one newline-delimited record of at most the protocol limit.
pub fn read_record<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    bingux_search::ipc::read_record(reader, crate::protocol::MAX_RECORD_BYTES)
}

pub fn prepare_socket_path(socket_path: &Path) -> io::Result<()> {
    bingux_search::ipc::prepare_socket_path(socket_path)
}

#[cfg(test)]
mod tests {
    use super::{prepare_socket_path, read_record};
    use std::{
        fs,
        io::Cursor,
        os::unix::net::UnixListener,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary_socket_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bingux-searchd-test-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn replaces_only_a_stale_socket_file() {
        let directory = temporary_socket_path();
        fs::create_dir_all(&directory).expect("create test directory");
        let socket_path = directory.join("search.sock");
        let listener = UnixListener::bind(&socket_path).expect("create stale socket");
        drop(listener);

        prepare_socket_path(&socket_path).expect("remove stale socket");

        assert!(!socket_path.exists());
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn refuses_to_remove_an_existing_non_socket_file() {
        let directory = temporary_socket_path();
        fs::create_dir_all(&directory).expect("create test directory");
        let socket_path = directory.join("search.sock");
        fs::write(&socket_path, "not a socket").expect("write regular file");

        assert!(prepare_socket_path(&socket_path).is_err());
        assert!(socket_path.exists());
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn rejects_a_record_that_exceeds_the_protocol_limit() {
        let input = vec![b'x'; crate::protocol::MAX_RECORD_BYTES + 1];
        let mut reader = Cursor::new(input);

        assert!(read_record(&mut reader).is_err());
    }
}
