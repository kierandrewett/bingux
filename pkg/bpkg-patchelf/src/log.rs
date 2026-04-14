use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use bingux_common::{BinguxError, SystemPaths};

use crate::planner::PatchPlan;
use crate::shebang::ShebangRewrite;

/// Generate the contents of a patchelf log file.
///
/// Records all ELF patches and shebang rewrites that were applied (or planned)
/// for a package.
pub fn generate_log(
    plan: &PatchPlan,
    shebang_rewrites: &[ShebangRewrite],
) -> String {
    let mut log = String::new();

    writeln!(log, "# bpkg-patchelf log").unwrap();
    writeln!(log, "# package: {}", plan.package_dir.display()).unwrap();
    writeln!(log).unwrap();

    // ELF patches
    let effective = plan.effective_patches();
    writeln!(log, "## ELF patches ({} files)", effective.len()).unwrap();
    writeln!(log).unwrap();

    for patch in &effective {
        writeln!(log, "file: {}", patch.path.display()).unwrap();

        if let (Some(old), Some(new)) = (&patch.old_interpreter, &patch.new_interpreter) {
            if old != new {
                writeln!(log, "  interpreter: {old} -> {new}").unwrap();
            }
        } else if let (None, Some(new)) = (&patch.old_interpreter, &patch.new_interpreter) {
            writeln!(log, "  interpreter: (none) -> {new}").unwrap();
        }

        if let Some(new_rp) = &patch.new_runpath {
            let old_rp = patch.old_runpath.as_deref().unwrap_or("(none)");
            if patch.old_runpath.as_deref() != Some(new_rp.as_str()) {
                writeln!(log, "  runpath: {old_rp} -> {new_rp}").unwrap();
            }
        }

        if !patch.needed.is_empty() {
            writeln!(log, "  needed: {}", patch.needed.join(", ")).unwrap();
        }

        writeln!(log).unwrap();
    }

    // Shebang rewrites
    if !shebang_rewrites.is_empty() {
        writeln!(
            log,
            "## Shebang rewrites ({} files)",
            shebang_rewrites.len()
        )
        .unwrap();
        writeln!(log).unwrap();

        for rewrite in shebang_rewrites {
            writeln!(log, "file: {}", rewrite.path.display()).unwrap();
            writeln!(log, "  shebang: {} -> {}", rewrite.original, rewrite.rewritten).unwrap();
            writeln!(log).unwrap();
        }
    }

    log
}

/// Write the patchelf log to the package's `.bpkg/` metadata directory.
pub fn write_log(
    package_dir: &Path,
    plan: &PatchPlan,
    shebang_rewrites: &[ShebangRewrite],
) -> Result<(), BinguxError> {
    let meta_dir = package_dir.join(SystemPaths::BPKG_META_DIR);
    fs::create_dir_all(&meta_dir)?;

    let log_path = meta_dir.join(SystemPaths::PATCHELF_LOG_FILENAME);
    let content = generate_log(plan, shebang_rewrites);
    fs::write(log_path, content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::ElfPatch;
    use std::path::PathBuf;

    fn sample_plan() -> PatchPlan {
        PatchPlan {
            package_dir: PathBuf::from("/system/packages/hello-1.0-x86_64-linux"),
            elf_patches: vec![
                ElfPatch {
                    path: PathBuf::from("/system/packages/hello-1.0-x86_64-linux/bin/hello"),
                    old_interpreter: Some("/lib64/ld-linux-x86-64.so.2".to_string()),
                    new_interpreter: Some(
                        "/system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2"
                            .to_string(),
                    ),
                    old_runpath: None,
                    new_runpath: Some(
                        "/system/packages/hello-1.0-x86_64-linux/lib:\
                         /system/packages/glibc-2.39-x86_64-linux/lib"
                            .to_string(),
                    ),
                    needed: vec!["libc.so.6".to_string()],
                },
                // An unchanged binary (should not appear in log)
                ElfPatch {
                    path: PathBuf::from("/system/packages/hello-1.0-x86_64-linux/bin/unchanged"),
                    old_interpreter: Some("/same/interp".to_string()),
                    new_interpreter: Some("/same/interp".to_string()),
                    old_runpath: Some("/same/rpath".to_string()),
                    new_runpath: Some("/same/rpath".to_string()),
                    needed: vec![],
                },
            ],
        }
    }

    fn sample_shebangs() -> Vec<ShebangRewrite> {
        vec![ShebangRewrite {
            path: PathBuf::from("/system/packages/hello-1.0-x86_64-linux/bin/wrapper.py"),
            original: "#!/usr/bin/python3".to_string(),
            rewritten: "#!/system/packages/python-3.12-x86_64-linux/bin/python3".to_string(),
        }]
    }

    #[test]
    fn log_format() {
        let plan = sample_plan();
        let shebangs = sample_shebangs();
        let log = generate_log(&plan, &shebangs);

        assert!(log.contains("# bpkg-patchelf log"));
        assert!(log.contains("hello-1.0-x86_64-linux"));
        assert!(log.contains("## ELF patches (1 files)"));
        assert!(log.contains("interpreter: /lib64/ld-linux-x86-64.so.2 ->"));
        assert!(log.contains("runpath: (none) ->"));
        assert!(log.contains("needed: libc.so.6"));
        // Unchanged binary should not appear
        assert!(!log.contains("unchanged"));
        // Shebang section
        assert!(log.contains("## Shebang rewrites (1 files)"));
        assert!(log.contains("shebang: #!/usr/bin/python3 ->"));
    }

    #[test]
    fn log_no_shebangs() {
        let plan = sample_plan();
        let log = generate_log(&plan, &[]);
        assert!(!log.contains("## Shebang rewrites"));
    }

    #[test]
    fn write_log_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = PatchPlan {
            package_dir: tmp.path().to_path_buf(),
            elf_patches: vec![],
        };

        write_log(tmp.path(), &plan, &[]).unwrap();

        let log_path = tmp.path().join(".bpkg/patchelf.log");
        assert!(log_path.exists());
        let content = fs::read_to_string(log_path).unwrap();
        assert!(content.contains("# bpkg-patchelf log"));
    }
}
