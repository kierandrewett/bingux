use std::path::PathBuf;

use crate::output;

fn default_config_path() -> PathBuf {
    std::env::var("BSYS_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/config/system.toml"))
}

/// Boot-essential packages that should never be unkepped without --force.
const BOOT_ESSENTIAL: &[&str] = &[
    "glibc", "linux", "systemd", "bash", "coreutils",
    "bpkg", "bsys", "bxc-shim", "bingux-gated",
];

pub fn run_keep(package: &str) {
    let config_path = default_config_path();
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();

    if content.contains(&format!("\"{package}\"")) {
        output::status("keep", &format!("{package} is already kept in system.toml"));
        return;
    }

    // Add to the keep list
    let new_content = if content.contains("keep = [") {
        content.replace(
            "keep = [",
            &format!("keep = [\"{package}\", "),
        )
    } else {
        format!("{content}\n[packages]\nkeep = [\"{package}\"]\n")
    };

    if let Err(e) = std::fs::write(&config_path, &new_content) {
        output::status("error", &format!("failed to update system.toml: {e}"));
        return;
    }

    output::status("keep", &format!("{package} promoted to persistent in system.toml"));
}

pub fn run_unkeep(package: &str, force: bool) {
    // Check if boot-essential
    if BOOT_ESSENTIAL.contains(&package) && !force {
        output::status("error", &format!(
            "{package} is boot-essential — use --force to override (dangerous!)"
        ));
        return;
    }

    let config_path = default_config_path();
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();

    if !content.contains(&format!("\"{package}\"")) {
        output::status("unkeep", &format!("{package} is not in system.toml keep list"));
        return;
    }

    // Remove from keep list
    let new_content = content
        .replace(&format!("\"{package}\", "), "")
        .replace(&format!(", \"{package}\""), "")
        .replace(&format!("\"{package}\""), "");

    if let Err(e) = std::fs::write(&config_path, &new_content) {
        output::status("error", &format!("failed to update system.toml: {e}"));
        return;
    }

    if BOOT_ESSENTIAL.contains(&package) {
        output::status("warning", &format!("{package} is boot-essential — system may not boot without it"));
    }
    output::status("unkeep", &format!("{package} demoted to volatile in system.toml"));
}
