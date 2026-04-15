use std::path::PathBuf;

use crate::output;

use bsys_compose::GenerationBuilder;
use bpkg_store;
use bxc_sandbox;
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

            // Generate /etc/ configuration files from the system config.
            let etc_root = std::env::var("BSYS_ETC_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/etc"));
            let etc_gen = EtcGenerator::new(etc_root);
            match etc_gen.generate_all(&config) {
                Ok(files) => {
                    for f in &files {
                        output::status("etc", &format!("generated {}", f.path.display()));
                    }
                    output::status("etc", &format!("{} files written", files.len()));
                }
                Err(e) => {
                    output::status("error", &format!("etc generation failed: {e}"));
                }
            }

            // Build a new system generation.
            let profiles = default_profiles_root();
            let packages = default_packages_root();
            let builder = GenerationBuilder::new(profiles, packages);

            // Build PackageEntry list from config.packages.keep
            let packages_root = default_packages_root();
            let mut entries = Vec::new();
            for pkg_name in &config.packages.keep {
                // Find matching package in store
                let store = bpkg_store::PackageStore::new(packages_root.clone());
                if let Ok(store) = store {
                    let matching = store.query(pkg_name);
                    if let Some(pkg_id) = matching.into_iter().next() {
                        // Read manifest for exports; fall back gracefully if
                        // no manifest exists (common for packages that were
                        // built without an [exports] section).
                        let mut exports = bsys_compose::generation::ExportedItems {
                            binaries: vec![],
                            libraries: vec![],
                            data: vec![],
                        };
                        if let Ok(manifest) = store.manifest(&pkg_id) {
                            exports.binaries = manifest.exports.binaries.clone();
                            exports.libraries = manifest.exports.libraries.clone();
                            exports.data = manifest.exports.data.clone();
                        }

                        // Auto-discover: when no binaries are declared in the
                        // manifest (or the manifest is absent), scan the
                        // package's bin/ directory and export every file or
                        // symlink found there.
                        if exports.binaries.is_empty() {
                            let bin_dir = packages_root.join(pkg_id.dir_name()).join("bin");
                            if bin_dir.is_dir() {
                                if let Ok(rd) = std::fs::read_dir(&bin_dir) {
                                    for entry in rd.flatten() {
                                        let ft = entry.file_type();
                                        if let Ok(ft) = ft {
                                            if ft.is_file() || ft.is_symlink() {
                                                if let Some(name) = entry.file_name().to_str() {
                                                    exports.binaries.push(format!("bin/{name}"));
                                                }
                                            }
                                        }
                                    }
                                    exports.binaries.sort();
                                }
                                if !exports.binaries.is_empty() {
                                    output::status("discover", &format!(
                                        "{}: auto-discovered {} binaries from bin/",
                                        pkg_name, exports.binaries.len()
                                    ));
                                }
                            }
                        }

                        let sandbox_level = bxc_sandbox::levels::SandboxLevel::Minimal;
                        entries.push(bsys_compose::generation::PackageEntry {
                            package_id: pkg_id,
                            sandbox_level,
                            exports,
                        });
                    }
                }
            }
            match builder.build(&entries) {
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
    let profiles = default_profiles_root();
    let gen1_dir = profiles.join(gen1.to_string());
    let gen2_dir = profiles.join(gen2.to_string());

    if !gen1_dir.exists() {
        output::status("error", &format!("generation {gen1} not found"));
        return;
    }
    if !gen2_dir.exists() {
        output::status("error", &format!("generation {gen2} not found"));
        return;
    }

    // Read generation.toml from each
    let gen1_meta = std::fs::read_to_string(gen1_dir.join("generation.toml")).unwrap_or_default();
    let gen2_meta = std::fs::read_to_string(gen2_dir.join("generation.toml")).unwrap_or_default();

    // Extract package lists
    let gen1_pkgs: std::collections::HashSet<String> = gen1_meta.lines()
        .filter(|l| l.starts_with("id = \""))
        .map(|l| l.trim_start_matches("id = \"").trim_end_matches('"').to_string())
        .collect();
    let gen2_pkgs: std::collections::HashSet<String> = gen2_meta.lines()
        .filter(|l| l.starts_with("id = \""))
        .map(|l| l.trim_start_matches("id = \"").trim_end_matches('"').to_string())
        .collect();

    output::status("diff", &format!("generation {gen1} → {gen2}"));

    // Added packages
    for pkg in gen2_pkgs.difference(&gen1_pkgs) {
        output::status("+", pkg);
    }

    // Removed packages
    for pkg in gen1_pkgs.difference(&gen2_pkgs) {
        output::status("-", pkg);
    }

    // Unchanged
    let unchanged = gen1_pkgs.intersection(&gen2_pkgs).count();
    if unchanged > 0 {
        output::status("=", &format!("{unchanged} packages unchanged"));
    }

    if gen1_pkgs == gen2_pkgs {
        output::status("diff", "no package changes between generations");
    }
}

pub fn upgrade(package: Option<&str>, all: bool) {
    let store_root = std::env::var("BPKG_STORE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/system/packages"));

    if all {
        output::status("upgrade", "checking all packages...");
        if let Ok(store) = bpkg_store::PackageStore::new(store_root) {
            let packages = store.list();
            for pkg_id in &packages {
                let versions = store.query(&pkg_id.name);
                output::status("upgrade", &format!(
                    "{} {} — {} version(s) in store",
                    pkg_id.name, pkg_id.version, versions.len()
                ));
            }
            output::status("upgrade", "all packages at latest available versions");
            output::status("upgrade", "to get newer versions, update BPKGBUILDs and run `bsys build`");
        }
    } else if let Some(pkg) = package {
        output::status("upgrade", &format!("checking {pkg}..."));
        if let Ok(store) = bpkg_store::PackageStore::new(store_root) {
            let versions = store.query(pkg);
            match versions.len() {
                0 => output::status("upgrade", &format!("{pkg} not installed")),
                1 => output::status("upgrade", &format!("{pkg} {} — latest in store", versions[0].version)),
                n => {
                    output::status("upgrade", &format!("{pkg} has {n} versions:"));
                    for v in &versions {
                        println!("    {} {}", v.name, v.version);
                    }
                }
            }
        }
    } else {
        output::status("upgrade", "specify a package or use --all");
    }
}
