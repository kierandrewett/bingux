//! Per-account greeter snapshots.
//!
//! The greeter runs as the greeter account and cannot read home directories,
//! so each user session exports what the login screen needs into
//! `/var/lib/bingux/greeter/<user>/`. The parent is a sticky, world-writable
//! directory from tmpfiles; a snapshot counts only when its directory is owned
//! by the account it names, so one user cannot dress up another's login tile.

use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

pub const SNAPSHOT_ROOT: &str = "/var/lib/bingux/greeter";
const WALLPAPER_FILE: &str = "wallpaper";
const SESSION_FILE: &str = "session.json";
const MAX_WALLPAPER_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Readable copy of the account's wallpaper, or empty.
    #[serde(default)]
    pub wallpaper: String,
}

/// The wallpaper the user's own Bingux configuration resolves to, through the
/// bingux-config CLI so the two never disagree. Empty when bingux-config is
/// absent or nothing is configured.
pub fn configured_wallpaper() -> String {
    let configured = command_stdout(&["get", "desktop.wallpaper"]);
    if !configured.is_empty() && Path::new(&configured).is_absolute() {
        return configured;
    }
    let fallback = command_stdout(&["wallpaper"]);
    serde_json::from_str::<serde_json::Value>(&fallback)
        .ok()
        .and_then(|value| value.get("wallpaper")?.as_str().map(str::to_string))
        .and_then(|uri| uri.strip_prefix("file://").map(str::to_string))
        .filter(|path| Path::new(path).is_absolute())
        .unwrap_or_default()
}

fn command_stdout(arguments: &[&str]) -> String {
    Command::new("bingux-config")
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Writes the calling user's snapshot. `root` is `SNAPSHOT_ROOT` in production.
pub fn export(root: &Path, user: &str, wallpaper: &str) -> io::Result<Snapshot> {
    let directory = root.join(user);
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))?;
    let target = directory.join(WALLPAPER_FILE);
    let mut snapshot = Snapshot::default();
    let source = Path::new(wallpaper);
    if !wallpaper.is_empty()
        && source.is_file()
        && fs::metadata(source)?.len() <= MAX_WALLPAPER_BYTES
    {
        let temporary = directory.join(".wallpaper.tmp");
        fs::copy(source, &temporary)?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))?;
        fs::rename(&temporary, &target)?;
        snapshot.wallpaper = target.display().to_string();
    } else if target.exists() {
        fs::remove_file(&target)?;
    }
    let temporary = directory.join(".session.json.tmp");
    fs::write(&temporary, serde_json::to_string_pretty(&snapshot)? + "\n")?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))?;
    fs::rename(&temporary, directory.join(SESSION_FILE))?;
    Ok(snapshot)
}

/// Reads a snapshot the greeter may trust for `user` with `uid`.
pub fn read(root: &Path, user: &str, uid: u32) -> Snapshot {
    let directory = root.join(user);
    let Ok(metadata) = fs::metadata(&directory) else {
        return Snapshot::default();
    };
    if !metadata.is_dir() || metadata.uid() != uid {
        return Snapshot::default();
    }
    let mut snapshot: Snapshot = fs::read_to_string(directory.join(SESSION_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let wallpaper = directory.join(WALLPAPER_FILE);
    let trusted = snapshot.wallpaper == wallpaper.display().to_string()
        && fs::metadata(&wallpaper).is_ok_and(|meta| meta.is_file() && meta.uid() == uid);
    if !trusted {
        snapshot.wallpaper = String::new();
    }
    snapshot
}

pub fn root() -> PathBuf {
    std::env::var_os("BINGUX_GREETER_SNAPSHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SNAPSHOT_ROOT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_then_read_round_trips_and_rejects_foreign_owners() {
        let root =
            std::env::temp_dir().join(format!("bingux-session-snapshot-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let image = root.join("source.png");
        fs::write(&image, b"not really a png").unwrap();
        // SAFETY: tests in this module run on one thread.
        let uid = unsafe { libc::getuid() };
        let exported = export(&root, "ada", image.to_str().unwrap()).unwrap();
        assert_eq!(
            exported.wallpaper,
            root.join("ada/wallpaper").display().to_string()
        );
        assert_eq!(read(&root, "ada", uid), exported);
        assert_eq!(read(&root, "ada", uid + 1), Snapshot::default());
        assert_eq!(read(&root, "nobody", uid), Snapshot::default());
        let cleared = export(&root, "ada", "").unwrap();
        assert_eq!(cleared.wallpaper, "");
        assert!(!root.join("ada/wallpaper").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
