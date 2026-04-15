use std::path::PathBuf;
use bpkg_build::{BuildConfig, BuildPipeline};
use bpkg_store::PackageStore;

/// Helper to build a package from a recipe string and verify the binary runs.
async fn build_and_verify(
    store_root: &std::path::Path,
    work_dir: &std::path::Path,
    cache_dir: &std::path::Path,
    recipe: &str,
    pkg_name: &str,
    binary_name: &str,
    version_flag: &str,
) {
    let recipe_dir = work_dir.join(format!("recipes/{pkg_name}"));
    std::fs::create_dir_all(&recipe_dir).unwrap();
    std::fs::write(recipe_dir.join("BPKGBUILD"), recipe).unwrap();

    let config = BuildConfig {
        recipe_path: PathBuf::new(),
        store_root: store_root.to_path_buf(),
        work_dir: work_dir.join("build"),
        source_cache: cache_dir.to_path_buf(),
        arch: "x86_64-linux".to_string(),
        network_fetch: true,
    };

    let pipeline = BuildPipeline::new(config);
    let result = pipeline.build(&recipe_dir.join("BPKGBUILD")).await.unwrap();
    assert_eq!(result.package_id.name, pkg_name);

    let store = PackageStore::new(store_root.to_path_buf()).unwrap();
    let pkg_dir = store.get(&result.package_id).unwrap();
    let bin = pkg_dir.join(format!("bin/{binary_name}"));
    assert!(bin.exists(), "{binary_name} should exist at {}", bin.display());

    let output = std::process::Command::new(&bin)
        .arg(version_flag)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run {binary_name}: {e}"));
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

const BAT_RECIPE: &str = r#"
pkgscope="bingux"
pkgname="bat"
pkgver="0.24.0"
pkgarch="x86_64-linux"
pkgdesc="Cat clone with syntax highlighting"
license="MIT"

depends=()
exports=(
    "bin/bat"
)

source=("https://github.com/sharkdp/bat/releases/download/v0.24.0/bat-v0.24.0-x86_64-unknown-linux-musl.tar.gz")
sha256sums=("SKIP")

package() {
    mkdir -p "$PKGDIR/bin"
    cp "$SRCDIR/bat-v0.24.0-x86_64-unknown-linux-musl/bat" "$PKGDIR/bin/bat"
    chmod +x "$PKGDIR/bin/bat"
}
"#;

const EZA_RECIPE: &str = r#"
pkgscope="bingux"
pkgname="eza"
pkgver="0.20.14"
pkgarch="x86_64-linux"
pkgdesc="Modern ls replacement"
license="MIT"

depends=()
exports=(
    "bin/eza"
)

source=("https://github.com/eza-community/eza/releases/download/v0.20.14/eza_x86_64-unknown-linux-musl.tar.gz")
sha256sums=("SKIP")

package() {
    mkdir -p "$PKGDIR/bin"
    cp "$SRCDIR/eza" "$PKGDIR/bin/eza"
    chmod +x "$PKGDIR/bin/eza"
}
"#;

/// Build all 5 packages into a shared store and verify each one runs.
#[tokio::test]
async fn test_build_all_packages() {
    let tmp = tempfile::tempdir().unwrap();
    let store_root = tmp.path().join("store");
    let work_dir = tmp.path().join("work");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&store_root).unwrap();
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let packages: Vec<(&str, &str, &str, &str)> = vec![
        (JQ_RECIPE, "jq", "jq", "--version"),
        (RIPGREP_RECIPE, "ripgrep", "rg", "--version"),
        (FD_RECIPE, "fd", "fd", "--version"),
        (BAT_RECIPE, "bat", "bat", "--version"),
        (EZA_RECIPE, "eza", "eza", "--version"),
    ];

    for (recipe, name, binary, flag) in &packages {
        println!("Building {name}...");
        build_and_verify(&store_root, &work_dir, &cache_dir, recipe, name, binary, flag).await;
    }

    // Verify the store has all 5 packages
    let store = PackageStore::new(store_root).unwrap();
    let installed = store.list();
    assert_eq!(
        installed.len(),
        5,
        "Expected 5 packages in store, got {}: {:?}",
        installed.len(),
        installed
    );

    println!("\nAll {} packages built and verified:", installed.len());
    for id in &installed {
        println!("  {} {}", id.name, id.version);
    }
}
