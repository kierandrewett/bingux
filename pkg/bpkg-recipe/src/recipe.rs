use serde::{Deserialize, Serialize};

/// A parsed BPKGBUILD recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Package scope (e.g. "bingux").
    pub pkgscope: String,
    /// Package name (e.g. "firefox").
    pub pkgname: String,
    /// Package version (e.g. "128.0.1").
    pub pkgver: String,
    /// Target architecture (e.g. "x86_64-linux").
    pub pkgarch: String,
    /// Short description.
    pub pkgdesc: Option<String>,
    /// SPDX license identifier.
    pub license: Option<String>,

    /// Runtime dependencies (e.g. ["glibc-2.39", "gtk3-3.24"]).
    pub depends: Vec<String>,
    /// Build-time dependencies.
    pub makedepends: Vec<String>,

    /// Exported paths visible to dependents.
    pub exports: Vec<String>,

    /// Source URLs (after variable expansion).
    pub source: Vec<String>,
    /// SHA-256 checksums for sources (one per source entry).
    pub sha256sums: Vec<String>,

    /// dlopen hints: `libfoo.so=/system/packages/foo-*/lib/`
    pub dlopen_hints: Vec<String>,

    /// Raw body of the `build()` function, if present.
    pub build: Option<String>,
    /// Raw body of the `package()` function.
    pub package: Option<String>,
}

impl Recipe {
    /// Create an empty recipe with all fields at default/empty values.
    pub fn empty() -> Self {
        Self {
            pkgscope: String::new(),
            pkgname: String::new(),
            pkgver: String::new(),
            pkgarch: String::new(),
            pkgdesc: None,
            license: None,
            depends: Vec::new(),
            makedepends: Vec::new(),
            exports: Vec::new(),
            source: Vec::new(),
            sha256sums: Vec::new(),
            dlopen_hints: Vec::new(),
            build: None,
            package: None,
        }
    }
}
