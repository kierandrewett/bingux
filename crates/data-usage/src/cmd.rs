//! Running helper programs with a hard deadline.

use std::{
    io::{self, Read},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Output beyond this is cut off; the programs we run print kilobytes.
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;

/// Runs `program` with `args` and returns its stdout. The child is killed
/// when it runs longer than `timeout`; a non-zero exit is an error carrying
/// the first line of stderr. The locale is forced to `C` so parsers see
/// stable text.
pub fn run_capture(program: &str, args: &[&str], timeout: Duration) -> io::Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");
    // Drain both pipes concurrently so a chatty child cannot stall on a
    // full pipe while we wait for it to exit.
    let out_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = (&mut stdout).take(MAX_OUTPUT).read_to_end(&mut buffer);
        buffer
    });
    let err_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = (&mut stderr).take(64 * 1024).read_to_end(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = out_reader.join().unwrap_or_default();
    let errors = err_reader.join().unwrap_or_default();

    match status {
        None => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("{program} did not finish within {timeout:?}"),
        )),
        Some(status) if !status.success() => {
            let message = String::from_utf8_lossy(&errors);
            let first = message.lines().next().unwrap_or("").trim();
            Err(io::Error::other(format!(
                "{program} exited with {status}: {first}"
            )))
        }
        Some(_) => Ok(String::from_utf8_lossy(&output).into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout() {
        let text = run_capture("sh", &["-c", "printf 'a\\nb'"], Duration::from_secs(5)).unwrap();
        assert_eq!(text, "a\nb");
    }

    #[test]
    fn nonzero_exit_is_an_error_with_stderr() {
        let error = run_capture(
            "sh",
            &["-c", "echo boom >&2; exit 3"],
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(error.to_string().contains("boom"), "{error}");
    }

    #[test]
    fn hanging_child_is_killed() {
        let started = Instant::now();
        let error = run_capture("sleep", &["30"], Duration::from_millis(200)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn missing_program_fails_fast() {
        let error =
            run_capture("definitely-not-a-program-xyz", &[], Duration::from_secs(5)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
