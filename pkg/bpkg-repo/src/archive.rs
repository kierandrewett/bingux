use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use bzip2::Compression;
use sha2::{Digest, Sha256};
use tar::{Archive, Builder};

use bingux_common::paths::SystemPaths;
use bpkg_store::Manifest;

use crate::bgx_info::BgxInfo;
use crate::error::RepoError;

/// Create a `.bgx` archive from a package directory.
///
/// The archive contains all files in `package_dir`, including the `.bpkg/`
/// metadata directory. The top-level entry in the tarball is the directory
/// name itself (i.e. the archive unpacks to a single directory).
pub fn create_bgx(package_dir: &Path, output_path: &Path) -> Result<(), RepoError> {
    if !package_dir.is_dir() {
        return Err(RepoError::ArchiveCreate(format!(
            "package directory does not exist: {}",
            package_dir.display()
        )));
    }

    // Verify manifest exists
    let manifest_path = package_dir
        .join(SystemPaths::BPKG_META_DIR)
        .join(SystemPaths::MANIFEST_FILENAME);
    if !manifest_path.exists() {
        return Err(RepoError::ArchiveCreate(
            "package directory is missing .bpkg/manifest.toml".into(),
        ));
    }

    let file = File::create(output_path).map_err(|e| {
        RepoError::ArchiveCreate(format!("failed to create output file: {e}"))
    })?;

    let encoder = BzEncoder::new(file, Compression::best());
    let mut builder = Builder::new(encoder);

    // Determine the base name for the archive entry
    let dir_name = package_dir
        .file_name()
        .ok_or_else(|| RepoError::ArchiveCreate("invalid package directory path".into()))?;

    builder
        .append_dir_all(dir_name, package_dir)
        .map_err(|e| RepoError::ArchiveCreate(format!("failed to append directory: {e}")))?;

    let encoder = builder
        .into_inner()
        .map_err(|e| RepoError::ArchiveCreate(format!("failed to finish tar: {e}")))?;

    encoder
        .finish()
        .map_err(|e| RepoError::ArchiveCreate(format!("failed to finish bzip2: {e}")))?;

    Ok(())
}

/// Extract a `.bgx` archive to a target directory.
///
/// Returns the package ID string parsed from the extracted manifest.
pub fn extract_bgx(bgx_path: &Path, target_dir: &Path) -> Result<String, RepoError> {
    let file = File::open(bgx_path).map_err(|e| {
        RepoError::ArchiveExtract(format!(
            "failed to open {}: {e}",
            bgx_path.display()
        ))
    })?;

    let decoder = BzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    archive.unpack(target_dir).map_err(|e| {
        RepoError::ArchiveExtract(format!("failed to extract archive: {e}"))
    })?;

    // Find the extracted package directory by looking for a manifest
    let extracted_dir = find_extracted_package_dir(target_dir)?;
    let manifest = read_manifest(&extracted_dir)?;

    let package_id = format!(
        "{}-{}-{}",
        manifest.package.name, manifest.package.version, manifest.package.arch
    );

    Ok(package_id)
}

/// Verify a `.bgx` archive without fully extracting it to a permanent location.
///
/// Extracts to a temp directory, reads the manifest, and optionally checks
/// file integrity if `files.txt` is present.
pub fn verify_bgx(bgx_path: &Path) -> Result<BgxInfo, RepoError> {
    let file = File::open(bgx_path).map_err(|e| {
        RepoError::ArchiveVerify(format!(
            "failed to open {}: {e}",
            bgx_path.display()
        ))
    })?;

    let archive_size = file.metadata().map(|m| m.len()).unwrap_or(0);

    let decoder = BzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    let tmp = tempfile::tempdir().map_err(|e| {
        RepoError::ArchiveVerify(format!("failed to create temp directory: {e}"))
    })?;

    archive.unpack(tmp.path()).map_err(|e| {
        RepoError::ArchiveVerify(format!("failed to extract archive for verification: {e}"))
    })?;

    let extracted_dir = find_extracted_package_dir(tmp.path()).map_err(|e| {
        RepoError::ArchiveVerify(e.to_string())
    })?;

    let manifest = read_manifest(&extracted_dir).map_err(|e| {
        RepoError::ArchiveVerify(e.to_string())
    })?;

    // Check files.txt integrity if present
    let files_path = extracted_dir
        .join(SystemPaths::BPKG_META_DIR)
        .join(SystemPaths::FILES_FILENAME);
    if files_path.exists() {
        verify_file_hashes(&extracted_dir, &files_path)?;
    }

    Ok(BgxInfo {
        package_id: format!(
            "{}-{}-{}",
            manifest.package.name, manifest.package.version, manifest.package.arch
        ),
        name: manifest.package.name,
        version: manifest.package.version,
        arch: manifest.package.arch,
        scope: manifest.package.scope,
        description: manifest.package.description,
        size: archive_size,
    })
}

/// Compute the hex-encoded SHA-256 of a file.
pub fn sha256_file(path: &Path) -> Result<String, RepoError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Find the single extracted package directory inside a target dir.
fn find_extracted_package_dir(target_dir: &Path) -> Result<std::path::PathBuf, RepoError> {
    let mut found = None;

    for entry in fs::read_dir(target_dir)? {
        let entry = entry?;
        if entry.path().is_dir() {
            let manifest_path = entry
                .path()
                .join(SystemPaths::BPKG_META_DIR)
                .join(SystemPaths::MANIFEST_FILENAME);
            if manifest_path.exists() {
                if found.is_some() {
                    return Err(RepoError::ArchiveExtract(
                        "archive contains multiple package directories".into(),
                    ));
                }
                found = Some(entry.path());
            }
        }
    }

    found.ok_or_else(|| {
        RepoError::ArchiveExtract(
            "archive does not contain a package directory with .bpkg/manifest.toml".into(),
        )
    })
}

/// Read a Manifest from a package directory.
fn read_manifest(package_dir: &Path) -> Result<Manifest, RepoError> {
    let manifest_path = package_dir
        .join(SystemPaths::BPKG_META_DIR)
        .join(SystemPaths::MANIFEST_FILENAME);

    let contents = fs::read_to_string(&manifest_path).map_err(|e| {
        RepoError::ArchiveExtract(format!(
            "failed to read manifest at {}: {e}",
            manifest_path.display()
        ))
    })?;

    let manifest: Manifest = toml::from_str(&contents).map_err(|e| {
        RepoError::ArchiveExtract(format!("failed to parse manifest: {e}"))
    })?;

    Ok(manifest)
}

/// Verify file hashes from a files.txt against actual files on disk.
fn verify_file_hashes(
    package_dir: &Path,
    files_path: &Path,
) -> Result<(), RepoError> {
    let contents = fs::read_to_string(files_path)?;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (expected_hash, rel_path) = line.split_once("  ").ok_or_else(|| {
            RepoError::ArchiveVerify(format!("malformed files.txt line: {line}"))
        })?;

        let abs_path = package_dir.join(rel_path);
        let actual_hash = sha256_file(&abs_path).map_err(|e| {
            RepoError::ArchiveVerify(format!(
                "failed to hash {}: {e}",
                rel_path
            ))
        })?;

        if actual_hash != expected_hash {
            return Err(RepoError::ArchiveVerify(format!(
                "hash mismatch for {rel_path}: expected {expected_hash}, got {actual_hash}"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a minimal package directory with a manifest and some content.
    fn make_test_package(tmp: &Path, name: &str, version: &str) -> std::path::PathBuf {
        let pkg_name = format!("{name}-{version}-x86_64-linux");
        let pkg = tmp.join(&pkg_name);
        let meta = pkg.join(".bpkg");
        fs::create_dir_all(&meta).unwrap();

        let manifest = format!(
            r#"[package]
name = "{name}"
version = "{version}"
arch = "x86_64-linux"
description = "Test package {name}"
"#
        );
        fs::write(meta.join("manifest.toml"), manifest).unwrap();

        let bin = pkg.join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join(name), "#!/bin/sh\necho hello\n").unwrap();

        pkg
    }

    #[test]
    fn create_bgx_produces_valid_bzip2() {
        let tmp = TempDir::new().unwrap();
        let pkg = make_test_package(tmp.path(), "hello", "1.0");
        let bgx_path = tmp.path().join("hello-1.0-x86_64-linux.bgx");

        create_bgx(&pkg, &bgx_path).unwrap();

        assert!(bgx_path.exists());
        // Verify it's valid bzip2 by attempting decompression
        let file = File::open(&bgx_path).unwrap();
        let decoder = BzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        let entries: Vec<_> = archive.entries().unwrap().collect();
        assert!(!entries.is_empty());
    }

    #[test]
    fn extract_bgx_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let pkg = make_test_package(tmp.path(), "hello", "1.0");
        let bgx_path = tmp.path().join("hello.bgx");

        create_bgx(&pkg, &bgx_path).unwrap();

        let extract_dir = tmp.path().join("extracted");
        fs::create_dir_all(&extract_dir).unwrap();
        let package_id = extract_bgx(&bgx_path, &extract_dir).unwrap();

        assert_eq!(package_id, "hello-1.0-x86_64-linux");

        // Verify files match original
        let extracted_pkg = extract_dir.join("hello-1.0-x86_64-linux");
        assert!(extracted_pkg.join("bin/hello").exists());
        assert!(extracted_pkg.join(".bpkg/manifest.toml").exists());

        let original_content = fs::read_to_string(pkg.join("bin/hello")).unwrap();
        let extracted_content =
            fs::read_to_string(extracted_pkg.join("bin/hello")).unwrap();
        assert_eq!(original_content, extracted_content);
    }

    #[test]
    fn verify_bgx_returns_correct_info() {
        let tmp = TempDir::new().unwrap();
        let pkg = make_test_package(tmp.path(), "myapp", "2.5");
        let bgx_path = tmp.path().join("myapp.bgx");

        create_bgx(&pkg, &bgx_path).unwrap();

        let info = verify_bgx(&bgx_path).unwrap();
        assert_eq!(info.name, "myapp");
        assert_eq!(info.version, "2.5");
        assert_eq!(info.arch, "x86_64-linux");
        assert_eq!(info.scope, "bingux");
        assert_eq!(info.description, "Test package myapp");
        assert_eq!(info.package_id, "myapp-2.5-x86_64-linux");
        assert!(info.size > 0);
    }

    #[test]
    fn create_bgx_fails_on_missing_manifest() {
        let tmp = TempDir::new().unwrap();
        let no_manifest = tmp.path().join("empty-pkg");
        fs::create_dir_all(&no_manifest).unwrap();

        let result = create_bgx(&no_manifest, &tmp.path().join("out.bgx"));
        assert!(result.is_err());
        match result.unwrap_err() {
            RepoError::ArchiveCreate(msg) => {
                assert!(msg.contains("manifest.toml"));
            }
            e => panic!("expected ArchiveCreate, got: {e}"),
        }
    }

    #[test]
    fn create_bgx_fails_on_nonexistent_dir() {
        let tmp = TempDir::new().unwrap();
        let result = create_bgx(
            &tmp.path().join("nonexistent"),
            &tmp.path().join("out.bgx"),
        );
        assert!(result.is_err());
    }
}
