use crate::output;
use std::path::Path;

pub fn run(package: Option<&str>, all: bool, index: Option<&Path>) {
    if let Some(dir) = index {
        output::status("export", &format!("would generate index.toml in {}", dir.display()));
    } else if all {
        output::status("export", "would export all packages as .bgx");
    } else if let Some(pkg) = package {
        output::status("export", &format!("would export {pkg} as .bgx"));
    } else {
        output::status("export", "nothing to export — specify a package, --all, or --index");
    }
}
