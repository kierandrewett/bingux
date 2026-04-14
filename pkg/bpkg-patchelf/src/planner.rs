use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use bingux_common::BinguxError;

use crate::analyzer;
use crate::scanner::{self, ElfKind, ScannedElf};

/// A single planned change to an ELF binary.
#[derive(Debug, Clone)]
pub struct ElfPatch {
    /// Path to the ELF file to patch.
    pub path: PathBuf,
    /// Current interpreter, if any.
    pub old_interpreter: Option<String>,
    /// New interpreter to set, if it needs changing.
    pub new_interpreter: Option<String>,
    /// Current RUNPATH/RPATH.
    pub old_runpath: Option<String>,
    /// New RUNPATH to set, if it needs changing.
    pub new_runpath: Option<String>,
    /// NEEDED libraries for reference/logging.
    pub needed: Vec<String>,
}

impl ElfPatch {
    /// Whether this patch actually changes anything.
    pub fn has_changes(&self) -> bool {
        let interp_changed = match (&self.old_interpreter, &self.new_interpreter) {
            (Some(old), Some(new)) => old != new,
            (None, Some(_)) => true,
            _ => false,
        };
        let runpath_changed = match (&self.old_runpath, &self.new_runpath) {
            (Some(old), Some(new)) => old != new,
            (None, Some(_)) => true,
            _ => false,
        };
        interp_changed || runpath_changed
    }
}

impl fmt::Display for ElfPatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  {}", self.path.display())?;
        if let (Some(old), Some(new)) = (&self.old_interpreter, &self.new_interpreter) {
            if old != new {
                writeln!(f, "    PT_INTERP: {old} -> {new}")?;
            }
        } else if let (None, Some(new)) = (&self.old_interpreter, &self.new_interpreter) {
            writeln!(f, "    PT_INTERP: (none) -> {new}")?;
        }
        if let (ref old, Some(new)) = (&self.old_runpath, &self.new_runpath) {
            let old_display = old.as_deref().unwrap_or("(none)");
            if old.as_deref() != Some(new.as_str()) {
                writeln!(f, "    RUNPATH: {old_display} -> {new}")?;
            }
        }
        Ok(())
    }
}

/// The complete patch plan for a package.
#[derive(Debug, Clone)]
pub struct PatchPlan {
    /// The package directory being patched.
    pub package_dir: PathBuf,
    /// Individual ELF patches.
    pub elf_patches: Vec<ElfPatch>,
}

impl PatchPlan {
    /// Compute a patch plan for a package directory.
    ///
    /// - `package_dir`: the root of the unpacked package
    /// - `new_interpreter`: the store path to the dynamic linker
    /// - `new_runpath`: the computed RUNPATH from bpkg-resolve
    pub fn compute(
        package_dir: &Path,
        new_interpreter: &str,
        new_runpath: &str,
    ) -> Result<Self, BinguxError> {
        let scan = scanner::scan_package_dir(package_dir)?;
        let mut elf_patches = Vec::new();

        for ScannedElf { path, kind } in &scan.elfs {
            let analysis = analyzer::analyze_elf(path)?;

            // Only patch dynamically linked files.
            if !analysis.is_dynamic {
                continue;
            }

            let new_interp = match kind {
                ElfKind::Executable => Some(new_interpreter.to_string()),
                // Shared libraries don't get PT_INTERP set.
                ElfKind::SharedLibrary | ElfKind::Other => None,
            };

            elf_patches.push(ElfPatch {
                path: path.clone(),
                old_interpreter: analysis.interpreter,
                new_interpreter: new_interp,
                old_runpath: analysis.runpath.or(analysis.rpath),
                new_runpath: Some(new_runpath.to_string()),
                needed: analysis.needed,
            });
        }

        Ok(Self {
            package_dir: package_dir.to_path_buf(),
            elf_patches,
        })
    }

    /// Return only patches that actually change something.
    pub fn effective_patches(&self) -> Vec<&ElfPatch> {
        self.elf_patches.iter().filter(|p| p.has_changes()).collect()
    }

    /// Apply the patch plan by shelling out to the `patchelf` command.
    ///
    /// Returns an error if the `patchelf` binary is not found on PATH.
    pub fn apply(&self) -> Result<(), BinguxError> {
        // Verify patchelf is available.
        let status = Command::new("which")
            .arg("patchelf")
            .output()
            .map_err(|e| BinguxError::PatchelfFailed {
                path: self.package_dir.clone(),
                message: format!("failed to check for patchelf: {e}"),
            })?;

        if !status.status.success() {
            return Err(BinguxError::PatchelfFailed {
                path: self.package_dir.clone(),
                message: "patchelf binary not found on PATH".to_string(),
            });
        }

        for patch in self.effective_patches() {
            if let Some(ref new_interp) = patch.new_interpreter {
                let output = Command::new("patchelf")
                    .arg("--set-interpreter")
                    .arg(new_interp)
                    .arg(&patch.path)
                    .output()
                    .map_err(|e| BinguxError::PatchelfFailed {
                        path: patch.path.clone(),
                        message: format!("failed to run patchelf --set-interpreter: {e}"),
                    })?;

                if !output.status.success() {
                    return Err(BinguxError::PatchelfFailed {
                        path: patch.path.clone(),
                        message: format!(
                            "patchelf --set-interpreter failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        ),
                    });
                }
            }

            if let Some(ref new_runpath) = patch.new_runpath {
                let output = Command::new("patchelf")
                    .arg("--set-rpath")
                    .arg(new_runpath)
                    .arg(&patch.path)
                    .output()
                    .map_err(|e| BinguxError::PatchelfFailed {
                        path: patch.path.clone(),
                        message: format!("failed to run patchelf --set-rpath: {e}"),
                    })?;

                if !output.status.success() {
                    return Err(BinguxError::PatchelfFailed {
                        path: patch.path.clone(),
                        message: format!(
                            "patchelf --set-rpath failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

impl fmt::Display for PatchPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Patch plan for {}", self.package_dir.display())?;
        let effective = self.effective_patches();
        writeln!(f, "{} binaries to patch:", effective.len())?;
        for patch in &effective {
            write!(f, "{patch}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Test computing a patch plan against a real system binary directory.
    #[test]
    fn compute_plan_with_real_binary() {
        // Create a temporary package dir with a copy of /usr/bin/true
        let src = Path::new("/usr/bin/true");
        if !src.exists() {
            eprintln!("skipping: /usr/bin/true not found");
            return;
        }

        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::copy(src, bin.join("true")).unwrap();

        let plan = PatchPlan::compute(
            tmp.path(),
            "/system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2",
            "/system/packages/mypkg-1.0-x86_64-linux/lib:/system/packages/glibc-2.39-x86_64-linux/lib",
        )
        .unwrap();

        assert!(!plan.elf_patches.is_empty());
        let patch = &plan.elf_patches[0];
        assert_eq!(
            patch.new_interpreter.as_deref(),
            Some("/system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2")
        );
        assert!(patch.new_runpath.as_ref().unwrap().contains("mypkg-1.0"));
        assert!(patch.has_changes());
    }

    #[test]
    fn empty_package_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = PatchPlan::compute(tmp.path(), "/lib/ld.so", "/lib").unwrap();
        assert!(plan.elf_patches.is_empty());
        assert!(plan.effective_patches().is_empty());
    }

    #[test]
    fn patch_display_format() {
        let patch = ElfPatch {
            path: PathBuf::from("/pkg/bin/hello"),
            old_interpreter: Some("/lib64/ld-linux-x86-64.so.2".to_string()),
            new_interpreter: Some("/system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2".to_string()),
            old_runpath: None,
            new_runpath: Some("/system/packages/hello-1.0-x86_64-linux/lib".to_string()),
            needed: vec!["libc.so.6".to_string()],
        };
        let text = format!("{patch}");
        assert!(text.contains("PT_INTERP:"));
        assert!(text.contains("RUNPATH:"));
        assert!(text.contains("(none) ->"));
    }
}
