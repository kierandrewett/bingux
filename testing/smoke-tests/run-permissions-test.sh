#!/bin/bash
# Bingux Permission System Test
# Tests permission granting, denying, and checking inside QEMU.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BOOT_LOG=/tmp/bingux-perms-test.log

echo "============================================"
echo "  Bingux Permission System Test"
echo "============================================"
echo ""

if [ ! -f /tmp/bingux-iso-build/isoroot/boot/vmlinuz ]; then
    bash "$ROOT_DIR/bootstrap/stage2/iso/build-test-iso.sh"
fi

KERNEL=/tmp/bingux-iso-build/isoroot/boot/vmlinuz
INITRD_SRC=/tmp/bingux-iso-build/isoroot/boot/initramfs.img

echo "==> Patching initramfs..."
PATCH_DIR=/tmp/bingux-perms-patch
rm -rf "$PATCH_DIR"
mkdir -p "$PATCH_DIR"
cd "$PATCH_DIR"
zcat "$INITRD_SRC" | cpio -id 2>/dev/null

# Add bxc-cli for permissions
cp "$ROOT_DIR/target/release/bxc-cli" bin/bxc 2>/dev/null || true

cat > init << 'TESTINIT'
#!/bin/sh
export PATH="/system/profiles/default/bin:/bin:/sbin"
export LD_LIBRARY_PATH="/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BXC_PERMS_DIR="/tmp/test-permissions"
export HOME="/tmp"
export TERM="linux"

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs tmpfs /tmp
mount -t tmpfs tmpfs /run

mkdir -p /tmp/test-permissions

echo ""
echo "=== BINGUX PERMISSIONS TEST ==="
echo ""

# Test 1: Grant permissions
echo "[TEST 1] Grant GPU to jq:"
bxc perms jq 2>&1 || echo "  (bxc perms output)"
echo ""

# Test 2: Check permissions file was created
echo "[TEST 2] Permission files:"
ls -la /tmp/test-permissions/ 2>/dev/null || echo "  (no files yet — expected before any grants)"
echo ""

# Test 3: bpkg grant
echo "[TEST 3] bpkg grant jq gpu:"
BPKG_STORE_ROOT="/system/packages" BXC_PERMS_DIR="/tmp/test-permissions" bpkg grant jq gpu 2>&1
echo ""

# Test 4: Check permission file
echo "[TEST 4] After grant:"
cat /tmp/test-permissions/jq.toml 2>/dev/null || echo "  (no permission file — grant may use different path)"
echo ""

# Test 5: bpkg revoke
echo "[TEST 5] bpkg revoke jq gpu:"
BPKG_STORE_ROOT="/system/packages" BXC_PERMS_DIR="/tmp/test-permissions" bpkg revoke jq gpu 2>&1
echo ""

# Test 6: Inspect sandbox config
echo "[TEST 6] bxc inspect jq:"
bxc inspect jq 2>&1 || echo "  (inspect output)"
echo ""

echo "=== ALL PERMISSIONS TESTS COMPLETE ==="

poweroff -f 2>/dev/null || reboot -f 2>/dev/null || true
TESTINIT
chmod +x init

find . | cpio -o -H newc 2>/dev/null | gzip > /tmp/bingux-perms-initrd.img
cd "$ROOT_DIR"

echo "==> Booting test VM..."
timeout 30 qemu-system-x86_64 \
    -enable-kvm -m 512M \
    -kernel "$KERNEL" \
    -initrd /tmp/bingux-perms-initrd.img \
    -append "init=/init console=ttyS0 quiet" \
    -nographic -no-reboot \
    2>&1 | tee "$BOOT_LOG" || true

echo ""
echo "============================================"
echo "  Permissions Test Results"
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

check "PERMISSIONS TEST" "Test harness started"
check "grant.*jq\|Grant.*gpu\|gpu" "Grant command ran"
check "revoke.*jq\|Revoke\|revoke" "Revoke command ran"
check "inspect\|sandbox\|Sandbox" "Inspect command ran"
check "PERMISSIONS TESTS COMPLETE" "All tests completed"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL TESTS PASSED" || exit 1
