//! Resolve the GNOME background setting to an image the shell can load.
//!
//! Formats that Qt cannot decode are converted once into a cached PNG,
//! keyed by path, size and modification time.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::Error;

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("bingux/wallpaper")
}

fn file_uri(path: &Path) -> String {
    url::Url::from_file_path(path)
        .map(|u| u.to_string())
        .unwrap_or_default()
}

fn run_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<std::process::Output, Error> {
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::Command {
            program: command.get_program().to_string_lossy().into_owned(),
            message: source.to_string(),
        })?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Command {
                    program: command.get_program().to_string_lossy().into_owned(),
                    message: "timed out".into(),
                });
            }
            Err(source) => {
                return Err(Error::Command {
                    program: command.get_program().to_string_lossy().into_owned(),
                    message: source.to_string(),
                });
            }
        }
    }
    child.wait_with_output().map_err(|source| Error::Command {
        program: command.get_program().to_string_lossy().into_owned(),
        message: source.to_string(),
    })
}

/// A `file://` URI for the configured GNOME wallpaper, or an empty string
/// when none is usable.
pub fn wallpaper_uri() -> Result<String, Error> {
    for key in ["picture-uri-dark", "picture-uri"] {
        let mut command = Command::new("gsettings");
        command.args(["get", "org.gnome.desktop.background", key]);
        let Ok(output) = run_with_timeout(command, Duration::from_secs(3)) else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let uri = text.trim().trim_matches('\'');
        let Ok(parsed) = url::Url::parse(uri) else {
            continue;
        };
        if parsed.scheme() != "file" {
            continue;
        }
        let Ok(source) = parsed.to_file_path() else {
            continue;
        };
        if !source.is_file() {
            continue;
        }
        let extension = source
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if ["png", "jpg", "jpeg", "webp"].contains(&extension.as_str()) {
            return Ok(file_uri(&source));
        }
        return convert(&source);
    }
    Ok(String::new())
}

fn convert(source: &Path) -> Result<String, Error> {
    let cache = cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| Error::io(&cache, e))?;
    let metadata = std::fs::metadata(source).map_err(|e| Error::io(source, e))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let fingerprint = fingerprint(&format!(
        "{}:{}:{}",
        source.display(),
        modified,
        metadata.len()
    ));
    let output = cache.join(format!("{fingerprint}.png"));
    if output.exists() {
        return Ok(file_uri(&output));
    }
    let temporary = cache.join(format!(".{fingerprint}.{}.png", std::process::id()));
    let mut command = Command::new("magick");
    command
        .args(["-limit", "memory", "256MiB", "-limit", "map", "512MiB"])
        .arg(format!("{}[0]", source.display()))
        .args(["-resize", "2560x1440>"])
        .arg(&temporary);
    let result = run_with_timeout(command, Duration::from_secs(12));
    match result {
        Ok(out) if out.status.success() => {
            std::fs::rename(&temporary, &output).map_err(|e| Error::io(&output, e))?;
            Ok(file_uri(&output))
        }
        Ok(out) => {
            let _ = std::fs::remove_file(&temporary);
            Err(Error::Command {
                program: "magick".into(),
                message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            })
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// FNV-1a over the key material; collisions only cost a stale cache entry.
fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
