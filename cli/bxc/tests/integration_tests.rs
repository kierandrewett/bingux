//! Integration tests for bxc-cli: verify that wired handlers interact
//! correctly with the sandbox and permission backend crates.

use std::fs;

use tempfile::TempDir;

use bingux_gated::permissions::{PackagePermissions, PermissionDb, PermissionGrant};
use bxc_sandbox::{SandboxLevel, SeccompProfile};
use bxc_shim::dispatch::{DispatchTable, resolve_dispatch_with_roots};
use bxc_shim::resolver::resolve_binary;

// ── bxc inspect: sandbox level + seccomp profile ──────────────────

#[test]
fn inspect_standard_sandbox_shows_seccomp_counts() {
    let profile = SeccompProfile::for_level(SandboxLevel::Standard);
    assert!(!profile.is_empty());
    assert!(!profile.allow_list.is_empty(), "standard should have allowed syscalls");
    assert!(!profile.notify_list.is_empty(), "standard should have notified syscalls");
    assert!(!profile.deny_list.is_empty(), "standard should have denied syscalls");
}

#[test]
fn inspect_none_sandbox_has_empty_profile() {
    let profile = SeccompProfile::for_level(SandboxLevel::None);
    assert!(profile.is_empty());
}

#[test]
fn inspect_strict_notifies_more_than_standard() {
    let standard = SeccompProfile::for_level(SandboxLevel::Standard);
    let strict = SeccompProfile::for_level(SandboxLevel::Strict);
    assert!(strict.notify_list.len() > standard.notify_list.len());
}

#[test]
fn inspect_resolves_sandbox_level_from_dispatch() {
    let tmp = TempDir::new().unwrap();
    let system_dir = tmp.path().join("profiles/current");
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(
        system_dir.join(".dispatch.toml"),
        r#"
[firefox]
package = "firefox-129.0-x86_64-linux"
binary = "bin/firefox"
sandbox = "standard"

[curl]
package = "curl-8.8-x86_64-linux"
binary = "bin/curl"
sandbox = "strict"
"#,
    )
    .unwrap();

    let entry = resolve_dispatch_with_roots(
        "firefox",
        1000,
        tmp.path().to_str().unwrap(),
        "/nonexistent",
    )
    .unwrap();
    assert_eq!(entry.sandbox, "standard");

    let entry = resolve_dispatch_with_roots(
        "curl",
        1000,
        tmp.path().to_str().unwrap(),
        "/nonexistent",
    )
    .unwrap();
    assert_eq!(entry.sandbox, "strict");
}

// ── bxc perms: permission database ────────────────────────────────

#[test]
fn perms_show_empty_for_unknown_package() {
    let tmp = TempDir::new().unwrap();
    let mut db = PermissionDb::new("testuser", tmp.path().to_path_buf());
    let perms = db.load("unknown-pkg").unwrap();
    assert!(perms.capabilities.is_empty());
    assert!(perms.mounts.is_empty());
    assert!(perms.files.is_empty());
}

#[test]
fn perms_show_granted_capabilities() {
    let tmp = TempDir::new().unwrap();
    let mut db = PermissionDb::new("testuser", tmp.path().to_path_buf());

    db.grant_capability("firefox", "gpu").unwrap();
    db.grant_capability("firefox", "audio").unwrap();
    db.deny_capability("firefox", "camera").unwrap();

    let perms = db.load("firefox").unwrap();
    assert_eq!(
        perms.capabilities.get("gpu"),
        Some(&PermissionGrant::Allow)
    );
    assert_eq!(
        perms.capabilities.get("audio"),
        Some(&PermissionGrant::Allow)
    );
    assert_eq!(
        perms.capabilities.get("camera"),
        Some(&PermissionGrant::Deny)
    );
}

#[test]
fn perms_reset_clears_all_permissions() {
    let tmp = TempDir::new().unwrap();
    let mut db = PermissionDb::new("testuser", tmp.path().to_path_buf());

    db.grant_capability("firefox", "gpu").unwrap();
    db.grant_mount("firefox", "~/Downloads", "list,w").unwrap();

    // Reset by saving empty permissions
    let empty = PackagePermissions::new_empty("firefox");
    db.save("firefox", &empty).unwrap();

    // Reload from disk to verify
    let mut db2 = PermissionDb::new("testuser", tmp.path().to_path_buf());
    let perms = db2.load("firefox").unwrap();
    assert!(perms.capabilities.is_empty());
    assert!(perms.mounts.is_empty());
}

#[test]
fn perms_persist_across_db_instances() {
    let tmp = TempDir::new().unwrap();
    {
        let mut db = PermissionDb::new("alice", tmp.path().to_path_buf());
        db.grant_capability("vlc", "audio").unwrap();
        db.grant_file("vlc", "/media/music", "r").unwrap();
    }

    // New instance reads from disk
    let mut db2 = PermissionDb::new("alice", tmp.path().to_path_buf());
    assert_eq!(
        db2.check_capability("vlc", "audio"),
        PermissionGrant::Allow
    );
    assert_eq!(db2.check_file("vlc", "/media/music"), PermissionGrant::Allow);
    assert_eq!(
        db2.check_capability("vlc", "network"),
        PermissionGrant::Prompt
    );
}

// ── Dispatch table resolution ─────────────────────────────────────

#[test]
fn dispatch_resolves_binary_path() {
    let dispatch_toml = r#"
[firefox]
package = "firefox-129.0-x86_64-linux"
binary = "bin/firefox"
sandbox = "standard"
"#;

    let table = DispatchTable::from_str(dispatch_toml).unwrap();
    let entry = table.lookup("firefox").unwrap();
    let path = resolve_binary(entry);
    assert_eq!(
        path.to_str().unwrap(),
        "/system/packages/firefox-129.0-x86_64-linux/bin/firefox"
    );
}
