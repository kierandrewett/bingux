use std::fs;
use std::io::Read;
use std::path::Path;

use bingux_common::error::{BinguxError, Result};
use bingux_common::paths::SystemPaths;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

/// Generate a file integrity list for a package directory.
///
/// Walks all files (excluding `.bpkg/files.txt` itself), computes their
/// SHA-256 hash, and returns lines of `<sha256>  <relative_path>`.
pub fn generate_file_list(package_dir: &Path) -> Result<String> {
    let files_rel_path = format!("{}/{}", SystemPaths::BPKG_META_DIR, SystemPaths::FILES_FILENAME);
    let mut entries: Vec<String> = Vec::new();

    for entry in WalkDir::new(package_dir).sort_by_file_name() {
        let entry = entry.map_err(|e| BinguxError::Io(e.into()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel_path = entry
            .path()
            .strip_prefix(package_dir)
            .unwrap()
            .to_string_lossy()
            .to_string();

        // Skip files.txt itself to avoid circular hashing
        if rel_path == files_rel_path {
            continue;
        }

        let hash = sha256_file(entry.path())?;
        entries.push(format!("{hash}  {rel_path}"));
    }

    Ok(entries.join("\n"))
}

/// Verify the file integrity list for a package directory.
///
/// Reads `.bpkg/files.txt` and checks that every listed file matches its
/// recorded SHA-256 hash.
pub fn verify_file_list(package_dir: &Path) -> Result<()> {
    let files_path = package_dir
        .join(SystemPaths::BPKG_META_DIR)
        .join(SystemPaths::FILES_FILENAME);
    let contents = fs::read_to_string(&files_path)?;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Format: <sha256>  <relative_path>  (two spaces)
        let (expected_hash, rel_path) = line.split_once("  ").ok_or_else(|| {
            BinguxError::Manifest {
                package: package_dir.display().to_string(),
                message: format!("malformed files.txt line: {line}"),
            }
        })?;

        let abs_path = package_dir.join(rel_path);
        let actual_hash = sha256_file(&abs_path)?;

        if actual_hash != expected_hash {
            return Err(BinguxError::IntegrityCheckFailed {
                path: rel_path.into(),
                expected: expected_hash.to_string(),
                actual: actual_hash,
            });
        }
    }

    Ok(())
}

/// Compute the hex-encoded SHA-256 of a file.
fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Create a minimal package directory with some files.
    fn make_package(tmp: &Path) -> PathBuf {
        let pkg = tmp.join("test-pkg");
        let meta = pkg.join(".bpkg");
        fs::create_dir_all(&meta).unwrap();

        fs::write(
            meta.join("manifest.toml"),
            "[package]\nname = \"test\"\nversion = \"1.0\"\narch = \"x86_64-linux\"\n",
        )
        .unwrap();

        let bin = pkg.join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("hello"), "#!/bin/sh\necho hello\n").unwrap();

        let lib = pkg.join("lib");
        fs::create_dir_all(&lib).unwrap();
        fs::write(lib.join("libtest.so"), b"fake library content").unwrap();

        pkg
    }

    #[test]
    fn generate_and_verify_file_list() {
        let tmp = TempDir::new().unwrap();
        let pkg = make_package(tmp.path());

        let file_list = generate_file_list(&pkg).unwrap();
        assert!(!file_list.is_empty());

        // Each line should have a hash and path
        for line in file_list.lines() {
            let parts: Vec<&str> = line.splitn(2, "  ").collect();
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0].len(), 64); // SHA-256 hex length
        }

        // Write files.txt and verify
        let files_path = pkg.join(".bpkg").join("files.txt");
        fs::write(&files_path, &file_list).unwrap();
        verify_file_list(&pkg).unwrap();
    }

    #[test]
    fn verify_fails_on_modified_file() {
        let tmp = TempDir::new().unwrap();
        let pkg = make_package(tmp.path());

        let file_list = generate_file_list(&pkg).unwrap();
        let files_path = pkg.join(".bpkg").join("files.txt");
        fs::write(&files_path, &file_list).unwrap();

        // Tamper with a file
        fs::write(pkg.join("bin/hello"), "TAMPERED").unwrap();

        let result = verify_file_list(&pkg);
        assert!(result.is_err());
        match result.unwrap_err() {
            BinguxError::IntegrityCheckFailed { path, .. } => {
                assert_eq!(path.to_string_lossy(), "bin/hello");
            }
            e => panic!("expected IntegrityCheckFailed, got: {e}"),
        }
    }

    #[test]
    fn files_txt_not_included_in_own_list() {
        let tmp = TempDir::new().unwrap();
        let pkg = make_package(tmp.path());

        // Write a dummy files.txt first
        let files_path = pkg.join(".bpkg").join("files.txt");
        fs::write(&files_path, "dummy").unwrap();

        let file_list = generate_file_list(&pkg).unwrap();
        // files.txt should not appear in its own list
        assert!(!file_list.contains(".bpkg/files.txt"));
    }
}
