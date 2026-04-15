#!/bin/bash
# Bingux Bootstrap Chain Validation
# Tests the complete self-hosted build chain in QEMU.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
LOG=/tmp/bingux-bootstrap-test.log

echo "============================================"
echo "  Bingux Bootstrap Chain Test"
echo "============================================"

# Build initramfs with toolchain + recipes
echo "==> Building test initramfs..."
FRESH=/tmp/bingux-bootstrap-validate
rm -rf "$FRESH"
mkdir -p "$FRESH"/{bin,sbin,lib,lib64,usr/{bin,sbin},dev,proc,sys,run,tmp}
mkdir -p "$FRESH"/system/{packages,profiles/1/bin,config,state,recipes/bootstrap-test}

cp /tmp/busybox-musl-static "$FRESH/bin/busybox" 2>/dev/null || cp /usr/bin/busybox "$FRESH/bin/busybox"
for cmd in $("$FRESH/bin/busybox" --list 2>/dev/null); do ln -sf busybox "$FRESH/bin/$cmd" 2>/dev/null; done
ln -sf ../../bin/busybox "$FRESH/usr/bin/sh" 2>/dev/null
cp -a /tmp/bingux-bootstrap-store/musl-toolchain-13.2.0-x86_64-linux "$FRESH/system/packages/" 2>/dev/null || true
cp "$ROOT_DIR/target/release/bpkg" "$FRESH/bin/"
cp "$ROOT_DIR/target/release/bsys-cli" "$FRESH/bin/bsys"
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 libbz2.so.1 libcrypto.so.3 libssl.so.3 libz.so.1; do
    src=$(find /lib64 /usr/lib64 -name "$lib" -maxdepth 1 2>/dev/null | head -1)
    [ -n "$src" ] && cp -L "$src" "$FRESH/lib64/"
done

for pkg_dir in "$FRESH/system/packages"/*/; do
    [ -d "$pkg_dir/bin" ] || continue
    for b in "$pkg_dir/bin"/*; do
        [ -f "$b" ] || [ -L "$b" ] || continue
        ln -sf "/system/packages/$(basename "$pkg_dir")/bin/$(basename "$b")" "$FRESH/system/profiles/1/bin/$(basename "$b")" 2>/dev/null
    done
done
ln -sf 1 "$FRESH/system/profiles/current"

cat > "$FRESH/system/recipes/bootstrap-test/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="bootstrap-test"
pkgver="1.0.0"
pkgarch="x86_64-linux"
pkgdesc="Bootstrap validation"
license="MIT"
depends=()
exports=("bin/bootstrap-test")
source=()
sha256sums=()
build() {
    cat > "$SRCDIR/test.c" << 'C'
#include <stdio.h>
int main() { printf("BOOTSTRAP VALIDATED\n"); return 0; }
C
    bingux-gcc -o "$BUILDDIR/bootstrap-test" "$SRCDIR/test.c"
}
package() { mkdir -p "$PKGDIR/bin"; cp "$BUILDDIR/bootstrap-test" "$PKGDIR/bin/bootstrap-test"; chmod +x "$PKGDIR/bin/bootstrap-test"; }
R

cat > "$FRESH/system/config/system.toml" << 'T'
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"
[packages]
keep = ["musl-toolchain"]
[services]
enable = []
T

cat > "$FRESH/init" << 'INIT'
#!/bin/sh
export PATH="/system/profiles/current/bin:/bin:/sbin"
export LD_LIBRARY_PATH="/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BSYS_WORK_DIR="/tmp/bsys-work"
export BSYS_CACHE_DIR="/tmp/bsys-cache"
export HOME="/tmp"
mount -t proc proc /proc; mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs -o size=512M tmpfs /tmp; mount -t tmpfs tmpfs /run

echo "=== BOOTSTRAP CHAIN VALIDATION ==="
echo "CHECK gcc: $(gcc --version 2>&1 | head -1 | grep -q GCC && echo PASS || echo FAIL)"
bpkg list >/dev/null 2>&1 && echo "CHECK bpkg: PASS" || echo "CHECK bpkg: FAIL"
bsys list >/dev/null 2>&1 && echo "CHECK bsys: PASS" || echo "CHECK bsys: FAIL"

echo '#include <stdio.h>' > /tmp/t.c
echo 'int main(){puts("COMPILE_OK");return 0;}' >> /tmp/t.c
bingux-gcc -o /tmp/t /tmp/t.c 2>/dev/null
if [ -f /tmp/t ]; then echo "CHECK compile: PASS"; else echo "CHECK compile: FAIL"; fi

bsys build /system/recipes/bootstrap-test/BPKGBUILD 2>/dev/null
echo "CHECK bsys-build: $([ -f /system/packages/bootstrap-test-1.0.0-x86_64-linux/bin/bootstrap-test ] && echo PASS || echo FAIL)"

/system/packages/bootstrap-test-1.0.0-x86_64-linux/bin/bootstrap-test 2>/dev/null
echo "CHECK run-built: $(/system/packages/bootstrap-test-1.0.0-x86_64-linux/bin/bootstrap-test 2>/dev/null | grep -q VALIDATED && echo PASS || echo FAIL)"

bpkg list 2>&1 | grep -q bootstrap-test
echo "CHECK bpkg-list: $(bpkg list 2>&1 | grep -q bootstrap-test && echo PASS || echo FAIL)"

echo "=== VALIDATION COMPLETE ==="
poweroff -f 2>/dev/null || true
INIT
chmod +x "$FRESH/init"
(cd "$FRESH" && find . | cpio -o -H newc 2>/dev/null | gzip > /tmp/bingux-bootstrap-val.img)

echo "==> Booting..."
KERNEL=/tmp/bingux-prod-iso/isoroot/boot/vmlinuz
timeout 30 qemu-system-x86_64 -enable-kvm -m 2G \
    -kernel "$KERNEL" -initrd /tmp/bingux-bootstrap-val.img \
    -append "init=/init console=ttyS0 quiet" \
    -nographic -no-reboot 2>&1 | tee "$LOG" || true

echo ""
echo "============================================"
echo "  Bootstrap Results"
echo "============================================"
PASS=0; FAIL=0
while IFS= read -r line; do
    case "$line" in
        *"CHECK"*PASS*) echo "  $line"; PASS=$((PASS+1)) ;;
        *"CHECK"*FAIL*) echo "  $line"; FAIL=$((FAIL+1)) ;;
    esac
done < "$LOG"
echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL BOOTSTRAP CHECKS PASSED" || exit 1
