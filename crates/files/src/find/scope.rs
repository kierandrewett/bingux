//! Where a search looks.

use std::path::{Path, PathBuf};

/// The part of the computer a search covers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// The folder on show and every folder below it.
    #[default]
    Subfolders,
    /// The home folder and everything in it.
    Home,
    /// The home folder and every drive mounted for the person. System
    /// folders (`/usr`, `/etc`, `/var`) are not searched.
    Everywhere,
    /// One folder and everything below it: Pictures, a bookmark.
    Place(PathBuf),
}

impl Scope {
    /// The folders to read: `here` is the folder on show, `home` the home
    /// folder, `drives` the mount points of the drives to include.
    pub fn roots(&self, here: &Path, home: &Path, drives: &[PathBuf]) -> Vec<PathBuf> {
        match self {
            Scope::Subfolders => vec![here.to_path_buf()],
            Scope::Home => vec![home.to_path_buf()],
            Scope::Place(path) => vec![path.clone()],
            Scope::Everywhere => {
                let mut roots = vec![home.to_path_buf()];
                for drive in drives {
                    if !drive.starts_with(home) && !roots.contains(drive) {
                        roots.push(drive.clone());
                    }
                }
                roots
            }
        }
    }

    /// The folder results are shown relative to: where the search starts.
    pub fn base(&self, here: &Path, home: &Path) -> PathBuf {
        match self {
            Scope::Subfolders => here.to_path_buf(),
            Scope::Home | Scope::Everywhere => home.to_path_buf(),
            Scope::Place(path) => path.clone(),
        }
    }
}
