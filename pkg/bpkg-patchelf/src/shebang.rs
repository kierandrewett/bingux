use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use bingux_common::BinguxError;

/// A detected shebang and its planned rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShebangRewrite {
    /// Path to the script file.
    pub path: PathBuf,
    /// The original shebang line (including `#!`).
    pub original: String,
    /// The rewritten shebang line.
    pub rewritten: String,
}

/// Detect whether a file starts with a shebang (`#!`).
///
/// Returns the shebang line (first line) if present, or `None` if the file
/// is not a text file with a shebang.
pub fn detect_shebang(path: &Path) -> Result<Option<String>, BinguxError> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();

    match reader.read_line(&mut first_line) {
        Ok(0) => return Ok(None), // empty file
        Ok(_) => {}
        Err(e) => {
            // Binary files will fail UTF-8 reading — that's fine, they're
            // not shebang scripts.
            if e.kind() == std::io::ErrorKind::InvalidData {
                return Ok(None);
            }
            return Err(BinguxError::Io(e));
        }
    }

    let trimmed = first_line.trim_end();
    if trimmed.starts_with("#!") {
        Ok(Some(trimmed.to_string()))
    } else {
        Ok(None)
    }
}

/// Extract the binary name from a shebang line.
///
/// Handles forms like:
/// - `#!/usr/bin/python3`       → `python3`
/// - `#!/usr/bin/env python3`   → `python3`
/// - `#!/usr/bin/env -S python3 -u` → `python3`
pub fn shebang_binary_name(shebang: &str) -> Option<String> {
    let content = shebang.strip_prefix("#!")?;
    let content = content.trim();

    let parts: Vec<&str> = content.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let interpreter = parts[0];
    let basename = Path::new(interpreter)
        .file_name()?
        .to_str()?;

    if basename == "env" {
        // Skip env flags (anything starting with -)
        for part in &parts[1..] {
            if !part.starts_with('-') {
                return Some(part.to_string());
            }
        }
        None
    } else {
        Some(basename.to_string())
    }
}

/// Rewrite a shebang line using the binary-to-store-path mapping.
///
/// `mapping` maps binary names (e.g. `python3`) to their full store path
/// (e.g. `/system/packages/python-3.12-x86_64-linux/bin/python3`).
///
/// Returns `None` if the binary is not in the mapping.
pub fn rewrite_shebang(shebang: &str, mapping: &HashMap<String, PathBuf>) -> Option<String> {
    let binary = shebang_binary_name(shebang)?;
    let store_path = mapping.get(&binary)?;

    // Preserve any arguments after the binary name.
    let content = shebang.strip_prefix("#!")?;
    let content = content.trim();
    let parts: Vec<&str> = content.split_whitespace().collect();

    let basename = Path::new(parts[0])
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let args_start = if basename == "env" {
        // Find where the actual binary is, then take everything after it.
        let mut idx = 1;
        // Skip flags
        while idx < parts.len() && parts[idx].starts_with('-') {
            idx += 1;
        }
        // Skip the binary name itself
        idx + 1
    } else {
        1
    };

    let remaining_args: Vec<&str> = parts[args_start..].to_vec();
    if remaining_args.is_empty() {
        Some(format!("#!{}", store_path.display()))
    } else {
        Some(format!(
            "#!{} {}",
            store_path.display(),
            remaining_args.join(" ")
        ))
    }
}

/// Scan a package directory for scripts with shebangs and compute rewrites.
pub fn scan_shebangs(
    dir: &Path,
    mapping: &HashMap<String, PathBuf>,
) -> Result<Vec<ShebangRewrite>, BinguxError> {
    let mut rewrites = Vec::new();

    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| BinguxError::Io(e.into()))?;
        let path = entry.path();

        // Skip .bpkg metadata
        if path
            .components()
            .any(|c| c.as_os_str() == ".bpkg")
        {
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        if let Some(shebang) = detect_shebang(path)? {
            if let Some(rewritten) = rewrite_shebang(&shebang, mapping) {
                if rewritten != shebang {
                    rewrites.push(ShebangRewrite {
                        path: path.to_path_buf(),
                        original: shebang,
                        rewritten,
                    });
                }
            }
        }
    }

    Ok(rewrites)
}

/// Apply shebang rewrites by modifying the files in place.
pub fn apply_shebang_rewrites(rewrites: &[ShebangRewrite]) -> Result<(), BinguxError> {
    for rewrite in rewrites {
        let content = fs::read_to_string(&rewrite.path)?;
        let new_content = content.replacen(&rewrite.original, &rewrite.rewritten, 1);
        fs::write(&rewrite.path, new_content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_shebang_python() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("script.py");
        fs::write(&path, "#!/usr/bin/python3\nimport sys\n").unwrap();
        assert_eq!(
            detect_shebang(&path).unwrap(),
            Some("#!/usr/bin/python3".to_string())
        );
    }

    #[test]
    fn detect_shebang_env() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("script.py");
        fs::write(&path, "#!/usr/bin/env python3\nimport sys\n").unwrap();
        assert_eq!(
            detect_shebang(&path).unwrap(),
            Some("#!/usr/bin/env python3".to_string())
        );
    }

    #[test]
    fn detect_no_shebang() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.txt");
        fs::write(&path, "hello world\n").unwrap();
        assert_eq!(detect_shebang(&path).unwrap(), None);
    }

    #[test]
    fn extract_binary_name_direct() {
        assert_eq!(
            shebang_binary_name("#!/usr/bin/python3"),
            Some("python3".to_string())
        );
    }

    #[test]
    fn extract_binary_name_env() {
        assert_eq!(
            shebang_binary_name("#!/usr/bin/env python3"),
            Some("python3".to_string())
        );
    }

    #[test]
    fn extract_binary_name_env_with_flags() {
        assert_eq!(
            shebang_binary_name("#!/usr/bin/env -S python3 -u"),
            Some("python3".to_string())
        );
    }

    #[test]
    fn rewrite_shebang_python() {
        let mut mapping = HashMap::new();
        mapping.insert(
            "python3".to_string(),
            PathBuf::from("/system/packages/python-3.12-x86_64-linux/bin/python3"),
        );

        assert_eq!(
            rewrite_shebang("#!/usr/bin/python3", &mapping),
            Some("#!/system/packages/python-3.12-x86_64-linux/bin/python3".to_string())
        );
    }

    #[test]
    fn rewrite_shebang_env_python() {
        let mut mapping = HashMap::new();
        mapping.insert(
            "python3".to_string(),
            PathBuf::from("/system/packages/python-3.12-x86_64-linux/bin/python3"),
        );

        assert_eq!(
            rewrite_shebang("#!/usr/bin/env python3", &mapping),
            Some("#!/system/packages/python-3.12-x86_64-linux/bin/python3".to_string())
        );
    }

    #[test]
    fn rewrite_shebang_with_args() {
        let mut mapping = HashMap::new();
        mapping.insert(
            "python3".to_string(),
            PathBuf::from("/system/packages/python-3.12-x86_64-linux/bin/python3"),
        );

        assert_eq!(
            rewrite_shebang("#!/usr/bin/env -S python3 -u", &mapping),
            Some("#!/system/packages/python-3.12-x86_64-linux/bin/python3 -u".to_string())
        );
    }

    #[test]
    fn rewrite_unknown_binary() {
        let mapping = HashMap::new();
        assert_eq!(rewrite_shebang("#!/usr/bin/perl", &mapping), None);
    }

    #[test]
    fn scan_and_apply_shebangs() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a script with a shebang
        let script = tmp.path().join("bin/run.py");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, "#!/usr/bin/python3\nprint('hello')\n").unwrap();

        // Create a non-script file
        fs::write(tmp.path().join("data.txt"), "just data").unwrap();

        let mut mapping = HashMap::new();
        mapping.insert(
            "python3".to_string(),
            PathBuf::from("/system/packages/python-3.12-x86_64-linux/bin/python3"),
        );

        let rewrites = scan_shebangs(tmp.path(), &mapping).unwrap();
        assert_eq!(rewrites.len(), 1);
        assert_eq!(rewrites[0].original, "#!/usr/bin/python3");
        assert_eq!(
            rewrites[0].rewritten,
            "#!/system/packages/python-3.12-x86_64-linux/bin/python3"
        );

        // Apply the rewrites
        apply_shebang_rewrites(&rewrites).unwrap();
        let content = fs::read_to_string(&script).unwrap();
        assert!(content.starts_with("#!/system/packages/python-3.12"));
        assert!(content.contains("print('hello')"));
    }
}
