use std::path::PathBuf;

use crate::output;

fn default_config_path() -> PathBuf {
    std::env::var("BSYS_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/config/system.toml"))
}

/// Remove a package from the system.
pub fn run(package: &str) {
    output::status("rm", &format!("removing {package}..."));

    let config_path = default_config_path();
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if content.contains(&format!("\"{package}\"")) {
            let new_content = content
                .replace(&format!("\"{package}\", "), "")
                .replace(&format!(", \"{package}\""), "")
                .replace(&format!("\"{package}\""), "");
            if let Err(e) = std::fs::write(&config_path, &new_content) {
                output::status("error", &format!("failed to update system.toml: {e}"));
                return;
            }
            output::status("rm", &format!("{package} removed from system.toml"));
        } else {
            output::status("rm", &format!("{package} not in system.toml"));
        }
    }

    output::status("rm", "run `bsys apply` to update generation, `bsys gc` to free space");
}
