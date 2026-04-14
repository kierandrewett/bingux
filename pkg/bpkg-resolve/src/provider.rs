use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bingux_common::BinguxError;

/// Maps library names to the store paths that provide them.
///
/// Scans installed packages' `lib/` directories to build a lookup table from
/// shared-object names (e.g. `libgtk-3.so.0`) to the full store path where
/// that library lives.
#[derive(Debug, Clone)]
pub struct LibraryProvider {
    /// library filename → full path inside the store
    libs: HashMap<String, PathBuf>,
}

impl LibraryProvider {
    /// Scan the package store at the given root and build the provider map.
    ///
    /// `store_root` is typically `/system/packages`.
    pub fn scan(store_root: &Path) -> Result<Self, BinguxError> {
        let mut libs = HashMap::new();

        let entries = match fs::read_dir(store_root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self { libs });
            }
            Err(e) => return Err(BinguxError::Io(e)),
        };

        for entry in entries {
            let entry = entry?;
            let pkg_dir = entry.path();
            if !pkg_dir.is_dir() {
                continue;
            }

            let lib_dir = pkg_dir.join("lib");
            if !lib_dir.is_dir() {
                continue;
            }

            let lib_entries = fs::read_dir(&lib_dir)?;
            for lib_entry in lib_entries {
                let lib_entry = lib_entry?;
                let lib_path = lib_entry.path();

                // Only index files (not directories) that look like shared objects.
                if lib_path.is_file() {
                    if let Some(name) = lib_path.file_name().and_then(|n| n.to_str()) {
                        if name.contains(".so") {
                            libs.insert(name.to_string(), lib_path);
                        }
                    }
                }
            }
        }

        Ok(Self { libs })
    }

    /// Look up which store path provides a given library.
    pub fn find(&self, library_name: &str) -> Option<&Path> {
        self.libs.get(library_name).map(|p| p.as_path())
    }

    /// Return the lib directory (parent) for a library, if found.
    pub fn find_lib_dir(&self, library_name: &str) -> Option<PathBuf> {
        self.libs
            .get(library_name)
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }

    /// Return all known library names.
    pub fn known_libraries(&self) -> Vec<&str> {
        self.libs.keys().map(|s| s.as_str()).collect()
    }

    /// Detect conflicts: two packages providing the same library.
    ///
    /// Returns a map from library name to the list of store paths if any
    /// library is provided by more than one package.
    pub fn scan_conflicts(store_root: &Path) -> Result<Vec<ExportConflict>, BinguxError> {
        let mut lib_to_pkgs: HashMap<String, Vec<PathBuf>> = HashMap::new();

        let entries = match fs::read_dir(store_root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(e) => return Err(BinguxError::Io(e)),
        };

        for entry in entries {
            let entry = entry?;
            let pkg_dir = entry.path();
            if !pkg_dir.is_dir() {
                continue;
            }

            // Check lib/ for .so files
            Self::scan_dir_for_conflicts(&pkg_dir.join("lib"), &mut lib_to_pkgs)?;
            // Check bin/ for executables
            Self::scan_dir_for_conflicts(&pkg_dir.join("bin"), &mut lib_to_pkgs)?;
        }

        let mut conflicts = Vec::new();
        for (name, paths) in &lib_to_pkgs {
            if paths.len() > 1 {
                conflicts.push(ExportConflict {
                    name: name.clone(),
                    providers: paths.clone(),
                });
            }
        }

        Ok(conflicts)
    }

    fn scan_dir_for_conflicts(
        dir: &Path,
        map: &mut HashMap<String, Vec<PathBuf>>,
    ) -> Result<(), BinguxError> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    map.entry(name.to_string())
                        .or_default()
                        .push(path);
                }
            }
        }
        Ok(())
    }
}

/// A conflict where multiple packages export the same filename.
#[derive(Debug, Clone)]
pub struct ExportConflict {
    pub name: String,
    pub providers: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_fake_store(root: &Path) {
        // gtk3 package with libgtk-3.so.0
        let gtk_lib = root.join("gtk3-3.24-x86_64-linux/lib");
        fs::create_dir_all(&gtk_lib).unwrap();
        fs::write(gtk_lib.join("libgtk-3.so.0"), b"fake elf").unwrap();

        // glib package with libglib-2.0.so.0
        let glib_lib = root.join("glib-2.78-x86_64-linux/lib");
        fs::create_dir_all(&glib_lib).unwrap();
        fs::write(glib_lib.join("libglib-2.0.so.0"), b"fake elf").unwrap();
    }

    #[test]
    fn find_library() {
        let tmp = tempfile::tempdir().unwrap();
        make_fake_store(tmp.path());

        let provider = LibraryProvider::scan(tmp.path()).unwrap();

        let path = provider.find("libgtk-3.so.0").unwrap();
        assert!(path.ends_with("lib/libgtk-3.so.0"));
        assert!(path.to_str().unwrap().contains("gtk3-3.24"));

        assert!(provider.find("libglib-2.0.so.0").is_some());
        assert!(provider.find("libnotexist.so.1").is_none());
    }

    #[test]
    fn find_lib_dir() {
        let tmp = tempfile::tempdir().unwrap();
        make_fake_store(tmp.path());

        let provider = LibraryProvider::scan(tmp.path()).unwrap();
        let dir = provider.find_lib_dir("libgtk-3.so.0").unwrap();
        assert!(dir.ends_with("lib"));
        assert!(dir.to_str().unwrap().contains("gtk3-3.24"));
    }

    #[test]
    fn conflict_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Two packages both providing libfoo.so.1
        let pkg_a = root.join("foo-a-1.0-x86_64-linux/lib");
        fs::create_dir_all(&pkg_a).unwrap();
        fs::write(pkg_a.join("libfoo.so.1"), b"a").unwrap();

        let pkg_b = root.join("foo-b-1.0-x86_64-linux/lib");
        fs::create_dir_all(&pkg_b).unwrap();
        fs::write(pkg_b.join("libfoo.so.1"), b"b").unwrap();

        let conflicts = LibraryProvider::scan_conflicts(root).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].name, "libfoo.so.1");
        assert_eq!(conflicts[0].providers.len(), 2);
    }

    #[test]
    fn empty_store() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = LibraryProvider::scan(tmp.path()).unwrap();
        assert!(provider.known_libraries().is_empty());
    }
}
