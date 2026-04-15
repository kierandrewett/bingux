use std::path::PathBuf;

use anyhow::Result;

use bpkg_home::HomeConfig;

use crate::output;

fn default_home_toml() -> PathBuf {
    std::env::var("BPKG_HOME_TOML")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config/bingux/config/home.toml")
        })
}

fn default_state_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".config/bingux/state")
}

/// Remove a package from the user profile.
///
/// When `purge` is true, also delete per-package state (sandboxed home, config, cache).
pub fn run(package: &str, purge: bool) -> Result<()> {
    output::print_spinner(&format!("Removing {package} from user profile..."));

    let home_path = default_home_toml();
    if home_path.exists() {
        let mut config = HomeConfig::load(&home_path)?;

        // Remove both unpinned and pinned versions
        let removed_plain = config.remove_package(package);

        // Also check for pinned versions (pkg@version)
        let pinned_prefix = format!("{package}@");
        let content = std::fs::read_to_string(&home_path).unwrap_or_default();
        let had_pin = content.contains(&pinned_prefix);

        if removed_plain || had_pin {
            // Reload to handle pin removal
            if had_pin {
                let new_content: String = content.lines()
                    .filter(|l| !l.contains(&format!("\"{pinned_prefix}")))
                    .collect::<Vec<_>>()
                    .join("\n");
                std::fs::write(&home_path, &new_content)?;
            } else {
                config.save(&home_path)?;
            }
            output::print_status("removed", &format!("{package} from home.toml"));
        } else {
            output::print_warning(&format!("{package} was not in home.toml"));
        }
    }

    // Remove from session (volatile)
    let session_root = std::env::var("BPKG_SESSION_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/run/bingux/session"));
    let session_bin = session_root.join("bin").join(package);
    if session_bin.exists() || session_bin.symlink_metadata().is_ok() {
        let _ = std::fs::remove_file(&session_bin);
        output::print_status("removed", &format!("{package} from session"));
    }

    if purge {
        output::print_spinner(&format!("Purging state for {package}..."));
        let state_dir = default_state_dir().join(package);
        if state_dir.exists() {
            std::fs::remove_dir_all(&state_dir)?;
            output::print_success(&format!("Purged per-package state for {package}"));
        } else {
            output::print_success(&format!("No state to purge for {package}"));
        }
    }

    output::print_success(&format!("Removed {package}"));
    Ok(())
}
