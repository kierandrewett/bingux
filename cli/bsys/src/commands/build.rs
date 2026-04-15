use std::path::PathBuf;

use crate::output;

use bpkg_build::{BuildConfig, BuildPipeline};
use bpkg_recipe::parse_recipe;

/// Default recipes directory.
fn default_recipes_dir() -> PathBuf {
    std::env::var("BSYS_RECIPES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/recipes"))
}

fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

pub fn run(recipe: &str) {
    // Recipe can be a path to a BPKGBUILD file or a recipe name
    let recipe_path = if recipe.contains('/') || recipe.ends_with("BPKGBUILD") {
        PathBuf::from(recipe)
    } else {
        let recipes_dir = default_recipes_dir();
        recipes_dir.join(recipe).join("BPKGBUILD")
    };

    if !recipe_path.exists() {
        output::status("error", &format!("recipe not found: {}", recipe_path.display()));
        return;
    }

    // Parse first to show what we're building
    match std::fs::read_to_string(&recipe_path) {
        Ok(content) => {
            match parse_recipe(&content) {
                Ok(parsed) => {
                    output::status("build", &format!(
                        "{} {} ({})", parsed.pkgname, parsed.pkgver, parsed.pkgarch
                    ));
                    if !parsed.depends.is_empty() {
                        output::status("deps", &parsed.depends.join(", "));
                    }
                }
                Err(e) => {
                    output::status("error", &format!("recipe parse failed: {e}"));
                    return;
                }
            }
        }
        Err(e) => {
            output::status("error", &format!("failed to read recipe: {e}"));
            return;
        }
    }

    // Run the full build pipeline
    let store_root = default_store_root();
    let work_dir = std::env::var("BSYS_WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/bsys-build"));
    let cache_dir = std::env::var("BSYS_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/bsys-cache"));

    // Ensure dirs exist
    let _ = std::fs::create_dir_all(&work_dir);
    let _ = std::fs::create_dir_all(&cache_dir);

    let config = BuildConfig {
        recipe_path: recipe_path.clone(),
        store_root: store_root.clone(),
        work_dir,
        source_cache: cache_dir,
        arch: "x86_64-linux".to_string(),
        network_fetch: true,
    };

    let pipeline = BuildPipeline::new(config);

    // Use a simple blocking runtime for the async build
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            output::status("error", &format!("failed to create runtime: {e}"));
            return;
        }
    };

    match rt.block_on(pipeline.build(&recipe_path)) {
        Ok(result) => {
            output::status("ok", &format!(
                "built {} → {}",
                result.package_id,
                result.store_path.display()
            ));
            output::status("ok", &format!(
                "{} files, {:?}",
                result.files_count,
                result.build_duration
            ));
        }
        Err(e) => {
            output::status("error", &format!("build failed: {e}"));
        }
    }
}
