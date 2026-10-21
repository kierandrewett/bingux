//! The permission store file.
//!
//! The file is `$XDG_CONFIG_HOME/bingux/portal-permissions.toml`, or
//! `~/.config/bingux/portal-permissions.toml` when `XDG_CONFIG_HOME` is unset.
//! A missing file means no rules. A file that cannot be read or validated
//! denies every request until it is fixed.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::policy::{Capability, Level, Rule};

pub const STORE_VERSION: u32 = 1;
pub const STORE_FILE_NAME: &str = "portal-permissions.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    pub version: u32,
    #[serde(default, skip_serializing_if = "is_inherit")]
    pub default: Level,
    #[serde(default, rename = "rule", skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
}

fn is_inherit(level: &Level) -> bool {
    *level == Level::Inherit
}

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Parse(toml::de::Error),
    Serialize(toml::ser::Error),
    Invalid(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(error) => write!(f, "cannot access the permission store: {error}"),
            StoreError::Parse(error) => write!(f, "cannot parse the permission store: {error}"),
            StoreError::Serialize(error) => write!(f, "cannot write the permission store: {error}"),
            StoreError::Invalid(message) => write!(f, "invalid permission store: {message}"),
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        StoreError::Io(error)
    }
}

impl From<toml::de::Error> for StoreError {
    fn from(error: toml::de::Error) -> Self {
        StoreError::Parse(error)
    }
}

impl From<toml::ser::Error> for StoreError {
    fn from(error: toml::ser::Error) -> Self {
        StoreError::Serialize(error)
    }
}

impl Store {
    /// The store used when no file exists.
    pub fn empty() -> Self {
        Store {
            version: STORE_VERSION,
            default: Level::Inherit,
            rules: Vec::new(),
        }
    }

    /// The store used when the file exists but cannot be used. Every request
    /// is denied.
    pub fn deny_all() -> Self {
        Store {
            version: STORE_VERSION,
            default: Level::Deny,
            rules: Vec::new(),
        }
    }

    /// Checks the rules against the schema.
    pub fn validate(&self) -> Result<(), StoreError> {
        let invalid = |message: String| Err(StoreError::Invalid(message));
        if self.version != STORE_VERSION {
            return invalid(format!("unsupported version {}", self.version));
        }
        if self.default == Level::Allow {
            return invalid("the top-level default cannot be allow".to_owned());
        }
        let mut names = std::collections::HashSet::new();
        for rule in &self.rules {
            if rule.name.is_empty() {
                return invalid("a rule has an empty name".to_owned());
            }
            if !names.insert(rule.name.as_str()) {
                return invalid(format!("duplicate rule name {}", rule.name));
            }
            validate_rule(rule)?;
        }
        Ok(())
    }
}

fn validate_rule(rule: &Rule) -> Result<(), StoreError> {
    let invalid = |message: String| {
        Err(StoreError::Invalid(format!(
            "rule {}: {message}",
            rule.name
        )))
    };
    if rule.capabilities.is_empty() {
        return invalid("no capabilities".to_owned());
    }
    if rule.app_id.is_none() && rule.exe.is_none() && rule.pattern.is_none() {
        return invalid("no app_id, exe or pattern selector".to_owned());
    }
    if rule.app_id.as_deref() == Some("") {
        return invalid("empty app_id".to_owned());
    }
    if let Some(exe) = &rule.exe
        && !Path::new(exe).is_absolute()
    {
        return invalid("exe must be an absolute path".to_owned());
    }
    if rule.pattern.as_deref() == Some("") {
        return invalid("empty pattern".to_owned());
    }
    if !rule.monitors.is_empty() && !rule.capabilities.iter().any(|c| c.supports_monitors()) {
        return invalid("monitors apply only to screen-cast or remote-desktop".to_owned());
    }
    if rule.monitors.iter().any(|m| m.is_empty()) {
        return invalid("empty monitor name".to_owned());
    }
    let remote = rule.capabilities.contains(&Capability::RemoteDesktop);
    if !rule.devices.is_empty() && !remote {
        return invalid("devices apply only to remote-desktop".to_owned());
    }
    if rule.clipboard && !remote {
        return invalid("clipboard applies only to remote-desktop".to_owned());
    }
    Ok(())
}

/// Returns the store path from the environment.
pub fn default_path() -> PathBuf {
    default_path_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn default_path_from(xdg_config_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    let config_home = match xdg_config_home.filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(home.unwrap_or_default()).join(".config"),
    };
    config_home.join("bingux").join(STORE_FILE_NAME)
}

/// Parses and validates the store text.
pub fn parse(text: &str) -> Result<Store, StoreError> {
    let store: Store = toml::from_str(text)?;
    store.validate()?;
    Ok(store)
}

/// Serialises a store to TOML.
pub fn to_toml(store: &Store) -> Result<String, StoreError> {
    store.validate()?;
    Ok(toml::to_string(store)?)
}

/// Loads the store at `path`. A missing file gives an empty store. Other read
/// or validation errors are returned.
pub fn load(path: &Path) -> Result<Store, StoreError> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Store::empty()),
        Err(error) => Err(StoreError::Io(error)),
    }
}

/// Loads the store for a request. Any error denies every request.
pub fn load_effective(path: &Path) -> Store {
    match load(path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("xdg-desktop-portal-bingux: {error}; denying portal requests");
            Store::deny_all()
        }
    }
}

/// Writes the store to `path` atomically. The file is private to the user.
pub fn save(path: &Path, store: &Store) -> Result<(), StoreError> {
    let text = to_toml(store)?;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let (temp, mut file) = create_temp(dir, &file_name)?;
    let result = (|| -> io::Result<()> {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        std::fs::File::open(dir)?.sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(StoreError::from)
}

/// Per-process counter that gives each temp file its own name.
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const TEMP_ATTEMPTS: usize = 8;

/// Creates a new temp file beside the store. `create_new` never reuses a leftover
/// file or follows a symlink planted at the name, so no stale file is truncated.
fn create_temp(dir: &Path, file_name: &str) -> io::Result<(PathBuf, std::fs::File)> {
    let mut last_error = None;
    for _ in 0..TEMP_ATTEMPTS {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = temp_path(dir, file_name, sequence);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
        {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "no free temp name")))
}

fn temp_path(dir: &Path, file_name: &str, sequence: u64) -> PathBuf {
    dir.join(format!(
        ".{file_name}.{}.{sequence}.tmp",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
version = 1
default = "ask"

[[rule]]
name = "allow-viewer"
app_id = "org.example.Viewer"
capabilities = ["screenshot"]
level = "allow"

[[rule]]
name = "deny-tool"
exe = "/usr/bin/tool"
capabilities = ["screen-cast", "remote-desktop"]
level = "deny"
monitors = ["DP-1"]
devices = ["keyboard"]
clipboard = true
"#;

    #[test]
    fn parses_a_valid_store() {
        let store = parse(SAMPLE).unwrap();
        assert_eq!(store.default, Level::Ask);
        assert_eq!(store.rules.len(), 2);
        assert_eq!(store.rules[1].monitors, vec!["DP-1".to_owned()]);
        assert!(store.rules[1].clipboard);
    }

    #[test]
    fn round_trips_through_toml() {
        let store = parse(SAMPLE).unwrap();
        let text = to_toml(&store).unwrap();
        assert_eq!(parse(&text).unwrap(), store);
    }

    #[test]
    fn rejects_schema_violations() {
        let bad = [
            "default = \"ask\"",
            "version = 2",
            "version = 1\ndefault = \"allow\"",
            "version = 1\nunknown = true",
            "version = 1\n[[rule]]\nname = \"a\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"",
            "version = 1\n[[rule]]\nname = \"a\"\napp_id = \"x\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"\nmonitors = [\"DP-1\"]",
            "version = 1\n[[rule]]\nname = \"a\"\napp_id = \"x\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"\ndevices = [\"keyboard\"]",
            "version = 1\n[[rule]]\nname = \"a\"\napp_id = \"x\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"\nclipboard = true",
            "version = 1\n[[rule]]\nname = \"a\"\nexe = \"relative\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"",
            "version = 1\n[[rule]]\nname = \"a\"\napp_id = \"x\"\ncapabilities = [\"nope\"]\nlevel = \"allow\"",
            "version = 1\n[[rule]]\nname = \"a\"\napp_id = \"x\"\ncapabilities = [\"screenshot\"]\nlevel = \"maybe\"",
            "version = 1\n[[rule]]\nname = \"a\"\napp_id = \"x\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"\n[[rule]]\nname = \"a\"\napp_id = \"y\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"",
            "version = 1\n[[rule]]\nname = \"a\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"\napp_id = \"x\"\nbogus = 1",
        ];
        for text in bad {
            assert!(parse(text).is_err(), "accepted: {text}");
        }
    }

    #[test]
    fn default_path_follows_xdg_then_home() {
        assert_eq!(
            default_path_from(Some("/cfg".into()), Some("/home/u".into())),
            PathBuf::from("/cfg/bingux/portal-permissions.toml")
        );
        assert_eq!(
            default_path_from(Some("".into()), Some("/home/u".into())),
            PathBuf::from("/home/u/.config/bingux/portal-permissions.toml")
        );
    }

    #[test]
    fn missing_file_is_empty_and_bad_file_denies() {
        let dir = tempdir();
        let missing = dir.join("missing.toml");
        assert_eq!(load(&missing).unwrap(), Store::empty());
        assert_eq!(load_effective(&missing), Store::empty());

        let bad = dir.join("bad.toml");
        std::fs::write(&bad, "version = 1\ndefault = \"allow\"").unwrap();
        assert!(load(&bad).is_err());
        assert_eq!(load_effective(&bad), Store::deny_all());
    }

    #[test]
    fn save_is_atomic_and_private() {
        let dir = tempdir();
        let path = dir.join("bingux").join("portal-permissions.toml");
        let store = parse(SAMPLE).unwrap();
        save(&path, &store).unwrap();
        assert_eq!(load(&path).unwrap(), store);
        let mode = std::fs::metadata(&path).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(mode.mode() & 0o777, 0o600);
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn temp_files_never_follow_or_reuse_existing_paths() {
        let dir = tempdir();
        let next = TEMP_SEQUENCE.load(Ordering::Relaxed);
        let target = dir.join("target");
        std::fs::write(&target, "keep").unwrap();
        let planted = temp_path(&dir, "portal-permissions.toml", next);
        std::os::unix::fs::symlink(&target, &planted).unwrap();
        let (temp, _file) = create_temp(&dir, "portal-permissions.toml").unwrap();
        assert_ne!(temp, planted);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bingux-portal-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
