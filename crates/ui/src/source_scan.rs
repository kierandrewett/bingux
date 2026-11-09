//! Source scans for the rules the API cannot enforce by its types. Test-only.

use std::fs;
use std::path::{Path, PathBuf};

/// The non-test code of every Rust file in the workspace, as
/// `(path from the workspace root, code)`.
pub(crate) fn workspace_code() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for domain in ["apps", "crates", "services", "bsm", "tools"] {
        sources(&root.join(domain), &mut files);
    }
    assert!(files.len() > 100, "the workspace sources were found");
    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let text = fs::read_to_string(&path).unwrap_or_default();
            (relative, code(&text).to_owned())
        })
        .collect()
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "target" && name != "vendor" {
                sources(&path, out);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// The code of a file, without its test module (the first `#[cfg(test)]`
/// or `#[cfg(all(test`).
fn code(text: &str) -> &str {
    let cut = ["#[cfg(test)]", "#[cfg(all(test"]
        .iter()
        .filter_map(|marker| text.find(marker))
        .min()
        .unwrap_or(text.len());
    &text[..cut]
}
