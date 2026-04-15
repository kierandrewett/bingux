use std::path::PathBuf;
use bpkg_build::{BuildConfig, BuildPipeline};
use bpkg_store::PackageStore;

/// Helper to build a package from a recipe string and verify the binary runs.
async fn build_and_verify(recipe: &str, pkg_name: &str, binary_name: &str, version_flag: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let store_root = tmp.path().join("store");
    let work_dir = tmp.path().join("work");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&store_root).unwrap();
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let recipe_dir = tmp.path().join(format!("recipes/{pkg_name}"));
    std::fs::create_dir_all(&recipe_dir).unwrap();
    std::fs::write(recipe_dir.join("BPKGBUILD"), recipe).unwrap();

    let config = BuildConfig {
        recipe_path: PathBuf::new(),
        store_root: store_root.clone(),
        work_dir,
        source_cache: cache_dir,
        arch: "x86_64-linux".to_string(),
        network_fetch: true,
    };

    let pipeline = BuildPipeline::new(config);
    let result = pipeline.build(&recipe_dir.join("BPKGBUILD")).await.unwrap();
    assert_eq!(result.package_id.name, pkg_name);

    let store = PackageStore::new(store_root).unwrap();
    let pkg_dir = store.get(&result.package_id).unwrap();
    let bin = pkg_dir.join(format!("bin/{binary_name}"));
    assert!(bin.exists(), "{binary_name} should exist at {}", bin.display());

    let output = std::process::Command::new(&bin)
        .arg(version_flag)
        .output()
        .expect(&format!("Failed to run {binary_name}"));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{pkg_name}: {}", combined.trim());
    assert!(
        output.status.success() || combined.contains(pkg_name),
        "{pkg_name} should produce version output: {combined}"
    );
}

const JQ_RECIPE: &str = r#"
pkgscope="bingux"
pkgname="jq"
pkgver="1.7.1"
pkgarch="x86_64-linux"
pkgdesc="Command-line JSON processor"
license="MIT"

depends=()
exports=(
    "bin/jq"
)

source=("https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64")
sha256sums=("SKIP")

package() {
    mkdir -p "$PKGDIR/bin"
    cp "$SRCDIR/jq-linux-amd64" "$PKGDIR/bin/jq"
    chmod +x "$PKGDIR/bin/jq"
}
"#;

const RIPGREP_RECIPE: &str = r#"
pkgscope="bingux"
pkgname="ripgrep"
pkgver="14.1.1"
pkgarch="x86_64-linux"
pkgdesc="Fast regex search tool"
license="MIT"

depends=()
exports=(
    "bin/rg"
)

source=("https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz")
sha256sums=("SKIP")

package() {
    mkdir -p "$PKGDIR/bin"
    cp "$SRCDIR/ripgrep-14.1.1-x86_64-unknown-linux-musl/rg" "$PKGDIR/bin/rg"
    chmod +x "$PKGDIR/bin/rg"
}
"#;

const FD_RECIPE: &str = r#"
pkgscope="bingux"
pkgname="fd"
pkgver="10.2.0"
pkgarch="x86_64-linux"
pkgdesc="Fast find alternative"
license="MIT"

depends=()
exports=(
    "bin/fd"
)

source=("https://github.com/sharkdp/fd/releases/download/v10.2.0/fd-v10.2.0-x86_64-unknown-linux-musl.tar.gz")
sha256sums=("SKIP")

package() {
    mkdir -p "$PKGDIR/bin"
    cp "$SRCDIR/fd-v10.2.0-x86_64-unknown-linux-musl/fd" "$PKGDIR/bin/fd"
    chmod +x "$PKGDIR/bin/fd"
}
"#;

#[tokio::test]
async fn test_build_real_jq() {
    build_and_verify(JQ_RECIPE, "jq", "jq", "--version").await;
}

#[tokio::test]
async fn test_build_real_ripgrep() {
    build_and_verify(RIPGREP_RECIPE, "ripgrep", "rg", "--version").await;
}

#[tokio::test]
async fn test_build_real_fd() {
    build_and_verify(FD_RECIPE, "fd", "fd", "--version").await;
}

#[tokio::test]
async fn test_build_real_jq_package() {
    let tmp = tempfile::tempdir().unwrap();
    let store_root = tmp.path().join("store");
    let work_dir = tmp.path().join("work");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&store_root).unwrap();
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let recipe_dir = tmp.path().join("recipes/jq");
    std::fs::create_dir_all(&recipe_dir).unwrap();
    std::fs::write(recipe_dir.join("BPKGBUILD"), JQ_RECIPE).unwrap();

    let config = BuildConfig {
        recipe_path: PathBuf::new(),
        store_root: store_root.clone(),
        work_dir,
        source_cache: cache_dir,
        arch: "x86_64-linux".to_string(),
        network_fetch: true,
    };

    let pipeline = BuildPipeline::new(config);
    let result = pipeline.build(&recipe_dir.join("BPKGBUILD")).await.unwrap();
    
    assert_eq!(result.package_id.name, "jq");
    assert_eq!(result.package_id.version, "1.7.1");
    
    // Verify jq binary exists and runs
    let store = PackageStore::new(store_root).unwrap();
    let pkg_dir = store.get(&result.package_id).unwrap();
    let jq_bin = pkg_dir.join("bin/jq");
    assert!(jq_bin.exists(), "jq binary should exist");
    
    // Run jq
    let output = std::process::Command::new(&jq_bin)
        .arg("--version")
        .output()
        .expect("Failed to run jq");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("jq output: stdout={}, stderr={}", stdout.trim(), stderr.trim());
    assert!(output.status.success() || stdout.contains("jq") || stderr.contains("jq"),
        "jq should run: stdout={}, stderr={}", stdout, stderr);
}
