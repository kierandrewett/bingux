use std::path::PathBuf;

use crate::output;

fn default_perms_dir() -> PathBuf {
    std::env::var("BXC_PERMS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or("/root".into());
            PathBuf::from(home).join(".config/bingux/permissions")
        })
}

/// Show the computed mount set for a sandboxed package.
pub fn run(package: &str) {
    output::status("mounts", &format!("{package} sandbox mount set:"));
    println!();

    // Always-present mounts (from the sandbox runtime)
    println!("  \x1b[1mBase mounts (always present):\x1b[0m");
    println!("    /system/packages/       ro  (package store)");
    println!("    /proc                   proc");
    println!("    /dev                    minimal devtmpfs (null, zero, urandom)");
    println!("    /dev/shm                tmpfs");
    println!("    /tmp                    tmpfs");
    println!("    ~/.config/bingux/state/{package}/home/  → /users/$USER/  (per-package home)");
    println!();

    // Permission-based mounts
    let perms_dir = default_perms_dir();
    let perm_file = perms_dir.join(format!("{package}.toml"));

    if perm_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&perm_file) {
            println!("  \x1b[1mGranted mounts (from permissions):\x1b[0m");
            let mut found_mounts = false;

            // Parse [mounts] section
            let mut in_mounts = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed == "[mounts]" {
                    in_mounts = true;
                    continue;
                }
                if trimmed.starts_with('[') && trimmed != "[mounts]" {
                    in_mounts = false;
                    continue;
                }
                if in_mounts && trimmed.contains('=') {
                    let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
                    let path = parts[0].trim().trim_matches('"');
                    let grants = parts[1].trim().trim_matches('"');
                    println!("    {path}  {grants}");
                    found_mounts = true;
                }
            }

            if !found_mounts {
                println!("    (none — per-package home only)");
            }
        }
    } else {
        println!("  \x1b[1mGranted mounts:\x1b[0m");
        println!("    (none — no permissions granted yet)");
        println!("    Files outside the package home will trigger runtime prompts");
    }

    println!();
}
