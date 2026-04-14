use anyhow::Result;

use crate::output::{self, PackageListEntry, PackageStatus};

/// List all user-installed packages with their status.
pub fn run() -> Result<()> {
    // TODO: read from user profile state to determine kept/volatile/pinned status
    // TODO: read from volatile.toml for session-only packages
    // For now, show stub data to demonstrate output formatting.

    let packages = vec![
        PackageListEntry {
            name: "firefox".to_string(),
            version: "129.0".to_string(),
            status: PackageStatus::Kept,
        },
        PackageListEntry {
            name: "ripgrep".to_string(),
            version: "14.1".to_string(),
            status: PackageStatus::Volatile,
        },
        PackageListEntry {
            name: "neovim".to_string(),
            version: "0.10".to_string(),
            status: PackageStatus::Pinned("0.10".to_string()),
        },
    ];

    output::print_package_list(&packages);
    Ok(())
}
