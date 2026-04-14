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
