//! Changing the signed-in user's password with `passwd`, with no terminal
//! window. `passwd` reads its prompts from the controlling terminal, so it
//! runs on the slave end of a pseudo-terminal. This module answers each prompt
//! with the typed password and reports the outcome in words the sheet can show.
//! The password never reaches a command line, so it is not in the process list.

use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How long the change may take. The passwords are already typed when this
/// starts, so this only covers `passwd` itself and the PAM stack behind it.
const TIMEOUT: Duration = Duration::from_secs(60);
/// How often the terminal is polled for output and the child's exit.
const POLL: Duration = Duration::from_millis(20);

/// The `passwd` program to run. Snapshot builds may point it at a fake, so the
/// real command is never run while testing.
fn program() -> String {
    #[cfg(feature = "snapshot")]
    {
        if let Ok(path) = std::env::var("BINGUX_SETTINGS_PASSWD_BIN") {
            return path;
        }
    }
    "passwd".to_string()
}

/// Change the password of the user who runs this process. `current` is the
/// existing password; `new` is typed once and then confirmed. Returns a
/// message for the user on failure.
pub fn change(current: &str, new: &str) -> Result<(), String> {
    run(&program(), current, new)
}

/// A pseudo-terminal pair: the master is read and written here, the slave
/// becomes the child's terminal.
fn open_pty() -> Result<(OwnedFd, OwnedFd), String> {
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: openpty writes two new descriptors on success. The name and
    // attribute arguments are optional and may be null.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if opened != 0 {
        return Err("A terminal could not be opened for the password change.".into());
    }
    // SAFETY: openpty succeeded, so both descriptors are new and owned by us.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
        // SAFETY: fd is a descriptor we own. Close-on-exec keeps the master
        // out of the child.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok((master, slave))
}

fn run(program: &str, current: &str, new: &str) -> Result<(), String> {
    let (master, slave) = open_pty()?;
    let stdout = slave
        .try_clone()
        .map_err(|_| "The password change could not start.".to_string())?;
    let stderr = slave
        .try_clone()
        .map_err(|_| "The password change could not start.".to_string())?;
    let mut command = Command::new(program);
    command
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::from(slave))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    // SAFETY: runs in the child between fork and exec, after the terminal is
    // on descriptor 0. It only calls setsid and ioctl, which are safe there.
    // The terminal becomes the child's controlling terminal, which is where
    // `passwd` reads its prompts.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| "The password tool could not be started.".to_string())?;
    // Drop the parent's copies of the slave, so the master sees end of file
    // once the child exits.
    drop(command);

    let mut master = File::from(master);
    set_nonblocking(&master)?;
    let mut transcript = String::new();
    let mut pending = String::new();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if read_available(&mut master, &mut transcript, &mut pending)? {
            answer(&mut master, &mut pending, current, new)?;
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|_| "The password change could not be checked.".to_string())?
        {
            // Output written just before the exit may still be buffered.
            read_available(&mut master, &mut transcript, &mut pending)?;
            return if status.success() {
                Ok(())
            } else {
                Err(failure(&transcript))
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The password change took too long and was stopped.".into());
        }
        thread::sleep(POLL);
    }
}

fn set_nonblocking(file: &File) -> Result<(), String> {
    let fd = file.as_raw_fd();
    // SAFETY: fd is the master descriptor, open for the life of `file`.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err("The password change could not start.".into());
    }
    // SAFETY: as above; only the status flags change.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err("The password change could not start.".into());
    }
    Ok(())
}

/// Read whatever the terminal has, without blocking. Returns whether any
/// output arrived. A closed terminal reads as the end of the output.
fn read_available(
    master: &mut File,
    transcript: &mut String,
    pending: &mut String,
) -> Result<bool, String> {
    let mut received = false;
    let mut buffer = [0u8; 512];
    loop {
        match master.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                received = true;
                let text = String::from_utf8_lossy(&buffer[..count]);
                transcript.push_str(&text);
                pending.push_str(&text);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            // EIO: the child closed the terminal.
            Err(_) => break,
        }
    }
    Ok(received)
}

/// Answer the prompt on the last line of `pending`, if it is one. The line
/// must end with a colon. Each password prompt is matched by its words: the
/// retype prompt contains "new password" too, so it is checked first.
fn answer(master: &mut File, pending: &mut String, current: &str, new: &str) -> Result<(), String> {
    let line = pending
        .rsplit(['\n', '\r'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    if !line.ends_with(':') {
        return Ok(());
    }
    let secret = if line.contains("retype")
        || line.contains("re-enter")
        || (line.contains("new") && line.contains("password"))
    {
        new
    } else if line.contains("password") && (line.contains("current") || line.contains("old")) {
        current
    } else {
        return Ok(());
    };
    pending.clear();
    master
        .write_all(format!("{secret}\n").as_bytes())
        .map_err(|_| "The password could not be sent to the password tool.".to_string())
}

/// The message for a failed change, from the words `passwd` printed.
fn failure(transcript: &str) -> String {
    let lower = transcript.to_lowercase();
    if let Some(reason) = transcript
        .lines()
        .find_map(|line| line.trim().strip_prefix("BAD PASSWORD:"))
    {
        let reason = reason.trim();
        return if reason.is_empty() {
            "The new password was rejected.".into()
        } else {
            reason.to_string()
        };
    }
    if lower.contains("do not match") {
        return "The new passwords do not match.".into();
    }
    if lower.contains("authentication token manipulation error")
        || lower.contains("incorrect password")
        || lower.contains("authentication failure")
    {
        return "The current password is not correct.".into();
    }
    "The password could not be changed.".into()
}
