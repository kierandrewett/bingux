//! The shape of the code base (`docs/browser/structure.md`, "Rules"): features are folders that
//! show a public API and hide the rest, state has behaviour on its type, and the engine's
//! vocabulary stays behind `engine/`. These tests read the source, so a change that breaks a
//! rule fails here before it is reviewed.

use std::fs;
use std::path::{Path, PathBuf};

fn sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = fs::read_to_string(&path).unwrap();
                out.push((path, text));
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    walk(&root, &mut out);
    out.sort();
    out
}

/// The path of `file` below `src/`, with `/` between its parts.
fn relative(file: &Path) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    file.strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

/// Whether `relative` is a file of tests, or of what only a test or a snapshot build uses.
fn is_test_code(relative: &str) -> bool {
    relative.ends_with("tests.rs")
        || relative.ends_with("structure_tests.rs")
        || relative.ends_with("testing.rs")
        || relative.ends_with("/snapshot.rs")
        || relative.starts_with("snapshot/")
}

/// The part of `text` before its test module.
fn before_tests(text: &str) -> &str {
    text.find("\n#[cfg(test)]\nmod tests")
        .map_or(text, |end| &text[..end])
}

#[test]
fn a_feature_shows_its_api_and_hides_its_submodules() {
    for (file, text) in sources() {
        let relative = relative(&file);
        if !relative.ends_with("/mod.rs") {
            continue;
        }
        for (number, line) in before_tests(&text).lines().enumerate() {
            assert!(
                !line.starts_with("pub mod "),
                "{relative}:{}: `pub mod`: a feature's submodules are private; `pub use` what it shows",
                number + 1
            );
        }
    }
}

/// Files whose job is free functions: the entry point and the registration of the catalogue.
const FREE_FUNCTION_FILES: &[&str] = &["main.rs", "locale.rs"];

#[test]
fn behaviour_is_in_impl_blocks_of_the_type_it_is_about() {
    for (file, text) in sources() {
        let relative = relative(&file);
        if is_test_code(&relative) || FREE_FUNCTION_FILES.contains(&relative.as_str()) {
            continue;
        }
        for (number, line) in before_tests(&text).lines().enumerate() {
            let free = [
                "fn ",
                "pub fn ",
                "pub(crate) fn ",
                "pub(super) fn ",
                "const fn ",
                "pub const fn ",
            ]
            .iter()
            .any(|start| line.starts_with(start));
            assert!(
                !free,
                "{relative}:{}: a free function `{}`: make it a method of the type it is about",
                number + 1,
                line.trim()
            );
        }
    }
}

#[test]
fn only_the_engine_speaks_to_the_backend() {
    for (file, text) in sources() {
        let relative = relative(&file);
        if is_test_code(&relative) || relative.starts_with("engine/") {
            continue;
        }
        let code = before_tests(&text);
        for name in ["BrowserBackend", "BrowserEvent", "TabEvent"] {
            assert!(
                !code.contains(name),
                "{relative}: names `{name}`; only `engine/` speaks to the backend"
            );
        }
    }
}
