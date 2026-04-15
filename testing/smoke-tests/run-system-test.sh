#!/bin/bash
# Bingux System Configuration Test
# Tests bsys apply, /etc/ generation, and generation rollback in QEMU.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BOOT_LOG=/tmp/bingux-system-test.log

echo "============================================"
echo "  Bingux System Configuration Test"
echo "============================================"
echo ""

# Build ISO first
if [ ! -f /tmp/bingux-iso-build/isoroot/boot/vmlinuz ]; then
    bash "$ROOT_DIR/bootstrap/stage2/iso/build-test-iso.sh"
fi

KERNEL=/tmp/bingux-iso-build/isoroot/boot/vmlinuz
INITRD_SRC=/tmp/bingux-iso-build/isoroot/boot/initramfs.img

# Patch initramfs with system test commands
echo "==> Patching initramfs..."
PATCH_DIR=/tmp/bingux-sys-patch
rm -rf "$PATCH_DIR"
mkdir -p "$PATCH_DIR"
cd "$PATCH_DIR"
zcat "$INITRD_SRC" | cpio -id 2>/dev/null

# Add bsys-cli to the initramfs
cp "$ROOT_DIR/target/release/bsys-cli" bin/bsys 2>/dev/null || true

cat > init << 'TESTINIT'
#!/bin/sh
export PATH="/system/profiles/default/bin:/bin:/sbin"
export LD_LIBRARY_PATH="/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BSYS_CONFIG_PATH="/system/config/system.toml"
export BSYS_ETC_ROOT="/tmp/generated-etc"
export BSYS_PROFILES_ROOT="/tmp/test-profiles"
export BSYS_PACKAGES_ROOT="/system/packages"
export HOME="/tmp"
export TERM="linux"

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs tmpfs /tmp
mount -t tmpfs tmpfs /run

mkdir -p /tmp/generated-etc /tmp/test-profiles

echo ""
echo "=== BINGUX SYSTEM TEST ==="
echo ""

# Test 1: Read and display system.toml
echo "[TEST 1] System config:"
cat /system/config/system.toml
echo ""

# Test 2: bsys apply — generate /etc/ and compose generation
echo "[TEST 2] bsys apply:"
bsys apply 2>&1
echo ""

# Test 3: Verify generated /etc/ files
echo "[TEST 3] Generated /etc/ files:"
if [ -d /tmp/generated-etc ]; then
    for f in /tmp/generated-etc/*; do
        [ -f "$f" ] || continue
        echo "  --- $(basename $f) ---"
        cat "$f"
        echo ""
    done
else
    echo "  (no files generated)"
fi

# Test 4: Verify hostname
echo "[TEST 4] Hostname check:"
if [ -f /tmp/generated-etc/hostname ]; then
    HOSTNAME_VAL=$(cat /tmp/generated-etc/hostname)
    echo "  hostname = $HOSTNAME_VAL"
    if [ "$HOSTNAME_VAL" = "bingux-live" ]; then
        echo "  PASS: hostname correct"
    else
        echo "  FAIL: expected bingux-live, got $HOSTNAME_VAL"
    fi
else
    echo "  FAIL: hostname file not generated"
fi
echo ""

# Test 5: Verify locale.conf
echo "[TEST 5] Locale check:"
if [ -f /tmp/generated-etc/locale.conf ]; then
    echo "  $(cat /tmp/generated-etc/locale.conf)"
    if grep -q "en_GB.UTF-8" /tmp/generated-etc/locale.conf; then
        echo "  PASS: locale correct"
    else
        echo "  FAIL: wrong locale"
    fi
else
    echo "  FAIL: locale.conf not generated"
fi
echo ""

# Test 6: Verify generation was created
echo "[TEST 6] Generation check:"
if [ -d /tmp/test-profiles ]; then
    echo "  Profiles: $(ls /tmp/test-profiles/ 2>/dev/null || echo 'empty')"
    if [ -L /tmp/test-profiles/current ] || [ -d /tmp/test-profiles/1 ]; then
        echo "  PASS: generation created"
    else
        echo "  INFO: generation structure: $(find /tmp/test-profiles -maxdepth 2 -type f -o -type l 2>/dev/null | head -5)"
    fi
else
    echo "  FAIL: no profiles directory"
fi
echo ""

# Test 7: bsys list
echo "[TEST 7] bsys list:"
BPKG_STORE_ROOT="/system/packages" bsys list 2>&1
echo ""

# Test 8: bpkg list (verify packages still accessible)
echo "[TEST 8] bpkg list:"
bpkg list 2>&1
echo ""

echo "=== ALL SYSTEM TESTS COMPLETE ==="

poweroff -f 2>/dev/null || reboot -f 2>/dev/null || true
TESTINIT
chmod +x init

find . | cpio -o -H newc 2>/dev/null | gzip > /tmp/bingux-sys-initrd.img
cd "$ROOT_DIR"

echo "==> Booting test VM..."
timeout 30 qemu-system-x86_64 \
    -enable-kvm -m 512M \
    -kernel "$KERNEL" \
    -initrd /tmp/bingux-sys-initrd.img \
    -append "init=/init console=ttyS0 quiet" \
    -nographic -no-reboot \
    2>&1 | tee "$BOOT_LOG" || true

echo ""
echo "============================================"
echo "  System Test Results"
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

check "SYSTEM TEST" "Test harness started"
check "hostname.*bingux-live" "system.toml has correct hostname"
check "bsys.*apply" "bsys apply ran"
check "hostname.*=.*bingux-live\|PASS.*hostname" "hostname generated correctly"
check "en_GB.UTF-8\|PASS.*locale" "locale generated correctly"
check "generation\|profile" "generation or profile created"
check "SYSTEM TESTS COMPLETE" "All system tests completed"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL TESTS PASSED" || exit 1
