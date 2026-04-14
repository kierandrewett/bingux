use std::path::PathBuf;

use crate::output;

use bsys_compose::GenerationBuilder;
use bsys_config::{EtcGenerator, parse_system_config};

/// Default system config path.
fn default_system_toml() -> PathBuf {
    std::env::var("BSYS_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/config/system.toml"))
}

fn default_profiles_root() -> PathBuf {
    std::env::var("BSYS_PROFILES_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/profiles"))
}

fn default_packages_root() -> PathBuf {
    std::env::var("BSYS_PACKAGES_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

pub fn apply() {
    let config_path = default_system_toml();
    output::status("apply", &format!("reading system config from {}", config_path.display()));

    match parse_system_config(&config_path) {
        Ok(config) => {
            output::status("apply", &format!(
                "hostname={}, locale={}, timezone={}, keymap={}",
                config.system.hostname, config.system.locale,
                config.system.timezone, config.system.keymap
            ));

            // Show what /etc/ files would be generated.
            let etc_gen = EtcGenerator::new(PathBuf::from("/etc"));
            match etc_gen.generate_all(&config) {
                Ok(files) => {
                    for f in &files {
                        output::status("etc", &format!("would write {}", f.path.display()));
                    }
                }
                Err(e) => {
                    output::status("error", &format!("etc generation failed: {e}"));
                }
            }

            // Build a new system generation.
            let profiles = default_profiles_root();
            let packages = default_packages_root();
            let builder = GenerationBuilder::new(profiles, packages);

            // TODO: build PackageEntry list from config.packages.keep
            match builder.build(&[]) {
                Ok(built) => {
                    if let Err(e) = builder.activate(built.id) {
                        output::status("error", &format!("activation failed: {e}"));
                    } else {
                        output::status("apply", &format!("system profile recomposed (generation {})", built.id));
                    }
                }
                Err(e) => {
                    output::status("error", &format!("build failed: {e}"));
                }
            }
        }
        Err(e) => {
            output::status("error", &format!("failed to read system config: {e}"));
        }
    }
}

pub fn rollback(generation: Option<u64>) {
    let profiles = default_profiles_root();
    let packages = default_packages_root();
    let builder = GenerationBuilder::new(profiles, packages);

    match generation {
        Some(g) => {
            if let Err(e) = builder.rollback(g) {
                output::status("error", &format!("rollback failed: {e}"));
            } else {
                output::status("rollback", &format!("rolled back to generation {g}"));
            }
        }
        None => {
            match builder.current() {
                Ok(Some(id)) if id > 1 => {
                    if let Err(e) = builder.rollback(id - 1) {
                        output::status("error", &format!("rollback failed: {e}"));
                    } else {
                        output::status("rollback", &format!("rolled back to generation {}", id - 1));
                    }
                }
                Ok(Some(_)) => output::status("rollback", "already at generation 1"),
                Ok(None) => output::status("rollback", "no active generation"),
                Err(e) => output::status("error", &format!("failed to read current generation: {e}")),
            }
        }
    }
}

pub fn history() {
    let profiles = default_profiles_root();
    let packages = default_packages_root();
    let builder = GenerationBuilder::new(profiles, packages);

    match builder.list_generations() {
        Ok(generations) => {
            if generations.is_empty() {
                output::status("history", "no system generations found");
                return;
            }
            let current_id = builder.current().ok().flatten().unwrap_or(0);
            for g in generations.iter().rev() {
                let marker = if g.id == current_id { " (current)" } else { "" };
                output::status("history", &format!(
                    "generation {} — {} packages{marker}",
                    g.id, g.packages.len()
                ));
            }
        }
        Err(e) => {
            output::status("error", &format!("failed to list generations: {e}"));
        }
    }
}

pub fn diff(gen1: u64, gen2: u64) {
    output::status("diff", &format!("would diff generations {gen1} and {gen2}"));
}

pub fn upgrade(package: Option<&str>, all: bool) {
    if all {
        output::status("upgrade", "would upgrade all packages");
    } else if let Some(pkg) = package {
        output::status("upgrade", &format!("would upgrade {pkg}"));
    } else {
        output::status("upgrade", "would interactively select packages to upgrade");
    }
}
