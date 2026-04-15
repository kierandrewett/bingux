//! End-to-end integration tests: recipe → build → store → compose → dispatch
use std::path::PathBuf;

use bpkg_recipe::parse_recipe;
use bpkg_build::{BuildConfig, BuildPipeline};
use bpkg_store::{PackageStore, generate_file_list, verify_file_list};
use bpkg_repo::archive::{create_bgx, extract_bgx, verify_bgx};
use bsys_compose::builder::GenerationBuilder;
use bsys_compose::generation::{PackageEntry, ExportedItems};
use bxc_sandbox::levels::SandboxLevel;

const HELLO_RECIPE: &str = r#"
pkgscope="bingux"
pkgname="hello"
pkgver="1.0.0"
pkgarch="x86_64-linux"
pkgdesc="Simple hello world test package"
license="MIT"

depends=()
exports=(
    "bin/hello"
)

source=()
sha256sums=()

package() {
    mkdir -p "$PKGDIR/bin"
    cat > "$PKGDIR/bin/hello" << 'SCRIPT'
#!/bin/bash
echo "Hello from Bingux!"
SCRIPT
    chmod +x "$PKGDIR/bin/hello"
}
"#;

#[tokio::test]
async fn test_full_pipeline_parse_build_install() {
    let tmp = tempfile::tempdir().unwrap();
    let store_root = tmp.path().join("store");
    let work_dir = tmp.path().join("work");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&store_root).unwrap();
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    // 1. Parse recipe
    let recipe = parse_recipe(HELLO_RECIPE).unwrap();
    assert_eq!(recipe.pkgname, "hello");
    assert_eq!(recipe.pkgver, "1.0.0");
    assert!(recipe.build.is_none(), "Binary package has no build()");
    assert!(recipe.package.is_some(), "Must have package()");

    // 2. Build package via pipeline
    let config = BuildConfig {
        recipe_path: PathBuf::new(),
        store_root: store_root.clone(),
        work_dir: work_dir.clone(),
        source_cache: cache_dir.clone(),
        arch: "x86_64-linux".to_string(),
        network_fetch: false,
    };
    let pipeline = BuildPipeline::new(config);

    let recipe_dir = tmp.path().join("recipes/hello");
    std::fs::create_dir_all(&recipe_dir).unwrap();
    std::fs::write(recipe_dir.join("BPKGBUILD"), HELLO_RECIPE).unwrap();

    let result = pipeline.build(&recipe_dir.join("BPKGBUILD")).await.unwrap();
    assert_eq!(result.package_id.name, "hello");
    assert_eq!(result.package_id.version, "1.0.0");

    // 3. Verify package is in store
    let store = PackageStore::new(store_root.clone()).unwrap();
    let packages = store.list();
    assert!(!packages.is_empty(), "Store should have the package");
    let hello_id = &packages[0];
    assert_eq!(hello_id.name, "hello");

    // 4. Verify the binary exists and runs
    let pkg_dir = store.get(hello_id).unwrap();
    let hello_bin = pkg_dir.join("bin/hello");
    assert!(hello_bin.exists(), "hello binary should exist");

    let output = std::process::Command::new(&hello_bin)
        .output()
        .expect("Failed to run hello");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hello from Bingux!"), "Output: {}", stdout);

    // 5. Verify manifest
    let manifest = store.manifest(hello_id).unwrap();
    assert_eq!(manifest.package.name, "hello");

    // 6. Verify file integrity
    let file_list = generate_file_list(&pkg_dir).unwrap();
    assert!(file_list.contains("bin/hello"));
    let files_path = pkg_dir.join(".bpkg/files.txt");
    std::fs::write(&files_path, &file_list).unwrap();
    verify_file_list(&pkg_dir).unwrap();
}

#[tokio::test]
async fn test_bgx_export_import_cycle() {
    let tmp = tempfile::tempdir().unwrap();
    let store_root = tmp.path().join("store");
    let work_dir = tmp.path().join("work");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&store_root).unwrap();
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let config = BuildConfig {
        recipe_path: PathBuf::new(),
        store_root: store_root.clone(),
        work_dir: work_dir.clone(),
        source_cache: cache_dir.clone(),
        arch: "x86_64-linux".to_string(),
        network_fetch: false,
    };
    let pipeline = BuildPipeline::new(config);
    let recipe_dir = tmp.path().join("recipes/hello");
    std::fs::create_dir_all(&recipe_dir).unwrap();
    std::fs::write(recipe_dir.join("BPKGBUILD"), HELLO_RECIPE).unwrap();
    let result = pipeline.build(&recipe_dir.join("BPKGBUILD")).await.unwrap();

    // Export as .bgx
    let store = PackageStore::new(store_root.clone()).unwrap();
    let pkg_dir = store.get(&result.package_id).unwrap();
    let bgx_path = tmp.path().join("hello-1.0.0-x86_64-linux.bgx");
    create_bgx(&pkg_dir, &bgx_path).unwrap();
    assert!(bgx_path.exists());
    assert!(bgx_path.metadata().unwrap().len() > 0);

    // Verify .bgx
    let info = verify_bgx(&bgx_path).unwrap();
    assert_eq!(info.name, "hello");
    assert_eq!(info.version, "1.0.0");

    // Extract
    let extract_dir = tmp.path().join("extract");
    std::fs::create_dir_all(&extract_dir).unwrap();
    let extracted_id = extract_bgx(&bgx_path, &extract_dir).unwrap();
    assert!(extracted_id.contains("hello"), "Extracted ID: {}", extracted_id);
}

#[test]
fn test_generation_compose_and_dispatch() {
    let tmp = tempfile::tempdir().unwrap();
    let profiles_root = tmp.path().join("profiles");
    let packages_root = tmp.path().join("packages");
    std::fs::create_dir_all(&profiles_root).unwrap();
    std::fs::create_dir_all(&packages_root).unwrap();

    // Create fake packages
    let hello_dir = packages_root.join("hello-1.0.0-x86_64-linux");
    std::fs::create_dir_all(hello_dir.join("bin")).unwrap();
    std::fs::write(hello_dir.join("bin/hello"), "#!/bin/bash\necho hello").unwrap();

    let greet_dir = packages_root.join("greet-2.0.0-x86_64-linux");
    std::fs::create_dir_all(greet_dir.join("bin")).unwrap();
    std::fs::write(greet_dir.join("bin/greet"), "#!/bin/bash\necho greet").unwrap();

    let builder = GenerationBuilder::new(profiles_root.clone(), packages_root.clone());
    let packages = vec![
        PackageEntry {
            package_id: "hello-1.0.0-x86_64-linux".parse().unwrap(),
            sandbox_level: SandboxLevel::Minimal,
            exports: ExportedItems {
                binaries: vec!["bin/hello".to_string()],
                libraries: vec![],
                data: vec![],
            },
        },
        PackageEntry {
            package_id: "greet-2.0.0-x86_64-linux".parse().unwrap(),
            sandbox_level: SandboxLevel::Standard,
            exports: ExportedItems {
                binaries: vec!["bin/greet".to_string()],
                libraries: vec![],
                data: vec![],
            },
        },
    ];

    let generation = builder.build(&packages).unwrap();
    assert!(generation.id > 0);
    builder.activate(generation.id).unwrap();
    assert_eq!(builder.current().unwrap(), Some(generation.id));

    // Dispatch table — builder uses numeric id as dir name
    let gen_dir = profiles_root.join(generation.id.to_string());
    let dispatch_content = std::fs::read_to_string(gen_dir.join(".dispatch.toml")).unwrap();
    assert!(dispatch_content.contains("hello"));
    assert!(dispatch_content.contains("greet"));
    assert!(gen_dir.join("bin").exists());

    // Rollback
    let gen2 = builder.build(&packages[..1]).unwrap();
    builder.activate(gen2.id).unwrap();
    builder.rollback(generation.id).unwrap();
    assert_eq!(builder.current().unwrap(), Some(generation.id));
}

#[test]
fn test_home_config_declarative_cycle() {
    use bpkg_home::{HomeConfig, compute_delta, ApplyEngine};

    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let home_dir = tmp.path().join("home");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&home_dir).unwrap();

    std::fs::write(config_dir.join("home.toml"), r#"
[user]
shell = "zsh"
editor = "nvim"

[packages]
keep = ["firefox", "neovim", "ripgrep"]
rm = ["epiphany"]

[env]
EDITOR = "nvim"
PAGER = "bat"

[dotfiles]
"gitconfig" = ".gitconfig"

[services]
enable = ["syncthing"]
"#).unwrap();
    std::fs::write(config_dir.join("gitconfig"), "[user]\n\tname = Test").unwrap();

    let config = HomeConfig::load(&config_dir.join("home.toml")).unwrap();
    assert!(config.has_package("firefox"));

    let delta = compute_delta(&config, &config_dir, &home_dir, &[]);
    assert!(delta.packages_to_add.contains(&"firefox".to_string()));
    assert_eq!(delta.dotfiles_to_link.len(), 1);

    let engine = ApplyEngine::new(home_dir.clone(), config_dir.clone());
    let linked = engine.link_dotfiles(&delta.dotfiles_to_link, &delta.dotfiles_to_backup).unwrap();
    assert_eq!(linked.len(), 1);

    let env_path = engine.generate_env_sh(&delta.env_changes).unwrap();
    assert!(std::fs::read_to_string(&env_path).unwrap().contains("EDITOR"));

    let mut config2 = config.clone();
    config2.add_package("git");
    config2.save(&config_dir.join("home2.toml")).unwrap();
    let reloaded = HomeConfig::load(&config_dir.join("home2.toml")).unwrap();
    assert!(reloaded.has_package("git"));
    assert!(reloaded.has_package("firefox"));
}

#[test]
fn test_permission_db_grant_and_check() {
    use bingux_gated::permissions::{PermissionDb, PermissionGrant};

    let tmp = tempfile::tempdir().unwrap();
    let perms_dir = tmp.path().join("permissions");
    std::fs::create_dir_all(&perms_dir).unwrap();

    let mut db = PermissionDb::new("testuser", perms_dir.clone());
    assert_eq!(db.check_capability("firefox", "gpu"), PermissionGrant::Prompt);

    db.grant_capability("firefox", "gpu").unwrap();
    assert_eq!(db.check_capability("firefox", "gpu"), PermissionGrant::Allow);

    db.deny_capability("firefox", "camera").unwrap();
    assert_eq!(db.check_capability("firefox", "camera"), PermissionGrant::Deny);

    db.grant_mount("firefox", "~/Downloads", "list,w").unwrap();
    assert!(db.check_mount("firefox", "~/Downloads").is_some());

    // Reload and verify persistence
    let mut db2 = PermissionDb::new("testuser", perms_dir);
    assert_eq!(db2.check_capability("firefox", "gpu"), PermissionGrant::Allow);
    assert_eq!(db2.check_capability("firefox", "camera"), PermissionGrant::Deny);
}

#[test]
fn test_system_config_etc_generation() {
    use bsys_config::{parse_system_config_str, EtcGenerator};

    let config = parse_system_config_str(r#"
[system]
hostname = "bingux-test"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = ["linux", "systemd", "bash"]

[services]
enable = ["NetworkManager", "sshd"]

[network]
dns = ["1.1.1.1", "1.0.0.1"]

[firewall]
allow_ports = [22, 80, 443]
"#).unwrap();
    assert_eq!(config.system.hostname, "bingux-test");

    let tmp = tempfile::tempdir().unwrap();
    let generator = EtcGenerator::new(tmp.path().join("etc"));
    std::fs::create_dir_all(tmp.path().join("etc")).unwrap();
    let files = generator.generate_all(&config).unwrap();
    assert!(!files.is_empty());

    assert_eq!(files.iter().find(|f| f.path.ends_with("hostname")).unwrap().content.trim(), "bingux-test");
    assert!(files.iter().find(|f| f.path.ends_with("locale.conf")).unwrap().content.contains("en_GB.UTF-8"));
    assert!(files.iter().find(|f| f.path.ends_with("resolv.conf")).unwrap().content.contains("1.1.1.1"));
    assert!(files.iter().find(|f| f.path.ends_with("nftables.conf")).unwrap().content.contains("22"));
}

#[test]
fn test_seccomp_profile_generation() {
    use bxc_sandbox::profile::SeccompProfile;
    use bxc_sandbox::levels::SandboxLevel;

    let none_profile = SeccompProfile::for_level(SandboxLevel::None);
    assert!(none_profile.allow_list.is_empty());

    let standard = SeccompProfile::for_level(SandboxLevel::Standard);
    assert!(!standard.allow_list.is_empty());
    assert!(!standard.notify_list.is_empty());
    assert!(standard.notify_list.contains(&257)); // openat
    assert!(standard.allow_list.contains(&0)); // read

    let strict = SeccompProfile::for_level(SandboxLevel::Strict);
    assert!(strict.notify_list.len() >= standard.notify_list.len());
}
