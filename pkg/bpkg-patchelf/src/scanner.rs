use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use bingux_common::BinguxError;
use walkdir::WalkDir;

/// The ELF magic bytes: `\x7fELF`.
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// Classification of an ELF file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElfKind {
    /// ET_EXEC or ET_DYN with a PT_INTERP — a runnable executable.
    Executable,
    /// ET_DYN without a PT_INTERP — a shared library.
    SharedLibrary,
    /// Relocatable object or core dump — we skip these.
    Other,
}

/// A discovered ELF file in a package directory.
#[derive(Debug, Clone)]
pub struct ScannedElf {
    pub path: PathBuf,
    pub kind: ElfKind,
}

/// Result of scanning a package directory.
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// ELF files found.
    pub elfs: Vec<ScannedElf>,
    /// Non-ELF files skipped (scripts, data, etc.).
    pub skipped: Vec<PathBuf>,
}

/// Check whether a file begins with the ELF magic bytes.
pub fn is_elf(path: &Path) -> Result<bool, BinguxError> {
    let mut f = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return Err(BinguxError::Io(e)),
    };
    let mut magic = [0u8; 4];
    match f.read_exact(&mut magic) {
        Ok(()) => Ok(magic == ELF_MAGIC),
        // File too small to contain the magic — not ELF.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(BinguxError::Io(e)),
    }
}

/// Classify an ELF file using goblin.
pub fn classify_elf(path: &Path) -> Result<ElfKind, BinguxError> {
    let data = fs::read(path)?;
    let elf = goblin::elf::Elf::parse(&data).map_err(|e| BinguxError::ElfParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    match elf.header.e_type {
        goblin::elf::header::ET_EXEC => Ok(ElfKind::Executable),
        goblin::elf::header::ET_DYN => {
            // PIE executables are ET_DYN but have a PT_INTERP.
            if elf.interpreter.is_some() {
                Ok(ElfKind::Executable)
            } else {
                Ok(ElfKind::SharedLibrary)
            }
        }
        _ => Ok(ElfKind::Other),
    }
}

/// Walk a package directory and find all ELF files.
///
/// Only regular files are inspected; symlinks, directories, and
/// the `.bpkg/` metadata directory are skipped.
pub fn scan_package_dir(dir: &Path) -> Result<ScanResult, BinguxError> {
    let mut elfs = Vec::new();
    let mut skipped = Vec::new();

    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| BinguxError::Io(e.into()))?;
        let path = entry.path();

        // Skip the metadata directory.
        if path
            .components()
            .any(|c| c.as_os_str() == ".bpkg")
        {
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        if is_elf(path)? {
            let kind = classify_elf(path)?;
            elfs.push(ScannedElf {
                path: path.to_path_buf(),
                kind,
            });
        } else {
            skipped.push(path.to_path_buf());
        }
    }

    Ok(ScanResult { elfs, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detect_elf_magic() {
        let tmp = tempfile::tempdir().unwrap();

        // A file with ELF magic
        let elf_path = tmp.path().join("prog");
        let mut data = ELF_MAGIC.to_vec();
        data.extend_from_slice(&[0u8; 100]); // padding
        fs::write(&elf_path, &data).unwrap();
        assert!(is_elf(&elf_path).unwrap());

        // A shell script
        let script_path = tmp.path().join("script.sh");
        fs::write(&script_path, b"#!/bin/bash\necho hi").unwrap();
        assert!(!is_elf(&script_path).unwrap());

        // An empty file
        let empty_path = tmp.path().join("empty");
        fs::write(&empty_path, b"").unwrap();
        assert!(!is_elf(&empty_path).unwrap());
    }

    #[test]
    fn scan_directory_with_mixed_files() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("mypkg");
        let bin = pkg.join("bin");
        fs::create_dir_all(&bin).unwrap();

        // A shell script (non-ELF)
        fs::write(bin.join("wrapper.sh"), b"#!/bin/bash\nexec ./real").unwrap();

        // A data file
        fs::write(pkg.join("data.txt"), b"hello world").unwrap();

        // .bpkg metadata should be skipped
        let meta = pkg.join(".bpkg");
        fs::create_dir_all(&meta).unwrap();
        fs::write(meta.join("manifest.toml"), b"[package]").unwrap();

        let result = scan_package_dir(&pkg).unwrap();
        // No actual ELF files (we only wrote scripts/data)
        assert!(result.elfs.is_empty());
        // The wrapper.sh and data.txt should be in skipped
        assert_eq!(result.skipped.len(), 2);
        // .bpkg files should NOT be in skipped
        assert!(
            !result
                .skipped
                .iter()
                .any(|p| p.to_str().unwrap().contains(".bpkg"))
        );
    }

    /// Test scanning a real system binary if available.
    #[test]
    fn scan_real_binary() {
        let path = Path::new("/usr/bin/true");
        if !path.exists() {
            eprintln!("skipping: /usr/bin/true not found");
            return;
        }
        assert!(is_elf(path).unwrap());
        let kind = classify_elf(path).unwrap();
        // /usr/bin/true is typically a PIE executable (ET_DYN with PT_INTERP).
        assert_eq!(kind, ElfKind::Executable);
    }
}
