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

/// Recursively copy a directory tree, handling symlinks.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_symlink() {
            // Copy symlinks as-is (even if broken)
            if let Ok(target) = fs::read_link(&src_path) {
                // Remove existing if present
                let _ = fs::remove_file(&dst_path);
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &dst_path)?;
                #[cfg(not(unix))]
                fs::copy(&src_path, &dst_path).ok();
            }
        } else if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_common::package_id::Arch;
    use tempfile::TempDir;

    /// Create a fake package source directory with a manifest and some files.
    fn make_source_pkg(
        tmp: &Path,
        name: &str,
        version: &str,
        arch: &str,
    ) -> PathBuf {
        let src = tmp.join(format!("src-{name}-{version}"));
        let meta = src.join(".bpkg");
        fs::create_dir_all(&meta).unwrap();

        let manifest = format!(
            r#"[package]
name = "{name}"
version = "{version}"
arch = "{arch}"
description = "Test package"
"#
        );
        fs::write(meta.join("manifest.toml"), manifest).unwrap();

        // Create some fake content
        let bin_dir = src.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join(name), "#!/bin/sh\necho hello\n").unwrap();

        src
    }

    #[test]
    fn install_and_get() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();

        let src = make_source_pkg(tmp.path(), "hello", "1.0", "x86_64-linux");
        let id = store.install(&src).unwrap();

        assert_eq!(id.name, "hello");
        assert_eq!(id.version, "1.0");

        let pkg_path = store.get(&id);
        assert!(pkg_path.is_some());
        assert!(pkg_path.unwrap().join("bin/hello").exists());
    }

    #[test]
    fn install_appears_in_list() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();

        let src = make_source_pkg(tmp.path(), "hello", "1.0", "x86_64-linux");
        let id = store.install(&src).unwrap();

        let listed = store.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], id);
    }

    #[test]
    fn manifest_readable_after_install() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();

        let src = make_source_pkg(tmp.path(), "hello", "1.0", "x86_64-linux");
        let id = store.install(&src).unwrap();

        let m = store.manifest(&id).unwrap();
        assert_eq!(m.package.name, "hello");
        assert_eq!(m.package.version, "1.0");
    }

    #[test]
    fn remove_package() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();

        let src = make_source_pkg(tmp.path(), "hello", "1.0", "x86_64-linux");
        let id = store.install(&src).unwrap();

        store.remove(&id).unwrap();
        assert!(store.get(&id).is_none());
        assert!(store.list().is_empty());
    }

    #[test]
    fn remove_nonexistent_fails() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();
        let id = PackageId::new("ghost", "1.0", Arch::X86_64Linux).unwrap();
        assert!(store.remove(&id).is_err());
    }

    #[test]
    fn query_multiple_versions() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();

        let src1 = make_source_pkg(tmp.path(), "hello", "1.0", "x86_64-linux");
        let src2 = make_source_pkg(tmp.path(), "hello", "2.0", "x86_64-linux");
        let src3 = make_source_pkg(tmp.path(), "world", "1.0", "x86_64-linux");

        store.install(&src1).unwrap();
        store.install(&src2).unwrap();
        store.install(&src3).unwrap();

        let results = store.query("hello");
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|id| id.name == "hello"));

        let results = store.query("world");
        assert_eq!(results.len(), 1);

        let results = store.query("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn reject_duplicate_install() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();

        let src = make_source_pkg(tmp.path(), "hello", "1.0", "x86_64-linux");
        store.install(&src).unwrap();

        // Second install of same package should fail
        let result = store.install(&src);
        assert!(result.is_err());
        match result.unwrap_err() {
            BinguxError::PackageAlreadyExists(_) => {}
            e => panic!("expected PackageAlreadyExists, got: {e}"),
        }
    }

    #[test]
    fn list_empty_store() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let tmp = TempDir::new().unwrap();
        let store = PackageStore::new(tmp.path().join("store")).unwrap();
        let id = PackageId::new("nope", "1.0", Arch::X86_64Linux).unwrap();
        assert!(store.get(&id).is_none());
    }
}
