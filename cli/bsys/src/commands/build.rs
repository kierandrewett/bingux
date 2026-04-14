use std::path::PathBuf;

use crate::output;

use bpkg_recipe::parse_recipe;

/// Default recipes directory.
fn default_recipes_dir() -> PathBuf {
    std::env::var("BSYS_RECIPES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/recipes"))
}

pub fn run(recipe: &str) {
    let recipes_dir = default_recipes_dir();
    let recipe_path = recipes_dir.join(recipe).join("BPKGBUILD");

    if !recipe_path.exists() {
        output::status("build", &format!("recipe file not found: {}", recipe_path.display()));
        return;
    }

    match std::fs::read_to_string(&recipe_path) {
        Ok(content) => {
            match parse_recipe(&content) {
                Ok(parsed) => {
                    output::status("build", &format!("recipe: {} {}", parsed.pkgname, parsed.pkgver));
                    if !parsed.makedepends.is_empty() {
                        output::status("build", &format!(
                            "build deps: {}", parsed.makedepends.join(", ")
                        ));
                    }
                    if !parsed.depends.is_empty() {
                        output::status("build", &format!(
                            "runtime deps: {}", parsed.depends.join(", ")
                        ));
                    }
                    let has_build = parsed.build.is_some();
                    let has_package = parsed.package.is_some();
                    output::status("build", &format!(
                        "build plan: build()={}, package()={}", has_build, has_package
                    ));
                }
                Err(e) => {
                    output::status("error", &format!("recipe parse failed: {e}"));
                }
            }
        }
        Err(e) => {
            output::status("error", &format!("failed to read recipe: {e}"));
        }
    }
}
