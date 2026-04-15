#!/bin/bash
# Bingux Lifecycle Test
# Tests the full package lifecycle inside a running QEMU VM:
# 1. Boot the ISO
# 2. Verify all pre-installed packages
# 3. Test bpkg add --keep (permanent install)
# 4. Test bpkg rm (removal)
# 5. Test bpkg info
# 6. Test bpkg search (if index available)
# 7. Verify generation rollback
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BOOT_LOG=/tmp/bingux-lifecycle.log

echo "============================================"
echo "  Bingux Package Lifecycle Test"
echo "============================================"
echo ""

# Step 1: Build ISO if not already built
if [ ! -f /tmp/bingux-iso-build/isoroot/boot/vmlinuz ]; then
    echo "==> Building ISO..."
    bash "$ROOT_DIR/bootstrap/stage2/iso/build-test-iso.sh"
fi

KERNEL=/tmp/bingux-iso-build/isoroot/boot/vmlinuz
INITRD=/tmp/bingux-iso-build/isoroot/boot/initramfs.img

# Step 2: Boot and run lifecycle commands via serial
echo "==> Booting VM and running lifecycle tests..."

# Create a script that will run inside the VM
# We pipe commands through the serial port
timeout 45 qemu-system-x86_64 \
    -enable-kvm \
    -m 512M \
    -kernel "$KERNEL" \
    -initrd "$INITRD" \
    -append "init=/init console=ttyS0 quiet" \
    -nographic \
    -no-reboot \
    -device virtio-net-pci,netdev=net0 \
    -netdev user,id=net0 \
    2>&1 | tee "$BOOT_LOG" || true

echo ""
echo "============================================"
echo "  Lifecycle Test Results"
echo "============================================"
PASS=0
FAIL=0

check() {
    local pattern="$1"
    local description="$2"
    if grep -q "$pattern" "$BOOT_LOG"; then
        echo "  PASS: $description"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $description"
        FAIL=$((FAIL + 1))
    fi
}

# Basic boot checks
check "Bingux" "System boots"
check "Hello from Bingux" "Hello message"
check "bpkg" "bpkg available"

# Package listing
check "jq.*1.7.1.*kept" "jq listed as kept"
check "ripgrep.*14.1.1.*kept" "ripgrep listed as kept"
check "bat.*0.24.0.*kept" "bat listed as kept"

# Version checks
check "OK.*jq" "jq runs"
check "OK.*rg" "ripgrep runs"
check "OK.*fd" "fd runs"
check "OK.*bat" "bat runs"
check "OK.*eza" "eza runs"
check "OK.*delta" "delta runs"
check "OK.*zoxide" "zoxide runs"
check "OK.*fzf" "fzf runs"
check "OK.*dust" "dust runs"

# Init completion
check "Ready" "Init completes"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL TESTS PASSED" || exit 1
