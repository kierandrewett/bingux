//! Drives made up for captures and tests, so the sidebar can be seen with a
//! USB stick, a disc, a share that does not answer and a full disk without
//! mounting anything. Only built for tests and the `testing` feature.
//!
//! `$BINGUX_FILES_FIXTURE_MOUNTS` names a file with one drive per line:
//!
//! ```text
//! kind | label | mount point | usage [| user] [| ro]
//! removable | KINGSTON | /run/media/me/KINGSTON | 64/12 | user
//! network | nas | /mnt/nas | stuck
//! ```
//!
//! `kind` is local, removable, network, optical or other. `usage` is
//! `TOTAL/FREE` in decimal gigabytes, `unknown`, `pending` or `stuck` (not
//! responding). The drives replace the real ones, and letting go of one
//! waits a few seconds and then fails, so the busy state and the error toast
//! can be seen.

use std::path::PathBuf;
use std::sync::OnceLock;

use bingux_files::mounts::{Mount, MountKind, Usage};

/// The variable that names the fixture file.
pub(crate) const VARIABLE: &str = "BINGUX_FILES_FIXTURE_MOUNTS";

/// The drives of the fixture file, when the variable is set. Read once.
pub(crate) fn mounts() -> Option<&'static Vec<Mount>> {
    static MOUNTS: OnceLock<Option<Vec<Mount>>> = OnceLock::new();
    MOUNTS
        .get_or_init(|| {
            let path = std::env::var_os(VARIABLE)?;
            match std::fs::read_to_string(&path) {
                Ok(text) => Some(parse(&text)),
                Err(error) => {
                    eprintln!("{VARIABLE}: {}: {error}", PathBuf::from(path).display());
                    None
                }
            }
        })
        .as_ref()
}

fn gigabytes(text: &str) -> Option<u64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .map(|value| (value * 1e9) as u64)
}

fn usage(text: &str) -> Usage {
    match text {
        "unknown" => Usage::Unknown,
        "pending" => Usage::Pending,
        "stuck" => Usage::NotResponding,
        other => match other.split_once('/') {
            Some((total, free)) => match (gigabytes(total), gigabytes(free)) {
                (Some(total), Some(free)) => Usage::Known { total, free },
                _ => Usage::Unknown,
            },
            None => Usage::Unknown,
        },
    }
}

/// The drives a fixture file describes. A line that does not parse is skipped.
pub(crate) fn parse(text: &str) -> Vec<Mount> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .enumerate()
        .filter_map(|(index, line)| {
            let fields: Vec<&str> = line.split('|').map(str::trim).collect();
            let [kind, label, point, usage_text, flags @ ..] = fields.as_slice() else {
                return None;
            };
            let kind = match *kind {
                "local" => MountKind::Local,
                "removable" => MountKind::Removable,
                "network" => MountKind::Network,
                "optical" => MountKind::Optical,
                "other" => MountKind::Other,
                _ => return None,
            };
            Some(Mount {
                id: 1000 + index as u32,
                label: (*label).into(),
                device: format!("/dev/fixture{index}"),
                mount_point: (*point).into(),
                fs_type: "fixture".into(),
                kind,
                usage: usage(usage_text),
                read_only: flags.contains(&"ro"),
                mounted_by_user: flags.contains(&"user"),
                device_number: (0, index as u32),
                visible: true,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_become_drives() {
        let drives = parse(
            "# comment\n\
             removable | KINGSTON | /run/media/me/KINGSTON | 64/12.5 | user\n\
             network | nas | /mnt/nas | stuck\n\
             nonsense\n\
             optical | disc | /run/media/me/disc | unknown | ro\n",
        );
        assert_eq!(drives.len(), 3);
        assert_eq!(drives[0].kind, MountKind::Removable);
        assert_eq!(
            drives[0].usage,
            Usage::Known {
                total: 64_000_000_000,
                free: 12_500_000_000
            }
        );
        assert!(drives[0].mounted_by_user);
        assert_eq!(drives[1].usage, Usage::NotResponding);
        assert!(drives[2].read_only);
    }
}
