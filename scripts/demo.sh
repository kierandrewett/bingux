#!/bin/bash
# Bingux Demo — showcasing the package management system
# Run this after `cargo build --release`
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DEMO_DIR=/tmp/bingux-demo
BPKG="$ROOT_DIR/target/release/bpkg"
BSYS="$ROOT_DIR/target/release/bsys-cli"

export BPKG_STORE_ROOT="$DEMO_DIR/store"
export BSYS_WORK_DIR="$DEMO_DIR/work"
export BSYS_CACHE_DIR="$DEMO_DIR/cache"
export BSYS_PROFILES_ROOT="$DEMO_DIR/profiles"
export BSYS_PACKAGES_ROOT="$DEMO_DIR/store"
export BSYS_CONFIG_PATH="$DEMO_DIR/system.toml"
export BSYS_ETC_ROOT="$DEMO_DIR/etc"
export BSYS_EXPORT_DIR="$DEMO_DIR/exports"
export BPKG_HOME_TOML="$DEMO_DIR/home.toml"
export BPKG_SESSION_ROOT="$DEMO_DIR/session"
export HOME="$DEMO_DIR/home"

rm -rf "$DEMO_DIR"
mkdir -p "$DEMO_DIR"/{store,work,cache,profiles,etc,exports,session/bin,home}
echo '[packages]' > "$DEMO_DIR/home.toml"
echo 'keep = []' >> "$DEMO_DIR/home.toml"

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║          Bingux Package Manager          ║"
echo "║                 Demo                     ║"
echo "╚══════════════════════════════════════════╝"
echo ""

# 1. Build packages
echo "━━━ 1. Build packages from BPKGBUILDs ━━━"
echo ""
mkdir -p "$DEMO_DIR/recipes/hello"
cat > "$DEMO_DIR/recipes/hello/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="hello"
pkgver="1.0.0"
pkgarch="x86_64-linux"
pkgdesc="Hello world"
license="MIT"
depends=()
exports=("bin/hello")
source=()
sha256sums=()
package() { mkdir -p "$PKGDIR/bin"; printf '#!/bin/sh\necho "Hello from Bingux!"\n' > "$PKGDIR/bin/hello"; chmod +x "$PKGDIR/bin/hello"; }
R

$BSYS build "$DEMO_DIR/recipes/hello/BPKGBUILD"

# Build jq from internet
mkdir -p "$DEMO_DIR/recipes/jq"
cat > "$DEMO_DIR/recipes/jq/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="jq"
pkgver="1.7.1"
pkgarch="x86_64-linux"
pkgdesc="JSON processor"
license="MIT"
depends=()
exports=("bin/jq")
source=("https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64")
sha256sums=("SKIP")
package() { mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/jq-linux-amd64" "$PKGDIR/bin/jq"; chmod +x "$PKGDIR/bin/jq"; }
R

$BSYS build "$DEMO_DIR/recipes/jq/BPKGBUILD"
echo ""

# 2. List packages
echo "━━━ 2. Package store ━━━"
echo ""
$BPKG list
echo ""

# 3. Compose a system generation
echo "━━━ 3. Compose system generation ━━━"
echo ""
cat > "$DEMO_DIR/system.toml" << 'T'
[system]
hostname = "bingux-demo"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"
[packages]
keep = ["hello", "jq"]
[services]
enable = []
T
$BSYS apply
echo ""

# 4. Package info
echo "━━━ 4. Package info ━━━"
echo ""
$BPKG info jq
echo ""

# 5. User package management
echo "━━━ 5. User packages (volatile + persistent) ━━━"
echo ""
echo "Add hello as volatile (session only):"
$BPKG add hello
echo ""
echo "Promote to persistent:"
$BPKG keep hello
echo ""
echo "Pin to specific version:"
$BPKG pin "jq=1.7.1"
echo ""
cat "$DEMO_DIR/home.toml"
echo ""

# 6. Export as .bgx
echo "━━━ 6. Export packages as .bgx ━━━"
echo ""
$BSYS export --all
echo ""

# 7. Generate repo index
echo "━━━ 7. Generate repository index ━━━"
echo ""
$BSYS export --index "$DEMO_DIR/exports"
echo ""

# 8. Generation history
echo "━━━ 8. Generation history ━━━"
echo ""
$BSYS history
echo ""

# 9. Garbage collection
echo "━━━ 9. Garbage collection (dry run) ━━━"
echo ""
$BSYS gc --dry-run
echo ""

# 10. Run the built package
echo "━━━ 10. Run packages ━━━"
echo ""
echo "hello:"
"$DEMO_DIR/store/hello-1.0.0-x86_64-linux/bin/hello"
echo ""
echo "jq:"
echo '{"name":"Bingux","version":"0.1.0"}' | "$DEMO_DIR/store/jq-1.7.1-x86_64-linux/bin/jq" '.name'
echo ""

echo "╔══════════════════════════════════════════╗"
echo "║           Demo complete!                 ║"
echo "╚══════════════════════════════════════════╝"
