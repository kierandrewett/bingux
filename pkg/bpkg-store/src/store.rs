use std::fs;
use std::path::{Path, PathBuf};

use bingux_common::error::{BinguxError, Result};
use bingux_common::package_id::PackageId;
use bingux_common::paths::SystemPaths;

use crate::manifest::Manifest;

/// The package store, managing installed packages under a root directory.
pub struct PackageStore {
    root: PathBuf,
}

impl PackageStore {
    /// Create or open a package store at `root`.
    ///
    /// Creates the directory if it does not exist.
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Return the path to a package directory if it exists.
    pub fn get(&self, id: &PackageId) -> Option<PathBuf> {
        let dir = self.root.join(id.dir_name());
        if dir.is_dir() {
            Some(dir)
        } else {
            None
        }
    }

    /// List all installed packages by reading directory names and parsing them.
    pub fn list(&self) -> Vec<PackageId> {
        let mut ids = Vec::new();
        let entries = match fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(_) => return ids,
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Ok(id) = entry.file_name().to_string_lossy().parse::<PackageId>() {
                    ids.push(id);
                }
            }
        }
        ids.sort_by(|a, b| a.dir_name().cmp(&b.dir_name()));
        ids
    }

    /// Install a package from `source_dir` into the store.
    ///
    /// Reads manifest.toml from `source_dir/.bpkg/manifest.toml`, derives
    /// the PackageId, and copies the directory into the store.
    pub fn install(&self, source_dir: &Path) -> Result<PackageId> {
        let manifest = Self::read_manifest_from(source_dir)?;
        let arch = manifest.package.arch.parse()?;
        let id = PackageId::new(&manifest.package.name, &manifest.package.version, arch)?;

        let dest = self.root.join(id.dir_name());
        if dest.exists() {
            return Err(BinguxError::PackageAlreadyExists(id.to_string()));
        }

        copy_dir_recursive(source_dir, &dest)?;

        Ok(id)
    }

    /// Remove an installed package.
    pub fn remove(&self, id: &PackageId) -> Result<()> {
        let dir = self.root.join(id.dir_name());
        if !dir.exists() {
            return Err(BinguxError::PackageNotFound(id.to_string()));
        }
        fs::remove_dir_all(&dir)?;
        Ok(())
    }

    /// Query all installed versions of a package by name.
    pub fn query(&self, name: &str) -> Vec<PackageId> {
        self.list()
            .into_iter()
            .filter(|id| id.name == name)
            .collect()
    }

    /// Read the manifest for an installed package.
    pub fn manifest(&self, id: &PackageId) -> Result<Manifest> {
        let dir = self.root.join(id.dir_name());
        if !dir.is_dir() {
            return Err(BinguxError::PackageNotFound(id.to_string()));
        }
        Self::read_manifest_from(&dir)
    }

    /// Read manifest.toml from a package directory.
    fn read_manifest_from(package_dir: &Path) -> Result<Manifest> {
        let manifest_path = package_dir
            .join(SystemPaths::BPKG_META_DIR)
            .join(SystemPaths::MANIFEST_FILENAME);
        let contents = fs::read_to_string(&manifest_path).map_err(|e| BinguxError::Manifest {
            package: package_dir.display().to_string(),
            message: format!("failed to read manifest: {e}"),
        })?;
        let manifest: Manifest = toml::from_str(&contents).map_err(|e| BinguxError::Manifest {
            package: package_dir.display().to_string(),
            message: format!("failed to parse manifest: {e}"),
        })?;
        Ok(manifest)
    }
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
