use std::path::PathBuf;

use anyhow::Result;

use bingux_gated::permissions::{PermissionDb, PermissionGrant};

use crate::output;

fn default_perms_dir() -> PathBuf {
    std::env::var("BXC_PERMS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or("/root".into());
            PathBuf::from(home).join(".config/bingux/permissions")
        })
}

/// Pre-grant permissions to a package.
pub fn run_grant(package: &str, permissions: &[String]) -> Result<()> {
    let perms_dir = default_perms_dir();
    let _ = std::fs::create_dir_all(&perms_dir);

    let mut db = PermissionDb::new("user", perms_dir);

    for perm in permissions {
        output::print_spinner(&format!("Granting {perm} to {package}..."));

        if perm.contains('/') || perm.starts_with('~') {
            // Mount permission: ~/Downloads:rw
            let parts: Vec<&str> = perm.splitn(2, ':').collect();
            let path = parts[0];
            let grants = if parts.len() > 1 { parts[1] } else { "rw" };
            db.grant_mount(package, path, grants)?;
            output::print_success(&format!("Granted mount {path}:{grants} to {package}"));
        } else {
            // Capability permission: gpu, audio, net:outbound, etc.
            db.grant_capability(package, perm)?;
            output::print_success(&format!("Granted {perm} to {package}"));
        }
    }

    Ok(())
}

/// Revoke permissions from a package.
pub fn run_revoke(package: &str, permissions: &[String]) -> Result<()> {
    let perms_dir = default_perms_dir();

    if !perms_dir.exists() {
        output::print_warning(&format!("No permissions recorded for {package}"));
        return Ok(());
    }

    let mut db = PermissionDb::new("user", perms_dir);

    for perm in permissions {
        output::print_spinner(&format!("Revoking {perm} from {package}..."));

        if perm.contains('/') || perm.starts_with('~') {
            // Mount permission
            db.deny_capability(package, &format!("mount:{perm}"))?;
            output::print_success(&format!("Revoked mount {perm} from {package}"));
        } else {
            // Capability — set to deny
            db.deny_capability(package, perm)?;
            output::print_success(&format!("Revoked {perm} from {package}"));
        }
    }

    Ok(())
}
