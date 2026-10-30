//! Bingux configuration: one declared schema, one TOML file, one validator.
//!
//! The shell, the search daemon and the Settings application all go through
//! this crate (or its `bingux-config` binary), so manual edits to
//! `~/.config/bingux/bingux.toml` and graphical edits share the same rules.

pub mod legacy;
pub mod schema;
pub mod store;
pub mod validate;
pub mod wallpaper;

use std::path::Path;

pub use schema::Config;
pub use store::{
    Loaded, config_dir, config_path, defaults_document, defaults_tree, get_value, import_layout,
    load, save_patch, save_tree_patch, set_value,
};
pub use wallpaper::wallpaper_uri;

/// Every way loading or saving can fail. `Invalid` carries the dotted key
/// path so `bingux-config check` can point at the offending line.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {message}")]
    Invalid { path: String, message: String },
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    #[error("{program}: {message}")]
    Command { program: String, message: String },
}

impl Error {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            message: source.to_string(),
        }
    }

    /// The validation or parse message without the key path, matching the
    /// strings the shell shows to the user.
    pub fn message(&self) -> &str {
        match self {
            Self::Invalid { message, .. }
            | Self::Parse { message, .. }
            | Self::Io { message, .. } => message,
            Self::Command { message, .. } => message,
        }
    }
}

/// The camelCase JSON tree the shell reads, including `null` for absent
/// optional keys.
pub fn legacy_json(loaded: &Loaded) -> serde_json::Value {
    legacy::to_legacy(&loaded.tree)
}

/// The camelCase defaults for the `desktop` section.
pub fn legacy_desktop_defaults() -> serde_json::Value {
    legacy::to_legacy(&defaults_tree())["desktop"].clone()
}
