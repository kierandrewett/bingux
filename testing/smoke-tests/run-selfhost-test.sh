#!/bin/bash
# Bingux Self-Hosting Test
# Tests the full bootstrap cycle: build → declare → compose → dispatch
# All inside a running QEMU VM.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
BOOT_LOG=/tmp/bingux-selfhost.log

echo "============================================"
echo "  Bingux Self-Hosting Test"
echo "============================================"

# Build production ISO if needed
if [ ! -f /tmp/bingux-prod-iso/isoroot/boot/vmlinuz ]; then
    echo "==> Building production ISO..."
    bash "$ROOT_DIR/bootstrap/stage2/iso/build-production-iso.sh" > /dev/null 2>&1
fi

KERNEL=/tmp/bingux-prod-iso/isoroot/boot/vmlinuz

# Build fresh initramfs
echo "==> Building test initramfs..."
FRESH=/tmp/bingux-selfhost-build
rm -rf "$FRESH"
mkdir -p "$FRESH"/{bin,sbin,lib,lib64,usr/{bin,sbin},dev,proc,sys,run,tmp,etc/ssl/certs}
mkdir -p "$FRESH"/system/{packages,profiles,config,state,recipes/greeting}

cp -a /tmp/bingux-prod-iso/store/* "$FRESH/system/packages/"
cp /usr/bin/busybox "$FRESH/bin/"
for cmd in sh cat echo ls mkdir mount umount wc head sort grep cp rm ln chmod printf find reboot poweroff readlink; do
    ln -sf busybox "$FRESH/bin/$cmd"
done
ln -sf ../../bin/busybox "$FRESH/usr/bin/sh"
ln -sf ../../bin/busybox "$FRESH/usr/sbin/mount"
cp "$ROOT_DIR/target/release/bpkg" "$FRESH/bin/"
cp "$ROOT_DIR/target/release/bsys-cli" "$FRESH/bin/bsys"
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 libbz2.so.1 libcrypto.so.3 libssl.so.3 libz.so.1; do
    src=$(find /lib64 /usr/lib64 -name "$lib" -maxdepth 1 2>/dev/null | head -1)
    [ -n "$src" ] && cp -L "$src" "$FRESH/lib64/"
done
cp /etc/ssl/certs/ca-bundle.crt "$FRESH/etc/ssl/certs/" 2>/dev/null || true

# Initial profile
mkdir -p "$FRESH/system/profiles/1/bin"
for pkg_dir in "$FRESH/system/packages"/*/; do
    [ -d "$pkg_dir/bin" ] || continue
    for b in "$pkg_dir/bin"/*; do
        [ -f "$b" ] || continue
        ln -sf "/system/packages/$(basename "$pkg_dir")/bin/$(basename "$b")" \
            "$FRESH/system/profiles/1/bin/$(basename "$b")"
    done
done
ln -sf 1 "$FRESH/system/profiles/current"

cat > "$FRESH/system/config/system.toml" << 'T'
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"
[packages]
keep = ["bpkg", "bsys", "bxc-shim", "jq", "ripgrep", "fd", "bat", "fzf"]
[services]
enable = []
T

cat > "$FRESH/system/recipes/greeting/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="greeting"
pkgver="1.0.0"
pkgarch="x86_64-linux"
pkgdesc="Self-hosted greeting"
license="MIT"
depends=()
exports=("bin/greeting")
source=()
sha256sums=()
package() { mkdir -p "$PKGDIR/bin"; printf '#!/bin/sh\necho "Built and composed by Bingux itself!"\n' > "$PKGDIR/bin/greeting"; chmod +x "$PKGDIR/bin/greeting"; }
R

cat > "$FRESH/init" << 'INIT'
#!/bin/sh
export PATH="/system/profiles/current/bin:/bin:/sbin"
export LD_LIBRARY_PATH="/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BSYS_CONFIG_PATH="/system/config/system.toml"
export BSYS_ETC_ROOT="/tmp/etc"
export BSYS_PROFILES_ROOT="/system/profiles"
export BSYS_PACKAGES_ROOT="/system/packages"
export BSYS_WORK_DIR="/tmp/bsys-work"
export BSYS_CACHE_DIR="/tmp/bsys-cache"
export HOME="/tmp"
mount -t proc proc /proc; mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs tmpfs /tmp; mount -t tmpfs tmpfs /run
mkdir -p /tmp/etc

echo "=== SELF-HOSTING TEST ==="
echo "[1] Before: $(ls /system/packages/ | wc -l) pkgs, gen $(readlink /system/profiles/current)"
bsys build /system/recipes/greeting/BPKGBUILD 2>&1
cat > /system/config/system.toml << 'SC'
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"
[packages]
keep = ["bpkg", "bsys", "bxc-shim", "jq", "ripgrep", "fd", "bat", "fzf", "greeting"]
[services]
enable = []
SC
bsys apply 2>&1
echo "[2] After: $(ls /system/packages/ | wc -l) pkgs, gen $(readlink /system/profiles/current)"
echo "[3] Profile bins: $(ls /system/profiles/current/bin/ 2>/dev/null | wc -l)"
if [ -f /system/profiles/current/bin/greeting ]; then
    /system/profiles/current/bin/greeting
    echo "PASS: self-hosted dispatch works"
else
    echo "FAIL: greeting not in profile"
fi
echo "[4] bpkg list:"
bpkg list 2>&1
echo "=== COMPLETE ==="
poweroff -f 2>/dev/null || true
INIT
chmod +x "$FRESH/init"
(cd "$FRESH" && find . | cpio -o -H newc 2>/dev/null | gzip > /tmp/bingux-selfhost.img)

echo "==> Booting..."
timeout 20 qemu-system-x86_64 -enable-kvm -m 2G \
    -kernel "$KERNEL" -initrd /tmp/bingux-selfhost.img \
    -append "init=/init console=ttyS0 quiet" \
    -nographic -no-reboot \
    2>&1 | tee "$BOOT_LOG" || true

echo ""
echo "============================================"
echo "  Results"
echo "============================================"
PASS=0; FAIL=0
check() { if grep -q "$1" "$BOOT_LOG"; then echo "  PASS: $2"; PASS=$((PASS+1)); else echo "  FAIL: $2"; FAIL=$((FAIL+1)); fi; }
check "SELF-HOSTING" "Test started"
check "ok.*greeting" "bsys build succeeds"
check "bsys.*apply\|recomposed" "bsys apply succeeds"
check "PASS.*dispatch\|Built and composed" "Greeting runs through dispatch"
check "greeting" "Greeting in bpkg list"
check "COMPLETE" "All steps done"
echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL TESTS PASSED" || exit 1
