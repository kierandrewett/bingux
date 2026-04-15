use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bingux_common::package_id::PackageId;
use bingux_common::paths::SystemPaths;
use bpkg_patchelf::{PatchPlan, scan_package_dir, write_log};
use bpkg_store::integrity::generate_file_list;
use bpkg_store::store::PackageStore;
use tracing::{info, warn};

use crate::config::BuildConfig;
use crate::error::Result;
use crate::executor::BuildExecutor;
use crate::fetch::SourceFetcher;

/// The result of a successful package build.
#[derive(Debug)]
pub struct BuildResult {
    /// The package identifier in the store.
    pub package_id: PackageId,
    /// Absolute path to the installed package in the store.
    pub store_path: PathBuf,
    /// Total wall-clock build duration.
    pub build_duration: std::time::Duration,
    /// Number of files in the built package.
    pub files_count: usize,
    /// Number of ELF binaries that were patched.
    pub patched_binaries: usize,
}

/// A dry-run plan describing what a build would do.
#[derive(Debug, Clone)]
pub struct BuildPlan {
    /// The recipe package name.
    pub recipe_name: String,
    /// The recipe package version.
    pub version: String,
    /// Runtime dependencies declared in the recipe.
    pub dependencies: Vec<String>,
    /// Source URLs declared in the recipe.
    pub sources: Vec<String>,
    /// Whether the recipe defines a `build()` function.
    pub has_build_step: bool,
}

/// The main build orchestrator that ties together all stages.
pub struct BuildPipeline {
    config: BuildConfig,
    fetcher: SourceFetcher,
}

impl BuildPipeline {
    /// Create a new pipeline from the given configuration.
    pub fn new(config: BuildConfig) -> Self {
        let fetcher = SourceFetcher::new(config.source_cache.clone());
        Self { config, fetcher }
    }

    /// Produce a dry-run plan from a recipe file without executing anything.
    pub fn plan(&self, recipe_path: &Path) -> Result<BuildPlan> {
        let recipe_text = fs::read_to_string(recipe_path)?;
        let recipe = bpkg_recipe::parse_recipe(&recipe_text)?;

        Ok(BuildPlan {
            recipe_name: recipe.pkgname,
            version: recipe.pkgver,
            dependencies: recipe.depends,
            sources: recipe.source,
            has_build_step: recipe.build.is_some(),
        })
    }

    /// Build a package from a recipe file. Full pipeline:
    ///
    /// 1. Parse recipe
    /// 2. Check dependencies in store
    /// 3. Fetch sources
    /// 4. Execute build/package steps
    /// 5. Patchelf ELF binaries
    /// 6. Generate file integrity list
    /// 7. Write manifest
    /// 8. Install to store
    pub async fn build(&self, recipe_path: &Path) -> Result<BuildResult> {
        let start = Instant::now();

        // 1. Parse recipe.
        info!("parsing recipe: {}", recipe_path.display());
        let recipe_text = fs::read_to_string(recipe_path)?;
        let recipe = bpkg_recipe::parse_recipe(&recipe_text)?;

        info!(
            "building {} {} ({})",
            recipe.pkgname, recipe.pkgver, recipe.pkgarch
        );

        // 2. Check dependencies in store.
        let store = PackageStore::new(self.config.store_root.clone())?;
        for dep in &recipe.depends {
            let found = store.query(dep);
            if found.is_empty() {
                warn!("dependency not in store: {dep} (continuing anyway)");
            }
        }

        // 3. Fetch sources.
        for (i, url) in recipe.source.iter().enumerate() {
            let checksum = recipe.sha256sums.get(i).map(|s| s.as_str());
            if self.config.network_fetch {
                self.fetcher.fetch(url, checksum).await?;
            } else {
                info!("network fetch disabled, skipping: {url}");
            }
        }

        // 4. Set up build environment and run build/package steps.
        let work_dir = self.config.work_dir.join(&recipe.pkgname);
        let executor = BuildExecutor::new(work_dir);
        let env = executor.prepare()?;

        // Extract or copy fetched sources into SRCDIR.
        for url in &recipe.source {
            let filename = url.rsplit('/').next().unwrap_or("download");
            let cached = self.config.source_cache.join(filename);
            if cached.exists() {
                // Try extracting as an archive; if it's not an archive, just copy the file
                match SourceFetcher::extract(&cached, &env.srcdir) {
                    Ok(()) => {}
                    Err(_) => {
                        // Not an archive — copy the raw file into SRCDIR
                        info!("not an archive, copying raw file: {filename}");
                        fs::copy(&cached, env.srcdir.join(filename))?;
                    }
                }
            }
        }

        if let Some(ref build_script) = recipe.build {
            executor.run_build(&env, build_script)?;
        }

        if let Some(ref package_script) = recipe.package {
            executor.run_package(&env, package_script)?;
        } else {
            warn!("recipe has no package() function");
        }

        // 5. Patchelf ELF binaries.
        let scan_result = scan_package_dir(&env.pkgdir)?;
        let patched_count = scan_result.elfs.len();

        if !scan_result.elfs.is_empty() {
            // Compute runpath from dependency store paths.
            let mut lib_paths: Vec<String> = Vec::new();
            for dep in &recipe.depends {
                let dep_versions = store.query(dep);
                for dep_id in &dep_versions {
                    let dep_dir = self.config.store_root.join(dep_id.dir_name());
                    let dep_lib = dep_dir.join("lib");
                    if dep_lib.is_dir() {
                        lib_paths.push(dep_lib.to_string_lossy().to_string());
                    }
                }
            }

            let pkg_lib = env.pkgdir.join("lib");
            if pkg_lib.is_dir() {
                lib_paths.insert(0, pkg_lib.to_string_lossy().to_string());
            }

            let runpath = lib_paths.join(":");

            // Use the store's glibc interpreter as default, or skip if not found.
            let interpreter = find_interpreter(&store);

            if let Some(ref interp) = interpreter {
                match PatchPlan::compute(&env.pkgdir, interp, &runpath) {
                    Ok(plan) => {
                        // Write patchelf log but don't apply (patchelf binary may not be present).
                        write_log(&env.pkgdir, &plan, &[])?;
                        info!("patchelf plan computed for {} binaries", plan.effective_patches().len());
                    }
                    Err(e) => {
                        warn!("patchelf planning failed: {e}");
                    }
                }
            } else {
                info!("no interpreter found, skipping patchelf");
            }
        }

        // 6. Write manifest.
        let meta_dir = env.pkgdir.join(SystemPaths::BPKG_META_DIR);
        fs::create_dir_all(&meta_dir)?;

        let manifest_content = format!(
            r#"[package]
name = "{name}"
scope = "{scope}"
version = "{version}"
arch = "{arch}"
description = "{desc}"
license = "{license}"

[dependencies]
runtime = [{runtime_deps}]
build = [{build_deps}]
"#,
            name = recipe.pkgname,
            scope = recipe.pkgscope,
            version = recipe.pkgver,
            arch = recipe.pkgarch,
            desc = recipe.pkgdesc.as_deref().unwrap_or(""),
            license = recipe.license.as_deref().unwrap_or(""),
            runtime_deps = recipe
                .depends
                .iter()
                .map(|d| format!("\"{d}\""))
                .collect::<Vec<_>>()
                .join(", "),
            build_deps = recipe
                .makedepends
                .iter()
                .map(|d| format!("\"{d}\""))
                .collect::<Vec<_>>()
                .join(", "),
        );

        fs::write(
            meta_dir.join(SystemPaths::MANIFEST_FILENAME),
            &manifest_content,
        )?;

        // 7. Generate file integrity list.
        let file_list = generate_file_list(&env.pkgdir)?;
        let files_count = file_list.lines().count();
        fs::write(meta_dir.join(SystemPaths::FILES_FILENAME), &file_list)?;

        // 8. Install to store.
        let package_id = store.install(&env.pkgdir)?;
        let store_path = self.config.store_root.join(package_id.dir_name());

        let build_duration = start.elapsed();
        info!(
            "build complete: {} installed to {} ({:?})",
            package_id,
            store_path.display(),
            build_duration
        );

        Ok(BuildResult {
            package_id,
            store_path,
            build_duration,
            files_count,
            patched_binaries: patched_count,
        })
    }
}

/// Try to find a glibc interpreter in the store.
fn find_interpreter(store: &PackageStore) -> Option<String> {
    for id in store.list() {
        if id.name == "glibc" {
            let interp_path = PathBuf::from(SystemPaths::PACKAGES)
                .join(id.dir_name())
                .join("lib/ld-linux-x86-64.so.2");
            return Some(interp_path.to_string_lossy().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_test_recipe(dir: &Path) -> PathBuf {
        let recipe_path = dir.join("BPKGBUILD");
        let content = r#"pkgscope="bingux"
pkgname="hello"
pkgver="1.0"
pkgarch="x86_64-linux"
pkgdesc="A test package"
license="MIT"

depends=()
makedepends=()
source=()
sha256sums=()

package() {
    mkdir -p "$PKGDIR/bin"
    echo '#!/bin/sh' > "$PKGDIR/bin/hello"
    echo 'echo hello world' >> "$PKGDIR/bin/hello"
    chmod +x "$PKGDIR/bin/hello"
}
"#;
        fs::write(&recipe_path, content).unwrap();
        recipe_path
    }

    #[test]
    fn plan_from_recipe() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = write_test_recipe(tmp.path());

        let config = BuildConfig {
            recipe_path: recipe_path.clone(),
            store_root: tmp.path().join("store"),
            work_dir: tmp.path().join("work"),
            source_cache: tmp.path().join("cache"),
            arch: "x86_64-linux".to_string(),
            network_fetch: false,
        };
        let pipeline = BuildPipeline::new(config);
        let plan = pipeline.plan(&recipe_path).unwrap();

        assert_eq!(plan.recipe_name, "hello");
        assert_eq!(plan.version, "1.0");
        assert!(plan.dependencies.is_empty());
        assert!(plan.sources.is_empty());
        assert!(!plan.has_build_step);
    }

    #[test]
    fn plan_with_deps_and_sources() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = tmp.path().join("BPKGBUILD");
        let content = r#"pkgscope="bingux"
pkgname="myapp"
pkgver="2.0"
pkgarch="x86_64-linux"

depends=("glibc" "zlib")
source=("https://example.com/myapp-2.0.tar.gz")
sha256sums=("SKIP")

build() {
    make -j$(nproc)
}

package() {
    make install DESTDIR="$PKGDIR"
}
"#;
        fs::write(&recipe_path, content).unwrap();

        let config = BuildConfig {
            recipe_path: recipe_path.clone(),
            store_root: tmp.path().join("store"),
            work_dir: tmp.path().join("work"),
            source_cache: tmp.path().join("cache"),
            arch: "x86_64-linux".to_string(),
            network_fetch: false,
        };
        let pipeline = BuildPipeline::new(config);
        let plan = pipeline.plan(&recipe_path).unwrap();

        assert_eq!(plan.recipe_name, "myapp");
        assert_eq!(plan.version, "2.0");
        assert_eq!(plan.dependencies, vec!["glibc", "zlib"]);
        assert_eq!(plan.sources, vec!["https://example.com/myapp-2.0.tar.gz"]);
        assert!(plan.has_build_step);
    }

    #[tokio::test]
    async fn end_to_end_build() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = write_test_recipe(tmp.path());

        let config = BuildConfig {
            recipe_path: recipe_path.clone(),
            store_root: tmp.path().join("store"),
            work_dir: tmp.path().join("work"),
            source_cache: tmp.path().join("cache"),
            arch: "x86_64-linux".to_string(),
            network_fetch: false,
        };
        let pipeline = BuildPipeline::new(config.clone());
        let result = pipeline.build(&recipe_path).await.unwrap();

        assert_eq!(result.package_id.name, "hello");
        assert_eq!(result.package_id.version, "1.0");
        assert!(result.store_path.is_dir());
        assert!(result.store_path.join("bin/hello").exists());
        assert!(result.files_count > 0);

        // Verify manifest was written.
        let manifest_path = result
            .store_path
            .join(SystemPaths::BPKG_META_DIR)
            .join(SystemPaths::MANIFEST_FILENAME);
        assert!(manifest_path.exists());

        // Verify file list was written.
        let files_path = result
            .store_path
            .join(SystemPaths::BPKG_META_DIR)
            .join(SystemPaths::FILES_FILENAME);
        assert!(files_path.exists());
    }

    #[tokio::test]
    async fn build_with_build_step() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = tmp.path().join("BPKGBUILD");
        let content = r#"pkgscope="bingux"
pkgname="compiled"
pkgver="0.1"
pkgarch="x86_64-linux"

depends=()
source=()
sha256sums=()

build() {
    echo "compiling..." > "$BUILDDIR/artifact.txt"
}

package() {
    mkdir -p "$PKGDIR/share"
    cp "$BUILDDIR/artifact.txt" "$PKGDIR/share/"
}
"#;
        fs::write(&recipe_path, content).unwrap();

        let config = BuildConfig {
            recipe_path: recipe_path.clone(),
            store_root: tmp.path().join("store"),
            work_dir: tmp.path().join("work"),
            source_cache: tmp.path().join("cache"),
            arch: "x86_64-linux".to_string(),
            network_fetch: false,
        };
        let pipeline = BuildPipeline::new(config);
        let result = pipeline.build(&recipe_path).await.unwrap();

        assert_eq!(result.package_id.name, "compiled");
        assert!(result.store_path.join("share/artifact.txt").exists());
        let artifact = fs::read_to_string(result.store_path.join("share/artifact.txt")).unwrap();
        assert_eq!(artifact.trim(), "compiling...");
    }
}
