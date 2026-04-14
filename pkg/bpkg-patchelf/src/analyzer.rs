use std::fs;
use std::path::{Path, PathBuf};

use bingux_common::BinguxError;

/// Detailed ELF analysis results for a single binary.
#[derive(Debug, Clone)]
pub struct ElfAnalysis {
    /// Path to the ELF file.
    pub path: PathBuf,
    /// Current PT_INTERP (dynamic linker path), if present.
    pub interpreter: Option<String>,
    /// NEEDED entries (required shared libraries).
    pub needed: Vec<String>,
    /// Current RUNPATH, if set.
    pub runpath: Option<String>,
    /// Current RPATH (legacy), if set.
    pub rpath: Option<String>,
    /// Whether this is a dynamically linked binary.
    pub is_dynamic: bool,
}

/// Analyze an ELF binary and extract dynamic linking information.
pub fn analyze_elf(path: &Path) -> Result<ElfAnalysis, BinguxError> {
    let data = fs::read(path)?;
    let elf = goblin::elf::Elf::parse(&data).map_err(|e| BinguxError::ElfParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let interpreter = elf.interpreter.map(|s| s.to_string());

    let needed: Vec<String> = elf
        .libraries
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Extract RUNPATH and RPATH from dynamic section.
    let mut runpath = None;
    let mut rpath = None;

    if let Some(ref dynamic) = elf.dynamic {
        // goblin provides runpath and rpath fields on the Dynamic struct.
        // We need to look through the dynamic entries manually.
        use goblin::elf::dynamic::{DT_RPATH, DT_RUNPATH};
        for dyn_entry in &dynamic.dyns {
            match dyn_entry.d_tag {
                DT_RUNPATH => {
                    if let Some(val) = elf.dynstrtab.get_at(dyn_entry.d_val as usize) {
                        runpath = Some(val.to_string());
                    }
                }
                DT_RPATH => {
                    if let Some(val) = elf.dynstrtab.get_at(dyn_entry.d_val as usize) {
                        rpath = Some(val.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    let is_dynamic = elf.dynamic.is_some();

    Ok(ElfAnalysis {
        path: path.to_path_buf(),
        interpreter,
        needed,
        runpath,
        rpath,
        is_dynamic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test analysis of a real system binary if available.
    #[test]
    fn analyze_real_binary() {
        let path = Path::new("/usr/bin/true");
        if !path.exists() {
            eprintln!("skipping: /usr/bin/true not found");
            return;
        }

        let analysis = analyze_elf(path).unwrap();

        // /usr/bin/true should be dynamically linked on most systems.
        assert!(analysis.is_dynamic);

        // It should have a PT_INTERP.
        assert!(analysis.interpreter.is_some());
        let interp = analysis.interpreter.as_ref().unwrap();
        assert!(
            interp.contains("ld-linux") || interp.contains("ld.so"),
            "unexpected interpreter: {interp}"
        );

        // It should have at least libc in NEEDED.
        assert!(
            analysis.needed.iter().any(|n| n.contains("libc")),
            "expected libc in NEEDED, got: {:?}",
            analysis.needed
        );
    }

    /// Test analysis of a shared library if available.
    #[test]
    fn analyze_shared_library() {
        // Try to find libc.so.6
        let candidates = [
            "/usr/lib64/libc.so.6",
            "/usr/lib/x86_64-linux-gnu/libc.so.6",
            "/lib64/libc.so.6",
            "/lib/x86_64-linux-gnu/libc.so.6",
        ];

        let path = candidates.iter().map(Path::new).find(|p| p.exists());
        let Some(path) = path else {
            eprintln!("skipping: no libc.so.6 found");
            return;
        };

        let analysis = analyze_elf(path).unwrap();
        assert!(analysis.is_dynamic);
        // libc itself typically has no PT_INTERP (it's a shared library).
        // Actually on some systems libc does have an interpreter for direct execution.
        // So we just verify the analysis succeeds.
    }
}
