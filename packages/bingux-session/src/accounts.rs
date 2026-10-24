//! Login accounts read through the system password database.

use crate::snapshot;
use serde::Serialize;
use std::{
    ffi::CStr,
    fs,
    path::{Path, PathBuf},
};

const DEFAULT_UID_MIN: u32 = 1000;
const DEFAULT_UID_MAX: u32 = 60000;
const NOLOGIN_SHELLS: [&str; 4] = [
    "/sbin/nologin",
    "/usr/sbin/nologin",
    "/bin/false",
    "/usr/bin/false",
];
const ACCOUNTS_SERVICE_ICONS: &str = "/var/lib/AccountsService/icons";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    /// Readable avatar path, or empty when the greeter should draw initials.
    pub icon: String,
    /// Readable copy of the account's wallpaper from its greeter snapshot.
    pub wallpaper: String,
}

#[derive(Debug, Serialize)]
pub struct Catalog {
    pub users: Vec<Account>,
}

impl Catalog {
    pub fn read() -> Self {
        let (low, high) = login_uid_range(Path::new("/etc/login.defs"));
        let mut users: Vec<Account> = passwd_entries()
            .into_iter()
            .filter(|entry| (low..=high).contains(&entry.uid))
            .filter(|entry| !NOLOGIN_SHELLS.contains(&entry.shell.as_str()))
            .map(|entry| Account {
                icon: avatar_path(&entry.name)
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                wallpaper: snapshot::read(&snapshot::root(), &entry.name, entry.uid).wallpaper,
                display_name: display_name(&entry.gecos, &entry.name),
                name: entry.name,
            })
            .collect();
        users.sort_by_key(|account| account.display_name.to_lowercase());
        Self { users }
    }
}

struct PasswdEntry {
    name: String,
    uid: u32,
    gecos: String,
    shell: String,
}

/// The login name of the calling user, from the password database.
pub fn current_user_name() -> Option<String> {
    // SAFETY: getpwuid returns a pointer to static storage that is copied
    // before any other passwd call.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() {
            return None;
        }
        Some(
            CStr::from_ptr((*entry).pw_name)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn passwd_entries() -> Vec<PasswdEntry> {
    let mut entries = Vec::new();
    // SAFETY: the passwd enumeration functions are called from one thread and
    // every returned pointer is copied before the next call.
    unsafe {
        libc::setpwent();
        loop {
            let entry = libc::getpwent();
            if entry.is_null() {
                break;
            }
            let entry = &*entry;
            entries.push(PasswdEntry {
                name: CStr::from_ptr(entry.pw_name).to_string_lossy().into_owned(),
                uid: entry.pw_uid,
                gecos: CStr::from_ptr(entry.pw_gecos)
                    .to_string_lossy()
                    .into_owned(),
                shell: CStr::from_ptr(entry.pw_shell)
                    .to_string_lossy()
                    .into_owned(),
            });
        }
        libc::endpwent();
    }
    entries
}

pub fn login_uid_range(login_defs: &Path) -> (u32, u32) {
    let mut range = (DEFAULT_UID_MIN, DEFAULT_UID_MAX);
    let Ok(text) = fs::read_to_string(login_defs) else {
        return range;
    };
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match (
            parts.next(),
            parts.next().and_then(|value| value.parse().ok()),
            parts.next(),
        ) {
            (Some("UID_MIN"), Some(value), None) => range.0 = value,
            (Some("UID_MAX"), Some(value), None) => range.1 = value,
            _ => {}
        }
    }
    range
}

pub fn display_name(gecos: &str, name: &str) -> String {
    let full = gecos.split(',').next().unwrap_or("").trim();
    if full.is_empty() {
        name.to_string()
    } else {
        full.to_string()
    }
}

fn avatar_path(name: &str) -> Option<PathBuf> {
    let path = Path::new(ACCOUNTS_SERVICE_ICONS).join(name);
    (path.is_file() && fs::File::open(&path).is_ok()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gecos_full_name_wins_over_login_name() {
        assert_eq!(display_name("Ada Lovelace,,,", "ada"), "Ada Lovelace");
        assert_eq!(display_name(",,,", "ada"), "ada");
        assert_eq!(display_name("", "ada"), "ada");
    }

    #[test]
    fn login_defs_overrides_the_default_range() {
        let dir = std::env::temp_dir().join(format!("bingux-session-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("login.defs");
        fs::write(
            &path,
            "# comment\nUID_MIN 2000\nUID_MAX\t 5000\nSYS_UID_MIN 201\n",
        )
        .unwrap();
        assert_eq!(login_uid_range(&path), (2000, 5000));
        assert_eq!(
            login_uid_range(&dir.join("missing")),
            (DEFAULT_UID_MIN, DEFAULT_UID_MAX)
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
