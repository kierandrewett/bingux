use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use tracing::{debug, info};

use crate::error::{BuildError, Result};

/// The build environment directories used during package construction.
#[derive(Debug, Clone)]
pub struct BuildEnvironment {
    /// `$SRCDIR` -- where sources are extracted.
    pub srcdir: PathBuf,
    /// `$BUILDDIR` -- for build artifacts (out-of-tree builds).
    pub builddir: PathBuf,
    /// `$PKGDIR` -- package output tree (what gets installed to the store).
    pub pkgdir: PathBuf,
}

/// Output from running a build or package step.
#[derive(Debug, Clone)]
pub struct BuildOutput {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Wall-clock duration of the step.
    pub duration: std::time::Duration,
}

/// Executes recipe build/package functions as shell scripts.
pub struct BuildExecutor {
    work_dir: PathBuf,
}

impl BuildExecutor {
    /// Create a new executor rooted at `work_dir`.
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }

    /// Prepare the build environment by creating the required directories.
    pub fn prepare(&self) -> Result<BuildEnvironment> {
        let srcdir = self.work_dir.join("src");
        let builddir = self.work_dir.join("build");
        let pkgdir = self.work_dir.join("pkg");

        fs::create_dir_all(&srcdir)?;
        fs::create_dir_all(&builddir)?;
        fs::create_dir_all(&pkgdir)?;

        info!("prepared build environment in {}", self.work_dir.display());

        Ok(BuildEnvironment {
            srcdir,
            builddir,
            pkgdir,
        })
    }

    /// Execute the recipe's `build()` function, if present.
    ///
    /// Runs as a bash script with `SRCDIR`, `BUILDDIR`, and `PKGDIR`
    /// environment variables set. The working directory is set to `$BUILDDIR`.
    pub fn run_build(&self, env: &BuildEnvironment, build_script: &str) -> Result<BuildOutput> {
        info!("running build() step");
        let output = self.run_script(env, build_script, &env.builddir)?;

        if output.exit_code != 0 {
            return Err(BuildError::BuildFailed {
                code: output.exit_code,
                stderr: output.stderr.clone(),
            });
        }

        Ok(output)
    }

    /// Execute the recipe's `package()` function.
    ///
    /// Runs as a bash script with `SRCDIR`, `BUILDDIR`, and `PKGDIR`
    /// environment variables set. The working directory is set to `$SRCDIR`.
    pub fn run_package(&self, env: &BuildEnvironment, package_script: &str) -> Result<BuildOutput> {
        info!("running package() step");
        let output = self.run_script(env, package_script, &env.srcdir)?;

        if output.exit_code != 0 {
            return Err(BuildError::PackageFailed {
                code: output.exit_code,
                stderr: output.stderr.clone(),
            });
        }

        Ok(output)
    }

    /// Run a shell script with the build environment variables.
    fn run_script(
        &self,
        env: &BuildEnvironment,
        script: &str,
        working_dir: &Path,
    ) -> Result<BuildOutput> {
        // Wrap the script body in `set -e` so failures are caught.
        let full_script = format!("set -e\n{script}");

        debug!("executing script in {}", working_dir.display());

        let start = Instant::now();

        // Collect dependency paths from the package store.
        let mut path_parts: Vec<String> = Vec::new();
        let mut pkg_config_parts: Vec<String> = Vec::new();
        let mut include_parts: Vec<String> = Vec::new();
        let mut lib_parts: Vec<String> = Vec::new();

        let store_root = std::env::var("BPKG_STORE_ROOT").unwrap_or_default();
        if !store_root.is_empty() {
            if let Ok(entries) = std::fs::read_dir(&store_root) {
                for entry in entries.flatten() {
                    let pkg = entry.path();

                    // Skip patchelf'd packages built for a different glibc.
                    // These have "-glibc-" in their name and can't be used
                    // for linking on the host.
                    let pkg_name = pkg.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    // Skip patchelf'd packages and the store's glibc — these
                    // are built for a different ABI and would poison the host
                    // linker/loader if added to LD_LIBRARY_PATH or PKG_CONFIG_PATH.
                    if pkg_name.contains("-glibc-")
                        || pkg_name.starts_with("glibc-")
                        || pkg_name.starts_with("gcc-src-")
                        || pkg_name.starts_with("gcc-glibc-")
                        || pkg_name.starts_with("musl-")
                    {
                        continue;
                    }

                    // PATH: <store>/<pkg>/bin
                    let bin_dir = pkg.join("bin");
                    if bin_dir.is_dir() {
                        path_parts.push(bin_dir.to_string_lossy().to_string());
                    }

                    // PKG_CONFIG_PATH: <store>/<pkg>/lib/pkgconfig + share/pkgconfig
                    let pc_lib = pkg.join("lib/pkgconfig");
                    if pc_lib.is_dir() {
                        pkg_config_parts.push(pc_lib.to_string_lossy().to_string());
                    }
                    let pc_lib64 = pkg.join("lib64/pkgconfig");
                    if pc_lib64.is_dir() {
                        pkg_config_parts.push(pc_lib64.to_string_lossy().to_string());
                    }
                    let pc_share = pkg.join("share/pkgconfig");
                    if pc_share.is_dir() {
                        pkg_config_parts.push(pc_share.to_string_lossy().to_string());
                    }

                    // Include paths for CFLAGS/CPPFLAGS
                    let inc_dir = pkg.join("include");
                    if inc_dir.is_dir() {
                        include_parts.push(pkg.to_string_lossy().to_string());
                    }

                    // Library paths for LDFLAGS and linker rpath-link
                    let lib_dir = pkg.join("lib");
                    if lib_dir.is_dir() {
                        lib_parts.push(lib_dir.to_string_lossy().to_string());
                    }
                    let lib64_dir = pkg.join("lib64");
                    if lib64_dir.is_dir() {
                        lib_parts.push(lib64_dir.to_string_lossy().to_string());
                    }
                }
            }
        }

        // Parent PATH goes FIRST — host tools must take priority over
        // store binaries which may be patchelf'd for a different glibc.
        let parent_path = std::env::var("PATH")
            .unwrap_or_else(|_| "/bin:/usr/bin:/sbin:/usr/sbin".to_string());
        let mut final_path_parts = vec![parent_path];
        final_path_parts.extend(path_parts);

        let path = final_path_parts.join(":");
        let pkg_config_path = pkg_config_parts.join(":");

        // Build LDFLAGS with -L and -Wl,-rpath-link for each lib dir
        let ldflags: String = lib_parts
            .iter()
            .map(|d| format!("-L{d} -Wl,-rpath-link,{d}"))
            .collect::<Vec<_>>()
            .join(" ");

        // Try bash first, fall back to sh (busybox environments may not have bash)
        let shell = if std::path::Path::new("/bin/bash").exists()
            && !is_busybox_symlink("/bin/bash")
        {
            "bash"
        } else {
            "sh"
        };

        let output = Command::new(shell)
            .arg("-c")
            .arg(&full_script)
            .current_dir(working_dir)
            .env("PATH", &path)
            .env("SRCDIR", &env.srcdir)
            .env("BUILDDIR", &env.builddir)
            .env("PKGDIR", &env.pkgdir)
            .env("PKG_CONFIG_PATH", &pkg_config_path)
            .env("BPKG_STORE_ROOT", &store_root)
            // LD_LIBRARY_PATH so the linker can resolve transitive shared lib
            // deps during autotools link tests and libtool builds.
            .env("LD_LIBRARY_PATH", &lib_parts.join(":"))
            .output()?;
        let duration = start.elapsed();

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        debug!("script finished in {duration:?} with exit code {exit_code}");

        Ok(BuildOutput {
            exit_code,
            stdout,
            stderr,
            duration,
        })
    }
}

/// Check if a path is a busybox symlink (busybox doesn't support bash).
fn is_busybox_symlink(path: &str) -> bool {
    std::fs::read_link(path)
        .map(|target| target.to_string_lossy().contains("busybox"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn prepare_creates_directories() {
        let tmp = TempDir::new().unwrap();
        let executor = BuildExecutor::new(tmp.path().to_path_buf());
        let env = executor.prepare().unwrap();

        assert!(env.srcdir.is_dir());
        assert!(env.builddir.is_dir());
        assert!(env.pkgdir.is_dir());
    }

    #[test]
    fn run_package_simple_script() {
        let tmp = TempDir::new().unwrap();
        let executor = BuildExecutor::new(tmp.path().to_path_buf());
        let env = executor.prepare().unwrap();

        let script = r#"
mkdir -p "$PKGDIR/bin"
echo '#!/bin/sh' > "$PKGDIR/bin/hello"
echo 'echo hello' >> "$PKGDIR/bin/hello"
chmod +x "$PKGDIR/bin/hello"
"#;

        let output = executor.run_package(&env, script).unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(env.pkgdir.join("bin/hello").exists());
    }

    #[test]
    fn run_package_failing_script() {
        let tmp = TempDir::new().unwrap();
        let executor = BuildExecutor::new(tmp.path().to_path_buf());
        let env = executor.prepare().unwrap();

        let script = "exit 42";

        let result = executor.run_package(&env, script);
        assert!(result.is_err());
        match result.unwrap_err() {
            BuildError::PackageFailed { code, .. } => assert_eq!(code, 42),
            e => panic!("expected PackageFailed, got: {e}"),
        }
    }

    #[test]
    fn run_build_sets_env_vars() {
        let tmp = TempDir::new().unwrap();
        let executor = BuildExecutor::new(tmp.path().to_path_buf());
        let env = executor.prepare().unwrap();

        // Script that writes env vars to files so we can check them.
        let script = r#"
echo "$SRCDIR" > "$BUILDDIR/srcdir.txt"
echo "$BUILDDIR" > "$BUILDDIR/builddir.txt"
echo "$PKGDIR" > "$BUILDDIR/pkgdir.txt"
"#;

        let output = executor.run_build(&env, script).unwrap();
        assert_eq!(output.exit_code, 0);

        let srcdir_val = fs::read_to_string(env.builddir.join("srcdir.txt"))
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(srcdir_val, env.srcdir.to_string_lossy());
    }

    #[test]
    fn run_build_failing_script() {
        let tmp = TempDir::new().unwrap();
        let executor = BuildExecutor::new(tmp.path().to_path_buf());
        let env = executor.prepare().unwrap();

        let result = executor.run_build(&env, "false");
        assert!(result.is_err());
        match result.unwrap_err() {
            BuildError::BuildFailed { code, .. } => assert_eq!(code, 1),
            e => panic!("expected BuildFailed, got: {e}"),
        }
    }

    #[test]
    fn build_output_captures_stdout() {
        let tmp = TempDir::new().unwrap();
        let executor = BuildExecutor::new(tmp.path().to_path_buf());
        let env = executor.prepare().unwrap();

        let output = executor.run_package(&env, "echo 'hello world'").unwrap();
        assert!(output.stdout.contains("hello world"));
    }
}
