#!/bin/bash
# Bingux Build & Test — complete CI pipeline
# Builds all tools, runs tests, builds ISO, verifies in QEMU.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "╔══════════════════════════════════════════╗"
echo "║       Bingux Build & Test Pipeline       ║"
echo "╚══════════════════════════════════════════╝"
echo ""

TOTAL_PASS=0
TOTAL_FAIL=0

phase() { echo ""; echo "━━━ $1 ━━━"; echo ""; }

# Phase 1: Build
phase "1. Building Bingux tools"
cargo build --release 2>&1 | tail -1
echo "  Tools built: bpkg, bsys, bxc-shim, bxc-cli"

# Phase 2: Rust tests
phase "2. Running Rust tests"
RUST_RESULT=$(cargo test 2>&1)
RUST_PASS=$(echo "$RUST_RESULT" | grep "^test result:" | awk '{sum += $4} END {print sum}')
echo "  $RUST_PASS tests passed"
TOTAL_PASS=$((TOTAL_PASS + RUST_PASS))

# Phase 3: Demo
phase "3. Running demo"
if bash scripts/demo.sh > /dev/null 2>&1; then
    echo "  Demo: PASS"
    TOTAL_PASS=$((TOTAL_PASS + 1))
else
    echo "  Demo: FAIL"
    TOTAL_FAIL=$((TOTAL_FAIL + 1))
fi

# Phase 4: Production ISO
phase "4. Building production ISO"
if bash bootstrap/stage2/iso/build-production-iso.sh > /dev/null 2>&1; then
    SIZE=$(du -h /tmp/bingux-prod.iso | cut -f1)
    PKGS=$(ls /tmp/bingux-prod-iso/store/ | wc -l)
    echo "  ISO: $SIZE, $PKGS packages"
    TOTAL_PASS=$((TOTAL_PASS + 1))
else
    echo "  ISO build: FAIL"
    TOTAL_FAIL=$((TOTAL_FAIL + 1))
fi

# Phase 5: QEMU boot test
phase "5. QEMU boot verification"
BOOT_OUTPUT=$(timeout 20 qemu-system-x86_64 -enable-kvm -m 2G \
    -kernel /tmp/bingux-prod-iso/isoroot/boot/vmlinuz \
    -initrd /tmp/bingux-prod-iso/isoroot/boot/initramfs.img \
    -append "rdinit=/usr/lib/systemd/systemd console=ttyS0 selinux=0" \
    -nographic -no-reboot 2>&1 || true)

if echo "$BOOT_OUTPUT" | grep -q "Finished.*bingux-welcome"; then
    echo "  systemd boot: PASS"
    TOTAL_PASS=$((TOTAL_PASS + 1))
else
    echo "  systemd boot: FAIL"
    TOTAL_FAIL=$((TOTAL_FAIL + 1))
fi

if echo "$BOOT_OUTPUT" | grep -q "Started.*bingux-shell"; then
    echo "  interactive shell: PASS"
    TOTAL_PASS=$((TOTAL_PASS + 1))
else
    echo "  interactive shell: FAIL"
    TOTAL_FAIL=$((TOTAL_FAIL + 1))
fi

PKG_COUNT=$(echo "$BOOT_OUTPUT" | grep -c "kept" || echo 0)
echo "  packages in boot: $PKG_COUNT"
if [ "$PKG_COUNT" -gt 10 ]; then
    echo "  package count: PASS (>10)"
    TOTAL_PASS=$((TOTAL_PASS + 1))
else
    echo "  package count: FAIL (<10)"
    TOTAL_FAIL=$((TOTAL_FAIL + 1))
fi

# Phase 6: Self-hosting test
phase "6. Self-hosting test"
if bash testing/smoke-tests/run-selfhost-test.sh > /tmp/bingux-ci-selfhost.log 2>&1; then
    SH_PASS=$(grep -c "PASS:" /tmp/bingux-ci-selfhost.log)
    echo "  Self-hosting: $SH_PASS checks passed"
    TOTAL_PASS=$((TOTAL_PASS + SH_PASS))
else
    echo "  Self-hosting: FAIL"
    TOTAL_FAIL=$((TOTAL_FAIL + 1))
fi

# Summary
echo ""
echo "╔══════════════════════════════════════════╗"
echo "║  Results: $TOTAL_PASS passed, $TOTAL_FAIL failed"
echo "╚══════════════════════════════════════════╝"
echo ""

[ "$TOTAL_FAIL" -eq 0 ] && echo "ALL TESTS PASSED" || exit 1
