#!/bin/bash
# Automated boot test: build ISO, boot in QEMU, verify output.
# Exits 0 if all checks pass, 1 otherwise.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
ISO_SCRIPT="$ROOT_DIR/bootstrap/stage2/iso/build-test-iso.sh"
BOOT_LOG="/tmp/bingux-boot.log"

echo "============================================"
echo "  Bingux Automated Boot Test"
echo "============================================"
echo ""

# --- Step 1: Build the ISO ---
echo "==> Step 1: Building ISO..."
bash "$ISO_SCRIPT"
echo ""

# --- Step 2: Boot in QEMU ---
KERNEL="/tmp/bingux-iso-build/isoroot/boot/vmlinuz"
INITRD="/tmp/bingux-iso-build/isoroot/boot/initramfs.img"

if [ ! -f "$KERNEL" ] || [ ! -f "$INITRD" ]; then
    echo "FATAL: Kernel or initramfs not found after build"
    exit 1
fi

# Detect KVM support
ACCEL_ARGS=""
if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
    ACCEL_ARGS="-enable-kvm -cpu host"
else
    ACCEL_ARGS="-cpu max"
    echo "WARN: KVM not available, using software emulation (will be slower)"
fi

echo "==> Step 2: Booting in QEMU (30s timeout)..."
timeout 30 qemu-system-x86_64 \
    $ACCEL_ARGS -m 512M \
    -kernel "$KERNEL" -initrd "$INITRD" \
    -append "init=/init console=ttyS0 quiet" \
    -nographic -no-reboot \
    2>&1 | tee "$BOOT_LOG" || true

# --- Step 3: Verify output ---
echo ""
echo "============================================"
echo "  Verification Results"
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

check "Bingux" "Bingux banner displayed"
check "Hello from Bingux" "Hello message shown"
check "bpkg" "bpkg output visible"
check "jq" "jq package listed/verified"
check "ripgrep" "ripgrep package listed/verified"
check "fd" "fd package listed/verified"
check "bat" "bat package listed/verified"
check "eza" "eza package listed/verified"
check "OK.*jq" "jq version check passed"
check "OK.*rg" "rg version check passed"
check "OK.*fd" "fd version check passed"
check "OK.*bat" "bat version check passed"
check "OK.*eza" "eza version check passed"
check "Ready" "Init completed successfully"

echo ""
echo "Results: $PASS passed, $FAIL failed"
echo ""

if [ "$FAIL" -eq 0 ]; then
    echo "ALL TESTS PASSED"
    exit 0
else
    echo "SOME TESTS FAILED"
    echo ""
    echo "Full boot log: $BOOT_LOG"
    exit 1
fi
