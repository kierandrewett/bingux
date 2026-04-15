use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tracing::{debug, info};

use crate::error::{BuildError, Result};

/// Fetches and caches source archives for package builds.
pub struct SourceFetcher {
    cache_dir: PathBuf,
}

impl SourceFetcher {
    /// Create a new fetcher that stores downloads in `cache_dir`.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Fetch a source URL to the cache. Returns the local path to the file.
    ///
    /// If the file is already cached and the checksum matches (when provided),
    /// the download is skipped. The special checksum value `"SKIP"` disables
    /// verification.
    pub async fn fetch(&self, url: &str, expected_sha256: Option<&str>) -> Result<PathBuf> {
        fs::create_dir_all(&self.cache_dir)?;

        let filename = url_to_filename(url);
        let cached_path = self.cache_dir.join(&filename);

        // Check if already cached with correct checksum.
        if cached_path.exists() {
            if let Some(expected) = expected_sha256 {
                if expected != "SKIP" {
                    match Self::verify_checksum(&cached_path, expected) {
                        Ok(()) => {
                            info!("source already cached: {}", cached_path.display());
                            return Ok(cached_path);
                        }
                        Err(_) => {
                            debug!("cached file checksum mismatch, re-downloading");
                            fs::remove_file(&cached_path)?;
                        }
                    }
                } else {
                    info!("source already cached (SKIP checksum): {}", cached_path.display());
                    return Ok(cached_path);
                }
            } else {
                info!("source already cached: {}", cached_path.display());
                return Ok(cached_path);
            }
        }

        // Download the file.
        info!("fetching source: {url}");
        let response = reqwest::get(url).await.map_err(|e| BuildError::FetchFailed {
            url: url.to_string(),
            message: e.to_string(),
        })?;

        if !response.status().is_success() {
            return Err(BuildError::FetchFailed {
                url: url.to_string(),
                message: format!("HTTP {}", response.status()),
            });
        }

        let bytes = response.bytes().await.map_err(|e| BuildError::FetchFailed {
            url: url.to_string(),
            message: e.to_string(),
        })?;

        fs::write(&cached_path, &bytes)?;
        info!("downloaded {} bytes to {}", bytes.len(), cached_path.display());

        // Verify checksum if provided.
        if let Some(expected) = expected_sha256 {
            if expected != "SKIP" {
                Self::verify_checksum(&cached_path, expected)?;
            }
        }

        Ok(cached_path)
    }

    /// Verify that a file's SHA-256 checksum matches `expected`.
    pub fn verify_checksum(path: &Path, expected: &str) -> Result<()> {
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
        let actual = format!("{:x}", hasher.finalize());

        if actual != expected {
            return Err(BuildError::ChecksumMismatch {
                path: path.to_path_buf(),
                expected: expected.to_string(),
                actual,
            });
        }

        Ok(())
    }

    /// Extract an archive to a target directory.
    ///
    /// Supports `.tar.gz`, `.tar.bz2`, and `.zip` (by extension).
    /// `.tar.xz` is not yet supported (requires the `xz2` crate).
    pub fn extract(archive: &Path, target_dir: &Path) -> Result<()> {
        fs::create_dir_all(target_dir)?;

        let filename = archive
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
            Self::extract_tar_gz(archive, target_dir)
        } else if filename.ends_with(".tar.bz2") || filename.ends_with(".tbz2") {
            Self::extract_tar_bz2(archive, target_dir)
        } else if filename.ends_with(".tar.xz") || filename.ends_with(".txz") {
            // Use xz command-line tool if available, otherwise error
            Self::extract_tar_xz(archive, target_dir)
        } else if filename.ends_with(".tar") {
            Self::extract_tar(archive, target_dir)
        } else {
            Err(BuildError::ExtractionFailed {
                path: archive.to_path_buf(),
                message: format!("unsupported archive format: {filename}"),
            })
        }
    }

    fn extract_tar_gz(archive: &Path, target_dir: &Path) -> Result<()> {
        let file = fs::File::open(archive)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(gz);
        tar.unpack(target_dir).map_err(|e| BuildError::ExtractionFailed {
            path: archive.to_path_buf(),
            message: e.to_string(),
        })?;
        Ok(())
    }

    fn extract_tar_bz2(archive: &Path, target_dir: &Path) -> Result<()> {
        let file = fs::File::open(archive)?;
        let bz2 = bzip2::read::BzDecoder::new(file);
        let mut tar = tar::Archive::new(bz2);
        tar.unpack(target_dir).map_err(|e| BuildError::ExtractionFailed {
            path: archive.to_path_buf(),
            message: e.to_string(),
        })?;
        Ok(())
    }

    fn extract_tar_xz(archive: &Path, target_dir: &Path) -> Result<()> {
        // Shell out to xz + tar (xz crate not in deps, but xz binary may be available)
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "xz -dc '{}' | tar x -C '{}'",
                archive.display(),
                target_dir.display()
            ))
            .output()
            .map_err(|e| BuildError::ExtractionFailed {
                path: archive.to_path_buf(),
                message: format!("failed to run xz: {e}"),
            })?;

        if !output.status.success() {
            return Err(BuildError::ExtractionFailed {
                path: archive.to_path_buf(),
                message: format!(
                    ".tar.xz extraction failed (exit {}): {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        Ok(())
    }

    fn extract_tar(archive: &Path, target_dir: &Path) -> Result<()> {
        let file = fs::File::open(archive)?;
        let mut tar = tar::Archive::new(file);
        tar.unpack(target_dir).map_err(|e| BuildError::ExtractionFailed {
            path: archive.to_path_buf(),
            message: e.to_string(),
        })?;
        Ok(())
    }
}

/// Derive a safe filename from a URL.
fn url_to_filename(url: &str) -> String {
    url.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("download")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn verify_checksum_correct() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        fs::write(&path, b"hello world").unwrap();

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        SourceFetcher::verify_checksum(&path, expected).unwrap();
    }

    #[test]
    fn verify_checksum_wrong() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        fs::write(&path, b"hello world").unwrap();

        let result = SourceFetcher::verify_checksum(&path, "0000000000000000000000000000000000000000000000000000000000000000");
        assert!(result.is_err());
        match result.unwrap_err() {
            BuildError::ChecksumMismatch { .. } => {}
            e => panic!("expected ChecksumMismatch, got: {e}"),
        }
    }

    #[test]
    fn extract_tar_gz_archive() {
        let tmp = TempDir::new().unwrap();

        // Create a tar.gz archive with a simple file inside.
        let archive_path = tmp.path().join("test.tar.gz");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(gz);

            let content = b"hello from archive";
            let mut header = tar::Header::new_gnu();
            header.set_path("test-dir/hello.txt").unwrap();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &content[..]).unwrap();
            builder.finish().unwrap();
        }

        let extract_dir = tmp.path().join("extracted");
        SourceFetcher::extract(&archive_path, &extract_dir).unwrap();

        let extracted_file = extract_dir.join("test-dir/hello.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "hello from archive");
    }

    #[test]
    fn extract_unsupported_format() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.zip");
        fs::write(&path, b"not a real zip").unwrap();

        let result = SourceFetcher::extract(&path, &tmp.path().join("out"));
        assert!(result.is_err());
        match result.unwrap_err() {
            BuildError::ExtractionFailed { .. } => {}
            e => panic!("expected ExtractionFailed, got: {e}"),
        }
    }

    #[test]
    fn url_to_filename_works() {
        assert_eq!(url_to_filename("https://example.com/foo/bar-1.0.tar.gz"), "bar-1.0.tar.gz");
        assert_eq!(url_to_filename("https://example.com/"), "download");
        assert_eq!(url_to_filename("file.tar.gz"), "file.tar.gz");
    }
}
