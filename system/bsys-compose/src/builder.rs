use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tracing::info;

use bingux_common::error::Result;
use bingux_common::paths::SystemPaths;
use bingux_common::BinguxError;


use crate::dispatch::{DispatchEntry, DispatchTable};
use crate::generation::{Generation, GenerationPackage, PackageEntry};

/// Builds, activates, and manages generation directories.
///
/// A generation directory layout:
/// ```text
/// <root>/
///   1/
///     bin/           ← symlinks (or shims) for exported binaries
///     lib/           ← symlinks for exported libraries
///     share/         ← symlinks for exported data files
///     .dispatch.toml ← argv[0] → package + sandbox mapping
///     generation.toml← metadata (id, timestamp, packages, config_hash)
///   2/
///     ...
///   current → 2      ← symlink to active generation
/// ```
pub struct GenerationBuilder {
    /// Root directory for generations (e.g. `/system/profiles/`).
    root: PathBuf,
    /// Root directory for installed packages (e.g. `/system/packages/`).
    packages_root: PathBuf,
}

impl GenerationBuilder {
    /// Create a new builder for the given profiles root.
    ///
    /// `packages_root` is the directory where installed packages live,
    /// used to resolve symlink targets.
    pub fn new(root: PathBuf, packages_root: PathBuf) -> Self {
        Self {
            root,
            packages_root,
        }
    }

    /// Build a new generation from the given package entries.
    ///
    /// Creates the generation directory, populates `bin/`, `lib/`, `share/`
    /// with symlinks, writes `.dispatch.toml` and `generation.toml`.
    pub fn build(&self, packages: &[PackageEntry]) -> Result<Generation> {
        let gen_id = self.next_id()?;
        let gen_dir = self.root.join(gen_id.to_string());
        fs::create_dir_all(&gen_dir)?;

        // Create subdirectories.
        let bin_dir = gen_dir.join("bin");
        let lib_dir = gen_dir.join("lib");
        let share_dir = gen_dir.join("share");
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&lib_dir)?;
        fs::create_dir_all(&share_dir)?;

        let mut dispatch = DispatchTable::new();

        for entry in packages {
            let pkg_dir = self.packages_root.join(entry.package_id.dir_name());

            // Create bin symlinks and dispatch entries.
            for bin_path in &entry.exports.binaries {
                let bin_name = Path::new(bin_path)
                    .file_name()
                    .ok_or_else(|| {
                        BinguxError::Generation(format!("invalid binary path: {bin_path}"))
                    })?
                    .to_string_lossy()
                    .into_owned();

                let target = pkg_dir.join(bin_path);
                let link = bin_dir.join(&bin_name);

                // For sandboxed binaries, in the future this would be a
                // hardlink to bxc-shim.  For now we use symlinks everywhere.
                symlink_or_create_parent(&target, &link)?;

                dispatch.insert(
                    bin_name,
                    DispatchEntry {
                        package: entry.package_id.dir_name(),
                        binary: bin_path.clone(),
                        sandbox: entry.sandbox_level,
                    },
                );
            }

            // Create lib symlinks.
            for lib_path in &entry.exports.libraries {
                let lib_name = Path::new(lib_path)
                    .file_name()
                    .ok_or_else(|| {
                        BinguxError::Generation(format!("invalid library path: {lib_path}"))
                    })?
                    .to_string_lossy()
                    .into_owned();

                let target = pkg_dir.join(lib_path);
                let link = lib_dir.join(&lib_name);
                symlink_or_create_parent(&target, &link)?;
            }

            // Create share symlinks (may need subdirectories).
            for data_path in &entry.exports.data {
                let target = pkg_dir.join(data_path);
                // data_path might be "share/applications/firefox.desktop",
                // strip leading "share/" to get relative path under share_dir.
                let rel = Path::new(data_path)
                    .strip_prefix("share")
                    .unwrap_or(Path::new(data_path));
                let link = share_dir.join(rel);
                if let Some(parent) = link.parent() {
                    fs::create_dir_all(parent)?;
                }
                symlink_or_create_parent(&target, &link)?;
            }
        }

        // Write dispatch table.
        let dispatch_path = gen_dir.join(SystemPaths::DISPATCH_FILENAME);
        dispatch.write_to(&dispatch_path)?;

        // Compute config hash from the serialised dispatch table (deterministic).
        let config_hash = {
            let dispatch_content = dispatch.to_toml()?;
            let hash = Sha256::digest(dispatch_content.as_bytes());
            format!("sha256:{}", hex_encode(&hash))
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let generation = Generation {
            id: gen_id,
            timestamp,
            packages: packages
                .iter()
                .map(|e| GenerationPackage {
                    id: e.package_id.dir_name(),
                    sandbox: e.sandbox_level,
                })
                .collect(),
            config_hash,
        };

        // Write generation.toml.
        let gen_toml = toml::to_string_pretty(&generation)?;
        fs::write(gen_dir.join("generation.toml"), gen_toml)?;

        info!(id = gen_id, "built generation");

        Ok(generation)
    }

    /// Atomically activate a generation by updating the `current` symlink.
    pub fn activate(&self, generation_id: u64) -> Result<()> {
        let gen_dir = self.root.join(generation_id.to_string());
        if !gen_dir.exists() {
            return Err(BinguxError::Generation(format!(
                "generation {generation_id} does not exist"
            )));
        }

        let current = self.root.join("current");
        let tmp_link = self.root.join(".current.tmp");

        // Remove stale temp link if it exists.
        let _ = fs::remove_file(&tmp_link);

        // Create temp symlink then atomically rename.
        std::os::unix::fs::symlink(generation_id.to_string(), &tmp_link)?;
        fs::rename(&tmp_link, &current)?;

        info!(id = generation_id, "activated generation");
        Ok(())
    }

    /// List all generations (sorted by ID).
    pub fn list_generations(&self) -> Result<Vec<Generation>> {
        let mut generations = Vec::new();

        if !self.root.exists() {
            return Ok(generations);
        }

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip non-numeric directories (e.g. "current" symlink).
            if name_str.parse::<u64>().is_err() {
                continue;
            }

            let gen_toml_path = entry.path().join("generation.toml");
            if gen_toml_path.exists() {
                let content = fs::read_to_string(&gen_toml_path)?;
                let generation: Generation = toml::from_str(&content)?;
                generations.push(generation);
            }
        }

        generations.sort_by_key(|g| g.id);
        Ok(generations)
    }

    /// Get the currently active generation ID, or `None` if no generation
    /// has been activated.
    pub fn current(&self) -> Result<Option<u64>> {
        let current = self.root.join("current");
        if !current.exists() {
            return Ok(None);
        }

        let target = fs::read_link(&current)?;
        let id_str = target
            .file_name()
            .ok_or_else(|| BinguxError::Generation("invalid current symlink".into()))?
            .to_string_lossy();

        let id: u64 = id_str.parse().map_err(|_| {
            BinguxError::Generation(format!("current symlink points to non-numeric: {id_str}"))
        })?;

        Ok(Some(id))
    }

    /// Roll back to a previous generation by reactivating it.
    pub fn rollback(&self, generation_id: u64) -> Result<()> {
        info!(id = generation_id, "rolling back to generation");
        self.activate(generation_id)
    }

    /// Determine the next generation ID (max existing + 1, or 1).
    fn next_id(&self) -> Result<u64> {
        let mut max_id = 0u64;

        if self.root.exists() {
            for entry in fs::read_dir(&self.root)? {
                let entry = entry?;
                let name = entry.file_name();
                if let Ok(id) = name.to_string_lossy().parse::<u64>() {
                    max_id = max_id.max(id);
                }
            }
        } else {
            fs::create_dir_all(&self.root)?;
        }

        Ok(max_id + 1)
    }
}

/// Create a symlink, ensuring the parent directory exists.
fn symlink_or_create_parent(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

/// Simple hex encoding (avoids pulling in the `hex` crate).
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_common::package_id::Arch;
    use bingux_common::PackageId;
    use bxc_sandbox::SandboxLevel;
    use crate::generation::ExportedItems;
    use tempfile::TempDir;

    /// Helper: set up a temp dir with fake packages and a profiles root.
    fn setup() -> (TempDir, PathBuf, PathBuf, Vec<PackageEntry>) {
        let tmp = TempDir::new().unwrap();
        let profiles = tmp.path().join("profiles");
        let packages = tmp.path().join("packages");

        // Create fake package directories with binaries, libs, and data.
        let firefox_dir = packages.join("firefox-129.0-x86_64-linux");
        fs::create_dir_all(firefox_dir.join("bin")).unwrap();
        fs::create_dir_all(firefox_dir.join("lib")).unwrap();
        fs::create_dir_all(firefox_dir.join("share/applications")).unwrap();
        fs::write(firefox_dir.join("bin/firefox"), "#!/bin/sh\n").unwrap();
        fs::write(firefox_dir.join("lib/libxul.so"), "").unwrap();
        fs::write(
            firefox_dir.join("share/applications/firefox.desktop"),
            "[Desktop Entry]\n",
        )
        .unwrap();

        let bash_dir = packages.join("bash-5.2-x86_64-linux");
        fs::create_dir_all(bash_dir.join("bin")).unwrap();
        fs::write(bash_dir.join("bin/bash"), "#!/bin/sh\n").unwrap();

        let entries = vec![
            PackageEntry {
                package_id: PackageId::new("firefox", "129.0", Arch::X86_64Linux).unwrap(),
                sandbox_level: SandboxLevel::Standard,
                exports: ExportedItems {
                    binaries: vec!["bin/firefox".into()],
                    libraries: vec!["lib/libxul.so".into()],
                    data: vec!["share/applications/firefox.desktop".into()],
                },
            },
            PackageEntry {
                package_id: PackageId::new("bash", "5.2", Arch::X86_64Linux).unwrap(),
                sandbox_level: SandboxLevel::Minimal,
                exports: ExportedItems {
                    binaries: vec!["bin/bash".into()],
                    libraries: vec![],
                    data: vec![],
                },
            },
        ];

        (tmp, profiles, packages, entries)
    }

    #[test]
    fn build_generation_creates_symlinks() {
        let (_tmp, profiles, packages, entries) = setup();
        let builder = GenerationBuilder::new(profiles.clone(), packages.clone());

        let built = builder.build(&entries).unwrap();
        assert_eq!(built.id, 1);
        assert_eq!(built.packages.len(), 2);

        let gen_dir = profiles.join("1");

        // Verify bin symlinks.
        assert!(gen_dir.join("bin/firefox").is_symlink());
        assert!(gen_dir.join("bin/bash").is_symlink());

        // Verify lib symlink.
        assert!(gen_dir.join("lib/libxul.so").is_symlink());

        // Verify share symlink.
        assert!(gen_dir.join("share/applications/firefox.desktop").is_symlink());

        // Verify symlink targets point into packages dir.
        let firefox_target = fs::read_link(gen_dir.join("bin/firefox")).unwrap();
        assert_eq!(
            firefox_target,
            packages.join("firefox-129.0-x86_64-linux/bin/firefox")
        );
    }

    #[test]
    fn dispatch_toml_written_correctly() {
        let (_tmp, profiles, packages, entries) = setup();
        let builder = GenerationBuilder::new(profiles.clone(), packages);

        builder.build(&entries).unwrap();

        let dispatch_path = profiles.join("1/.dispatch.toml");
        assert!(dispatch_path.exists());

        let table = DispatchTable::read_from(&dispatch_path).unwrap();
        assert_eq!(table.entries.len(), 2);

        let ff = table.get("firefox").unwrap();
        assert_eq!(ff.package, "firefox-129.0-x86_64-linux");
        assert_eq!(ff.binary, "bin/firefox");
        assert_eq!(ff.sandbox, SandboxLevel::Standard);

        let bash = table.get("bash").unwrap();
        assert_eq!(bash.sandbox, SandboxLevel::Minimal);
    }

    #[test]
    fn activate_swaps_current_symlink() {
        let (_tmp, profiles, packages, entries) = setup();
        let builder = GenerationBuilder::new(profiles.clone(), packages);

        let built = builder.build(&entries).unwrap();
        builder.activate(built.id).unwrap();

        let current = profiles.join("current");
        assert!(current.is_symlink());
        let target = fs::read_link(&current).unwrap();
        assert_eq!(target.to_string_lossy(), "1");

        assert_eq!(builder.current().unwrap(), Some(1));
    }

    #[test]
    fn list_generations_returns_all() {
        let (_tmp, profiles, packages, entries) = setup();
        let builder = GenerationBuilder::new(profiles.clone(), packages);

        builder.build(&entries).unwrap();
        builder.build(&entries).unwrap();
        builder.build(&entries).unwrap();

        let gens = builder.list_generations().unwrap();
        assert_eq!(gens.len(), 3);
        assert_eq!(gens[0].id, 1);
        assert_eq!(gens[1].id, 2);
        assert_eq!(gens[2].id, 3);
    }

    #[test]
    fn rollback_points_current_to_older_generation() {
        let (_tmp, profiles, packages, entries) = setup();
        let builder = GenerationBuilder::new(profiles.clone(), packages);

        let gen1 = builder.build(&entries).unwrap();
        let gen2 = builder.build(&entries).unwrap();

        builder.activate(gen2.id).unwrap();
        assert_eq!(builder.current().unwrap(), Some(2));

        builder.rollback(gen1.id).unwrap();
        assert_eq!(builder.current().unwrap(), Some(1));
    }

    #[test]
    fn current_returns_none_when_no_activation() {
        let (_tmp, profiles, packages, entries) = setup();
        let builder = GenerationBuilder::new(profiles.clone(), packages);

        builder.build(&entries).unwrap();

        assert_eq!(builder.current().unwrap(), None);
    }

    #[test]
    fn generation_metadata_written() {
        let (_tmp, profiles, packages, entries) = setup();
        let builder = GenerationBuilder::new(profiles.clone(), packages);

        let built = builder.build(&entries).unwrap();

        let meta_path = profiles.join("1/generation.toml");
        assert!(meta_path.exists());

        let content = fs::read_to_string(&meta_path).unwrap();
        let parsed: Generation = toml::from_str(&content).unwrap();
        assert_eq!(parsed.id, built.id);
        assert_eq!(parsed.packages.len(), 2);
        assert!(parsed.config_hash.starts_with("sha256:"));
    }
}
