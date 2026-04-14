//! Full dispatch flow integration tests: write dispatch table, resolve
//! binary name, and verify resulting paths.

use std::fs;

use tempfile::TempDir;

use bxc_shim::dispatch::{DispatchTable, resolve_dispatch_with_roots};
use bxc_shim::resolver::{resolve_binary, resolve_versioned};
use bxc_shim::version::parse_version_syntax;

/// End-to-end: write a dispatch table to disk, resolve a binary name
/// through the full user-then-system fallback, and verify the binary path.
#[test]
fn full_dispatch_flow_system_table() {
    let tmp = TempDir::new().unwrap();

    // Set up system dispatch table
    let system_dir = tmp.path().join("system/profiles/current");
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(
        system_dir.join(".dispatch.toml"),
        r#"
[bash]
package = "bash-5.2.21-x86_64-linux"
binary = "bin/bash"
sandbox = "none"

[firefox]
package = "firefox-129.0-x86_64-linux"
binary = "bin/firefox"
sandbox = "standard"

[steam]
package = "steam-1.0.0-x86_64-linux"
binary = "bin/steam"
sandbox = "strict"
"#,
    )
    .unwrap();

    let system_root = tmp.path().join("system");

    // Resolve bash
    let entry = resolve_dispatch_with_roots(
        "bash",
        1000,
        system_root.to_str().unwrap(),
        "/nonexistent",
    )
    .unwrap();
    assert_eq!(entry.package, "bash-5.2.21-x86_64-linux");
    assert_eq!(entry.sandbox, "none");

    let binary_path = resolve_binary(&entry);
    assert_eq!(
        binary_path.to_str().unwrap(),
        "/system/packages/bash-5.2.21-x86_64-linux/bin/bash"
    );

    // Resolve firefox
    let entry = resolve_dispatch_with_roots(
        "firefox",
        1000,
        system_root.to_str().unwrap(),
        "/nonexistent",
    )
    .unwrap();
    assert_eq!(entry.sandbox, "standard");

    // Resolve steam
    let entry = resolve_dispatch_with_roots(
        "steam",
        1000,
        system_root.to_str().unwrap(),
        "/nonexistent",
    )
    .unwrap();
    assert_eq!(entry.sandbox, "strict");
}

/// End-to-end: user dispatch table overrides system dispatch table.
#[test]
fn full_dispatch_flow_user_override() {
    let tmp = TempDir::new().unwrap();

    // System table has firefox 128
    let system_dir = tmp.path().join("system/profiles/current");
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(
        system_dir.join(".dispatch.toml"),
        r#"
[firefox]
package = "firefox-128.0-x86_64-linux"
binary = "bin/firefox"
sandbox = "standard"
"#,
    )
    .unwrap();

    // User table has firefox 130 with minimal sandbox
    let user_dir = tmp.path().join("home/1000/.config/bingux/profiles/current");
    fs::create_dir_all(&user_dir).unwrap();
    fs::write(
        user_dir.join(".dispatch.toml"),
        r#"
[firefox]
package = "firefox-130.0-x86_64-linux"
binary = "bin/firefox"
sandbox = "minimal"
"#,
    )
    .unwrap();

    let entry = resolve_dispatch_with_roots(
        "firefox",
        1000,
        tmp.path().join("system").to_str().unwrap(),
        tmp.path().join("home").to_str().unwrap(),
    )
    .unwrap();

    // User table wins
    assert_eq!(entry.package, "firefox-130.0-x86_64-linux");
    assert_eq!(entry.sandbox, "minimal");

    let binary_path = resolve_binary(&entry);
    assert_eq!(
        binary_path.to_str().unwrap(),
        "/system/packages/firefox-130.0-x86_64-linux/bin/firefox"
    );
}

/// End-to-end: version syntax parsing feeds into versioned resolution.
#[test]
fn full_dispatch_flow_with_version_syntax() {
    let (name, version) = parse_version_syntax("firefox@128.0.1");
    assert_eq!(name, "firefox");
    assert_eq!(version, Some("128.0.1".to_string()));

    let path = resolve_versioned(&name, version.as_deref().unwrap(), "x86_64-linux");
    assert_eq!(
        path.to_str().unwrap(),
        "/system/packages/firefox-128.0.1-x86_64-linux/bin/firefox"
    );
}

/// Verify that a missing binary in dispatch returns an error.
#[test]
fn full_dispatch_flow_missing_binary() {
    let tmp = TempDir::new().unwrap();

    // Empty dispatch table
    let system_dir = tmp.path().join("system/profiles/current");
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(system_dir.join(".dispatch.toml"), "").unwrap();

    let result = resolve_dispatch_with_roots(
        "nonexistent",
        1000,
        tmp.path().join("system").to_str().unwrap(),
        "/nonexistent",
    );
    assert!(result.is_err());
}

/// Verify dispatch table can be loaded and all entries are queryable.
#[test]
fn dispatch_table_all_entries_queryable() {
    let dispatch_content = r#"
[bash]
package = "bash-5.2-x86_64-linux"
binary = "bin/bash"
sandbox = "none"

[python]
package = "python-3.12-x86_64-linux"
binary = "bin/python3"
sandbox = "minimal"

[firefox]
package = "firefox-129.0-x86_64-linux"
binary = "bin/firefox"
sandbox = "standard"
"#;

    let table = DispatchTable::from_str(dispatch_content).unwrap();

    assert_eq!(table.entries.len(), 3);
    assert!(table.lookup("bash").is_some());
    assert!(table.lookup("python").is_some());
    assert!(table.lookup("firefox").is_some());
    assert!(table.lookup("missing").is_none());

    // Verify each entry's binary path resolves correctly
    for (name, entry) in &table.entries {
        let path = resolve_binary(entry);
        assert!(
            path.to_str().unwrap().contains(&entry.package),
            "binary path for {name} should contain package name"
        );
    }
}
