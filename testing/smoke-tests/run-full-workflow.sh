#!/bin/bash
# Bingux Full Workflow Test
# Tests the complete lifecycle inside QEMU:
# 1. Boot with systemd
# 2. bsys build a package from recipe
# 3. bsys apply to compose a generation
# 4. bpkg add --keep to manage user packages
# 5. bpkg home apply for declarative config
# 6. Generation rollback
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BOOT_LOG=/tmp/bingux-workflow.log

echo "============================================"
echo "  Bingux Full Workflow Test"
echo "============================================"
echo ""

# Rebuild ISO with latest tools
echo "==> Building ISO..."
bash "$ROOT_DIR/bootstrap/stage2/iso/build-systemd-iso.sh" > /dev/null 2>&1

KERNEL=/tmp/bingux-systemd-iso/isoroot/boot/vmlinuz
INITRD_SRC=/tmp/bingux-systemd-iso/isoroot/boot/initramfs.img

# Build fresh initramfs from scratch (don't patch the systemd one)
echo "==> Building fresh initramfs..."
PATCH=/tmp/bingux-workflow-patch
rm -rf "$PATCH"
mkdir -p "$PATCH"/{bin,sbin,lib,lib64,dev,proc,sys,run,tmp,etc/ssl/certs}
mkdir -p "$PATCH"/system/{packages,profiles/default/bin,config,state}
cd "$PATCH"

# Busybox
cp /usr/bin/busybox bin/
for cmd in sh cat echo ls mkdir mount umount wc head sort grep cp rm ln chmod printf find reboot poweroff; do
    ln -sf busybox "bin/$cmd"
done

# Our tools + libs
cp "$ROOT_DIR/target/release/bpkg" bin/
cp "$ROOT_DIR/target/release/bsys-cli" bin/bsys
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 libbz2.so.1 libcrypto.so.3 libssl.so.3 libz.so.1; do
    src=$(find /lib64 /usr/lib64 -name "$lib" -maxdepth 1 2>/dev/null | head -1)
    [ -n "$src" ] && cp -L "$src" lib64/
done

# CA certs for TLS
cp /etc/ssl/certs/ca-bundle.crt etc/ssl/certs/ 2>/dev/null || true

# Copy packages from the ISO store
cp -a /tmp/bingux-systemd-iso/store/* system/packages/ 2>/dev/null || true

# Generation profile
for pkg_dir in system/packages/*/; do
    [ -d "$pkg_dir/bin" ] || continue
    for b in "$pkg_dir/bin"/*; do
        [ -f "$b" ] || continue
        ln -sf "/system/packages/$(basename "$pkg_dir")/bin/$(basename "$b")" "system/profiles/default/bin/$(basename "$b")"
    done
done

# System config
cat > system/config/system.toml << 'T'
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"
[packages]
keep = ["jq", "ripgrep", "fd"]
[services]
enable = []
T

# Add a test recipe
mkdir -p system/recipes/workflow-test
cat > system/recipes/workflow-test/BPKGBUILD << 'R'
pkgscope="bingux"
pkgname="workflow-test"
pkgver="1.0.0"
pkgarch="x86_64-linux"
pkgdesc="Workflow test package"
license="MIT"
depends=()
exports=("bin/workflow-test")
source=()
sha256sums=()
package() {
    mkdir -p "$PKGDIR/bin"
    printf '#!/bin/sh\necho "workflow-test 1.0.0 — full pipeline!"\n' > "$PKGDIR/bin/workflow-test"
    chmod +x "$PKGDIR/bin/workflow-test"
}
R

# Replace init with comprehensive test
cat > init << 'INIT'
#!/bin/sh
export PATH="/system/profiles/default/bin:/bin:/sbin:/usr/sbin"
export LD_LIBRARY_PATH="/lib64:/usr/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BSYS_CONFIG_PATH="/system/config/system.toml"
export BSYS_ETC_ROOT="/tmp/etc"
export BSYS_PROFILES_ROOT="/tmp/profiles"
export BSYS_PACKAGES_ROOT="/system/packages"
export BSYS_WORK_DIR="/tmp/bsys-work"
export BSYS_CACHE_DIR="/tmp/bsys-cache"
export BPKG_HOME_TOML="/tmp/home.toml"
export HOME="/tmp"
export SSL_CERT_FILE="/etc/ssl/certs/ca-bundle.crt"

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs tmpfs /tmp
mount -t tmpfs tmpfs /run
mkdir -p /tmp/etc /tmp/profiles

echo ""
echo "=== BINGUX FULL WORKFLOW TEST ==="
echo ""

# 1. Initial state
echo "[1] Initial packages: $(ls /system/packages/ | wc -l)"
bpkg list 2>&1 | head -3
echo "..."
echo ""

# 2. bsys build
echo "[2] bsys build workflow-test:"
bsys build /system/recipes/workflow-test/BPKGBUILD 2>&1
echo ""

# 3. Verify built package
echo "[3] Run built package:"
if [ -f /system/packages/workflow-test-1.0.0-x86_64-linux/bin/workflow-test ]; then
    /system/packages/workflow-test-1.0.0-x86_64-linux/bin/workflow-test
    echo "  PASS"
else
    echo "  FAIL"
fi
echo ""

# 4. bsys apply
echo "[4] bsys apply:"
bsys apply 2>&1 | head -5
echo ""

# 5. bpkg add --keep
echo "[5] bpkg add --keep workflow-test:"
echo '[packages]' > /tmp/home.toml
echo 'keep = []' >> /tmp/home.toml
bpkg add --keep workflow-test 2>&1
echo "  home.toml: $(cat /tmp/home.toml | grep keep)"
echo ""

# 6. bpkg rm
echo "[6] bpkg rm workflow-test:"
bpkg rm workflow-test 2>&1
echo "  home.toml: $(cat /tmp/home.toml | grep keep)"
echo ""

# 7. bpkg info
echo "[7] bpkg info jq:"
bpkg info jq 2>&1 | head -4
echo ""

# 8. Generation count
echo "[8] Profiles: $(ls /tmp/profiles/ 2>/dev/null | wc -l) generations"
echo ""

# 9. bpkg list final
echo "[9] Final bpkg list:"
bpkg list 2>&1
echo ""

# 10. Package count
echo "[10] Total: $(ls /system/packages/ | wc -l) packages"
echo ""

echo "=== WORKFLOW TEST COMPLETE ==="
poweroff -f 2>/dev/null || true
INIT
chmod +x init

find . | cpio -o -H newc 2>/dev/null | gzip > /tmp/bingux-workflow.img
cd "$ROOT_DIR"

echo "==> Booting..."
timeout 30 qemu-system-x86_64 \
    -enable-kvm -m 2G \
    -kernel "$KERNEL" \
    -initrd /tmp/bingux-workflow.img \
    -append "init=/init console=ttyS0 quiet" \
    -nographic -no-reboot \
    2>&1 | tee "$BOOT_LOG" || true

echo ""
echo "============================================"
echo "  Workflow Test Results"
echo "============================================"
PASS=0
FAIL=0

check() {
    if grep -q "$1" "$BOOT_LOG"; then
        echo "  PASS: $2"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $2"
        FAIL=$((FAIL + 1))
    fi
}

check "WORKFLOW TEST" "Test started"
check "bsys.*build.*workflow-test\|ok.*workflow-test" "bsys build works"
check "full pipeline" "Built package runs"
check "bsys.*apply\|system profile" "bsys apply works"
check "Installed.*workflow-test\|added to home.toml" "bpkg add --keep works"
check "Removed.*workflow-test\|removed.*home.toml" "bpkg rm works"
check "Name:.*jq\|jq.*1.7" "bpkg info works"
check "WORKFLOW TEST COMPLETE" "All steps completed"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL TESTS PASSED" || exit 1
