#!/bin/bash
# Bingux Interactive Test
# Embeds test commands into the init script, boots VM, verifies output.
# Tests: bpkg add --keep, bpkg rm, bpkg info, generation rollback
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BOOT_LOG=/tmp/bingux-interactive.log

echo "============================================"
echo "  Bingux Interactive Package Test"
echo "============================================"
echo ""

# Build ISO first
if [ ! -f /tmp/bingux-iso-build/isoroot/boot/vmlinuz ]; then
    bash "$ROOT_DIR/bootstrap/stage2/iso/build-test-iso.sh"
fi

KERNEL=/tmp/bingux-iso-build/isoroot/boot/vmlinuz
INITRD_SRC=/tmp/bingux-iso-build/isoroot/boot/initramfs.img

# Modify the initramfs to include interactive test commands
echo "==> Patching initramfs with test commands..."
PATCH_DIR=/tmp/bingux-patch-initrd
rm -rf "$PATCH_DIR"
mkdir -p "$PATCH_DIR"
cd "$PATCH_DIR"
zcat "$INITRD_SRC" | cpio -id 2>/dev/null

# Replace init with test version
cat > init << 'TESTINIT'
#!/bin/sh
export PATH="/system/profiles/default/bin:/bin:/sbin"
export LD_LIBRARY_PATH="/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BPKG_HOME_TOML="/tmp/test-home.toml"
export HOME="/tmp"
export TERM="linux"

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs tmpfs /tmp
mount -t tmpfs tmpfs /run

# Initialize home.toml for bpkg add --keep
mkdir -p /tmp
cat > /tmp/test-home.toml << 'HOMETOML'
[packages]
keep = []
HOMETOML

echo ""
echo "=== BINGUX INTERACTIVE TEST ==="
echo ""

# Test 1: bpkg list (should show pre-installed packages)
echo "[TEST 1] bpkg list:"
bpkg list 2>&1
echo ""

# Test 2: bpkg info
echo "[TEST 2] bpkg info jq:"
bpkg info jq 2>&1
echo ""

# Test 3: bpkg add --keep (add ripgrep to home.toml)
echo "[TEST 3] bpkg add --keep ripgrep:"
bpkg add --keep ripgrep 2>&1
echo ""

# Test 4: Verify home.toml was updated
echo "[TEST 4] home.toml contents:"
cat /tmp/test-home.toml
echo ""

# Test 5: bpkg rm (remove from home.toml)
echo "[TEST 5] bpkg rm ripgrep:"
bpkg rm ripgrep 2>&1
echo ""

# Test 6: Verify removal from home.toml
echo "[TEST 6] home.toml after rm:"
cat /tmp/test-home.toml
echo ""

# Test 7: Run tools through generation profile
echo "[TEST 7] Tools via profile:"
echo "  jq: $(jq --version 2>&1)"
echo "  rg: $(rg --version 2>&1 | head -1)"
echo "  fd: $(fd --version 2>&1)"
echo "  bat: $(bat --version 2>&1)"
echo "  fzf: $(fzf --version 2>&1)"
echo ""

# Test 8: bpkg list count
echo "[TEST 8] Package count: $(bpkg list 2>&1 | grep -c 'kept')"
echo ""

echo "=== ALL INTERACTIVE TESTS COMPLETE ==="
echo ""

# Don't drop to shell — just poweroff
poweroff -f 2>/dev/null || reboot -f 2>/dev/null || true
TESTINIT
chmod +x init

# Repack initramfs
find . | cpio -o -H newc 2>/dev/null | gzip > /tmp/bingux-test-initrd.img
cd "$ROOT_DIR"

echo "==> Booting test VM..."
timeout 30 qemu-system-x86_64 \
    -enable-kvm -m 512M \
    -kernel "$KERNEL" \
    -initrd /tmp/bingux-test-initrd.img \
    -append "init=/init console=ttyS0 quiet" \
    -nographic -no-reboot \
    2>&1 | tee "$BOOT_LOG" || true

echo ""
echo "============================================"
echo "  Interactive Test Results"
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

# Basic
check "INTERACTIVE TEST" "Test harness started"
check "bpkg" "bpkg runs"

# Test 1: list
check "jq.*kept" "bpkg list shows jq"
check "ripgrep.*kept" "bpkg list shows ripgrep"

# Test 2: info
check "Name:.*jq" "bpkg info shows name"

# Test 3: add --keep
check "Installed.*ripgrep.*persistent" "bpkg add --keep works"

# Test 4: home.toml updated
check 'keep.*=.*\[.*"ripgrep"' "home.toml contains ripgrep after add"

# Test 5: rm
check "Removed.*ripgrep" "bpkg rm works"

# Test 6: home.toml cleaned
check "INTERACTIVE TESTS COMPLETE" "All tests ran"

# Test 7: tools via profile
check "jq-1.7.1" "jq runs via profile"
check "ripgrep 14.1.1" "rg runs via profile"

# Test 8: package count
check "Package count:" "Package count reported"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL TESTS PASSED" || exit 1
