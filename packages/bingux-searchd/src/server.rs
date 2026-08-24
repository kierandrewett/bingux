use std::{
    fs,
    io::{self, BufRead},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixListener,
    },
    path::Path,
};

const RUNTIME_DIRECTORY_MODE: u32 = 0o700;
const SOCKET_MODE: u32 = 0o600;

pub fn bind_listener(socket_path: &Path) -> io::Result<UnixListener> {
    prepare_socket_path(socket_path)?;
    let listener = UnixListener::bind(socket_path)?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(SOCKET_MODE))?;
    Ok(listener)
}

pub fn read_record<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut record = Vec::with_capacity(crate::protocol::MAX_RECORD_BYTES + 1);

    loop {
        let (consumed, ends_record) = {
            let buffer = reader.fill_buf()?;
            if buffer.is_empty() {
                if record.is_empty() {
                    return Ok(None);
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "search client closed a record before its newline delimiter",
                ));
            }

            let remaining = crate::protocol::MAX_RECORD_BYTES + 1 - record.len();
            let newline = buffer.iter().position(|byte| *byte == b'\n');
            let consumed = newline
                .map_or(buffer.len(), |index| index + 1)
                .min(remaining);
            let ends_record = newline.is_some_and(|index| index + 1 == consumed);
            record.extend_from_slice(&buffer[..consumed]);
            (consumed, ends_record)
        };
        reader.consume(consumed);

        if ends_record {
            record.pop();
            return Ok(Some(record));
        }
        if record.len() > crate::protocol::MAX_RECORD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "search client sent a record that exceeds the protocol limit",
            ));
        }
    }
}

pub fn prepare_socket_path(socket_path: &Path) -> io::Result<()> {
    let parent = socket_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "search socket path must have a parent directory",
        )
    })?;

    fs::create_dir_all(parent)?;
    let metadata = fs::metadata(parent)?;
    if metadata.uid() != libc_geteuid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "search runtime directory is not owned by the active user",
        ));
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(RUNTIME_DIRECTORY_MODE))?;

    match fs::symlink_metadata(socket_path) {
        Ok(metadata) if metadata.file_type().is_socket() => fs::remove_file(socket_path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "refusing to replace a non-socket search path",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

unsafe extern "C" {
    fn geteuid() -> u32;
}

fn libc_geteuid() -> u32 {
    // SAFETY: geteuid has no arguments, no preconditions, and cannot invalidate Rust memory.
    unsafe { geteuid() }
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
