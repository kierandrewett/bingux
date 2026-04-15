use std::path::PathBuf;

use crate::output;

fn default_state_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".config/bingux/state")
}

/// List the per-package sandboxed home contents for a package.
pub fn run(package: &str) {
    let state_dir = default_state_dir();
    let pkg_home = state_dir.join(package).join("home");

    if !pkg_home.exists() {
        output::status("ls", &format!("{package}: no per-package home (empty or never run)"));
        return;
    }

    output::status("ls", &format!("{package} per-package home:"));

    let mut total_size: u64 = 0;
    let mut file_count: usize = 0;

    for entry in walkdir::WalkDir::new(&pkg_home)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            file_count += 1;
        }

        let depth = entry.depth();
        if depth == 0 {
            continue;
        }

        let name = entry.file_name().to_string_lossy();
        let size = if entry.file_type().is_file() {
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            human_size(bytes)
        } else {
            String::new()
        };

        let indent = "  ".repeat(depth);
        if entry.file_type().is_dir() {
            println!("  {indent}{name}/");
        } else {
            println!("  {indent}{name}  {size}");
        }
    }

    println!();
    output::status("ls", &format!(
        "{file_count} files, {} total",
        human_size(total_size)
    ));
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}
