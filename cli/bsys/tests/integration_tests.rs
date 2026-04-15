//! Integration tests for bsys-cli: verify that wired handlers produce
//! expected output when pointed at real (but temporary) data directories.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use bpkg_store::PackageStore;
use bsys_config::{EtcGenerator, parse_system_config_str};

// ── Store-backed `bsys list` behaviour ────────────────────────────

/// Helper: create a fake installed package in a store directory.
fn install_fake_package(store_root: &Path, name: &str, version: &str) {
    let store = PackageStore::new(store_root.to_path_buf()).unwrap();
    let src = store_root.parent().unwrap().join(format!("src-{name}-{version}"));
    let meta = src.join(".bpkg");
    fs::create_dir_all(&meta).unwrap();

    let manifest = format!(
        r#"[package]
name = "{name}"
version = "{version}"
arch = "x86_64-linux"
description = "Test package {name}"
"#
    );
    fs::write(meta.join("manifest.toml"), manifest).unwrap();
    let bin_dir = src.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join(name), "#!/bin/sh\n").unwrap();

    store.install(&src).unwrap();
}

#[test]
fn list_empty_store_returns_no_packages() {
    let tmp = TempDir::new().unwrap();
    let store = PackageStore::new(tmp.path().join("store")).unwrap();
    let ids = store.list();
    assert!(ids.is_empty());
}

#[test]
fn list_store_returns_installed_packages() {
    let tmp = TempDir::new().unwrap();
    let store_root = tmp.path().join("store");
    install_fake_package(&store_root, "bash", "5.2");
    install_fake_package(&store_root, "coreutils", "9.4");

    let store = PackageStore::new(store_root).unwrap();
    let ids = store.list();
    assert_eq!(ids.len(), 2);

    let names: Vec<&str> = ids.iter().map(|id| id.name.as_str()).collect();
    assert!(names.contains(&"bash"));
    assert!(names.contains(&"coreutils"));
}

#[test]
fn list_store_output_includes_version() {
    let tmp = TempDir::new().unwrap();
    let store_root = tmp.path().join("store");
    install_fake_package(&store_root, "firefox", "129.0");

    let store = PackageStore::new(store_root).unwrap();
    let ids = store.list();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].name, "firefox");
    assert_eq!(ids[0].version, "129.0");
}

// ── System config / EtcGenerator integration ──────────────────────

const SYSTEM_TOML: &str = r#"
[system]
hostname = "test-box"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = ["bash", "coreutils"]

[services]
enable = ["sshd"]

[network]
dns = ["1.1.1.1"]

[firewall]
allow_ports = [22, 443]
"#;

#[test]
fn apply_reads_system_toml_and_generates_etc_files() {
    let config = parse_system_config_str(SYSTEM_TOML).unwrap();

    let tmp = TempDir::new().unwrap();
    let etc_dir = tmp.path().join("etc");
    fs::create_dir_all(&etc_dir).unwrap();

    let generator = EtcGenerator::new(etc_dir.clone());
    let files = generator.generate_all(&config).unwrap();

    // passwd, group, os-release + hostname, locale.conf, locale.gen, vconsole.conf,
    // localtime, resolv.conf, nftables.conf = 10 files
    assert_eq!(files.len(), 10);

    // Verify hostname content
    let hostname_content = fs::read_to_string(etc_dir.join("hostname")).unwrap();
    assert_eq!(hostname_content, "test-box\n");

    // Verify locale.conf content
    let locale_content = fs::read_to_string(etc_dir.join("locale.conf")).unwrap();
    assert_eq!(locale_content, "LANG=en_GB.UTF-8\n");

    // Verify vconsole.conf content
    let vconsole_content = fs::read_to_string(etc_dir.join("vconsole.conf")).unwrap();
    assert_eq!(vconsole_content, "KEYMAP=uk\n");

    // Verify resolv.conf content
    let resolv_content = fs::read_to_string(etc_dir.join("resolv.conf")).unwrap();
    assert!(resolv_content.contains("nameserver 1.1.1.1"));

    // Verify nftables has the allowed ports
    let nft_content = fs::read_to_string(etc_dir.join("nftables.conf")).unwrap();
    assert!(nft_content.contains("tcp dport 22 accept"));
    assert!(nft_content.contains("tcp dport 443 accept"));
}

#[test]
fn apply_minimal_config_generates_core_files_only() {
    let minimal = r#"
[system]
hostname = "minimal"
locale = "en_US.UTF-8"
timezone = "UTC"
keymap = "us"

[packages]
keep = []

[services]
enable = []
"#;

    let config = parse_system_config_str(minimal).unwrap();
    let tmp = TempDir::new().unwrap();
    let etc_dir = tmp.path().join("etc");
    fs::create_dir_all(&etc_dir).unwrap();

    let generator = EtcGenerator::new(etc_dir);
    let files = generator.generate_all(&config).unwrap();

    // passwd, group, os-release + 5 config files (no resolv.conf, no nftables.conf)
    assert_eq!(files.len(), 8);
}
