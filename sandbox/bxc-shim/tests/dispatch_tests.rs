use std::fs;
use std::path::Path;

use bxc_shim::dispatch::{resolve_dispatch_with_roots, DispatchTable};

const SAMPLE_DISPATCH: &str = r#"
[firefox]
package = "firefox-129.0-x86_64-linux"
binary = "bin/firefox"
sandbox = "standard"

[bash]
package = "bash-5.2.21-x86_64-linux"
binary = "bin/bash"
sandbox = "none"

[steam]
package = "steam-1.0.0-x86_64-linux"
binary = "bin/steam"
sandbox = "strict"
"#;

#[test]
fn load_from_toml_string() {
    let table = DispatchTable::from_str(SAMPLE_DISPATCH).unwrap();
    assert_eq!(table.entries.len(), 3);
}

#[test]
fn lookup_finds_existing_entry() {
    let table = DispatchTable::from_str(SAMPLE_DISPATCH).unwrap();
    let entry = table.lookup("firefox").unwrap();
    assert_eq!(entry.package, "firefox-129.0-x86_64-linux");
    assert_eq!(entry.binary, "bin/firefox");
    assert_eq!(entry.sandbox, "standard");
}

#[test]
fn lookup_returns_none_for_missing() {
    let table = DispatchTable::from_str(SAMPLE_DISPATCH).unwrap();
    assert!(table.lookup("nonexistent").is_none());
}

#[test]
fn load_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".dispatch.toml");
    fs::write(&path, SAMPLE_DISPATCH).unwrap();

    let table = DispatchTable::load(&path).unwrap();
    assert_eq!(table.entries.len(), 3);
}

#[test]
fn load_missing_file_returns_error() {
    let result = DispatchTable::load(Path::new("/nonexistent/.dispatch.toml"));
    assert!(result.is_err());
}

#[test]
fn load_invalid_toml_returns_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".dispatch.toml");
    fs::write(&path, "this is not valid toml {{{{").unwrap();

    let result = DispatchTable::load(&path);
    assert!(result.is_err());
}

#[test]
fn resolve_dispatch_system_fallback() {
    let dir = tempfile::tempdir().unwrap();

    // Set up system dispatch table
    let system_dir = dir.path().join("profiles/current");
    fs::create_dir_all(&system_dir).unwrap();
    fs::write(system_dir.join(".dispatch.toml"), SAMPLE_DISPATCH).unwrap();

    let entry = resolve_dispatch_with_roots(
        "firefox",
        1000,
        dir.path().to_str().unwrap(),
        "/nonexistent/home",
    )
    .unwrap();
    assert_eq!(entry.package, "firefox-129.0-x86_64-linux");
}

#[test]
fn resolve_dispatch_user_override() {
    let dir = tempfile::tempdir().unwrap();

    // Set up system dispatch table
    let system_dir = dir.path().join("system/profiles/current");
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

    // Set up user dispatch table (overrides system)
    let user_dir = dir
        .path()
        .join("home/1000/.config/bingux/profiles/current");
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
        dir.path().join("system").to_str().unwrap(),
        dir.path().join("home").to_str().unwrap(),
    )
    .unwrap();

    // User table should win
    assert_eq!(entry.package, "firefox-130.0-x86_64-linux");
    assert_eq!(entry.sandbox, "minimal");
}

#[test]
fn resolve_dispatch_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let result = resolve_dispatch_with_roots(
        "nonexistent",
        1000,
        dir.path().to_str().unwrap(),
        "/nonexistent/home",
    );
    assert!(result.is_err());
}
