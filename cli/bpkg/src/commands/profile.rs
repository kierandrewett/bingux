use std::path::PathBuf;

use anyhow::Result;

use bsys_compose::GenerationBuilder;

use crate::output;

/// Default profiles root for user generations.
fn default_home_toml() -> PathBuf {
    std::env::var("BPKG_HOME_TOML")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config/bingux/config/home.toml")
        })
}

fn default_profiles_root() -> PathBuf {
    std::env::var("BPKG_PROFILES_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config/bingux/profiles")
        })
}

/// Default store root for packages.
fn default_packages_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

/// Recompose the user profile from the declared state.
pub fn run_apply() -> Result<()> {
    output::print_spinner("Recomposing user profile...");

    let profiles = default_profiles_root();
    let packages = default_packages_root();
    let builder = GenerationBuilder::new(profiles, packages);

    // TODO: read user package list (kept + volatile + pins) and build PackageEntry list
    // For now, build an empty generation to demonstrate the wiring.
    let built = builder.build(&[])?;
    builder.activate(built.id)?;

    output::print_success(&format!("User profile recomposed (generation {})", built.id));
    Ok(())
}

/// Roll back the user profile to a previous generation.
pub fn run_rollback(generation: Option<u64>) -> Result<()> {
    let profiles = default_profiles_root();
    let packages = default_packages_root();
    let builder = GenerationBuilder::new(profiles, packages);

    match generation {
        Some(number) => {
            output::print_spinner(&format!("Rolling back to generation {number}..."));
            builder.rollback(number)?;
            output::print_success(&format!("Rolled back to generation {number}"));
        }
        None => {
            output::print_spinner("Rolling back to previous generation...");
            let current = builder.current()?;
            match current {
                Some(id) if id > 1 => {
                    builder.rollback(id - 1)?;
                    output::print_success(&format!("Rolled back to generation {}", id - 1));
                }
                Some(_) => {
                    output::print_warning("Already at generation 1, cannot roll back further");
                }
                None => {
                    output::print_warning("No active generation found");
                }
            }
        }
    }
    Ok(())
}

/// List user profile generations.
pub fn run_history() -> Result<()> {
    let profiles = default_profiles_root();
    let packages = default_packages_root();
    let builder = GenerationBuilder::new(profiles, packages);

    let generations = builder.list_generations()?;

    if generations.is_empty() {
        println!("No profile generations found.");
        return Ok(());
    }

    let current_id = builder.current()?.unwrap_or(0);

    let entries: Vec<(u64, String, String)> = generations
        .iter()
        .rev()
        .map(|g| {
            let marker = if g.id == current_id { " (current)" } else { "" };
            let pkg_count = g.packages.len();
            let action = format!("{pkg_count} packages{marker}");
            // Convert epoch timestamp to a human-readable date.
            let date = format_epoch(g.timestamp);
            (g.id, date, action)
        })
        .collect();

    output::print_history(&entries);
    Ok(())
}

/// First-time user profile setup.
pub fn run_init() -> Result<()> {
    output::print_spinner("Initialising user profile...");

    let profiles = default_profiles_root();
    let packages = default_packages_root();

    std::fs::create_dir_all(&profiles)?;
    std::fs::create_dir_all(&packages)?;

    // Generate initial generation
    let builder = GenerationBuilder::new(profiles, packages.clone());
    let built = builder.build(&[])?;
    builder.activate(built.id)?;

    // Generate home.toml from current store
    let home_toml_path = default_home_toml();
    if !home_toml_path.exists() {
        if let Some(parent) = home_toml_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Discover packages in store
        let mut keep_list = Vec::new();
        if let Ok(store) = bpkg_store::PackageStore::new(packages) {
            for pkg_id in store.list() {
                keep_list.push(pkg_id.name.clone());
            }
        }

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".into());
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());

        let keep_str = keep_list.iter()
            .map(|p| format!("\"{}\"", p))
            .collect::<Vec<_>>()
            .join(", ");

        let content = format!(
            r#"# Bingux home configuration — generated by bpkg init
# Edit this file and run `bpkg home apply` to converge your environment.

[user]
shell = "{shell}"
editor = "{editor}"

[packages]
keep = [{keep_str}]

[env]
EDITOR = "{editor}"

[services]
enable = []
"#
        );

        std::fs::write(&home_toml_path, &content)?;
        output::print_success(&format!(
            "Generated home.toml with {} packages at {}",
            keep_list.len(),
            home_toml_path.display()
        ));
    } else {
        output::print_warning("home.toml already exists — not overwriting");
    }

    output::print_success("User profile initialised");
    Ok(())
}

/// Simple epoch-to-date formatting.
fn format_epoch(epoch: u64) -> String {
    // Basic UTC conversion — same algorithm as bingux-gated chrono_now
    let secs = epoch;
    let days = secs / 86400;
    let day_secs = secs % 86400;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;

    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02}")
}
