use anyhow::Result;

use crate::output;

/// Install a package into the user profile.
///
/// When `keep` is false, the package is volatile (disappears on reboot).
/// When `keep` is true, the package is persisted across reboots.
pub fn run(package: &str, keep: bool) -> Result<()> {
    let mode = if keep { "persistent" } else { "volatile" };
    output::print_spinner(&format!("Resolving {package}..."));
    output::print_spinner(&format!("Installing {package} ({mode})..."));

    // TODO: resolve package from repo index
    // TODO: fetch/build package if not in store
    // TODO: add to user profile state (volatile.toml or kept list)
    // TODO: recompose user profile

    output::print_success(&format!("Installed {package} ({mode})"));
    if !keep {
        output::print_warning(
            "This is a volatile install. It will disappear on reboot. Use `bpkg keep` to persist.",
        );
    }
    Ok(())
}
