#!/bin/bash
# Run all Bingux tests: Rust unit/integration tests + QEMU e2e tests
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

TOTAL_PASS=0
TOTAL_FAIL=0

echo "============================================"
echo "  Bingux Full Test Suite"
echo "============================================"
echo ""

# Phase 1: Rust tests
echo "==> Phase 1: Rust unit & integration tests"
RUST_RESULT=$(cargo test 2>&1)
RUST_PASS=$(echo "$RUST_RESULT" | grep "^test result:" | awk '{sum += $4} END {print sum}')
RUST_FAIL=$(echo "$RUST_RESULT" | grep "^test result:" | awk '{sum += $6} END {print sum}')
echo "    $RUST_PASS passed, $RUST_FAIL failed"
TOTAL_PASS=$((TOTAL_PASS + RUST_PASS))
TOTAL_FAIL=$((TOTAL_FAIL + RUST_FAIL))
echo ""

# Phase 2: QEMU boot test
echo "==> Phase 2: QEMU boot test (22 checks)"
if bash "$SCRIPT_DIR/smoke-tests/run-boot-test.sh" > /tmp/bingux-boot-result.log 2>&1; then
    BOOT_PASS=$(grep -c "PASS:" /tmp/bingux-boot-result.log)
    echo "    $BOOT_PASS checks passed"
    TOTAL_PASS=$((TOTAL_PASS + BOOT_PASS))
else
    BOOT_FAIL=$(grep -c "FAIL:" /tmp/bingux-boot-result.log 2>/dev/null || echo 1)
    echo "    FAILED ($BOOT_FAIL checks)"
    TOTAL_FAIL=$((TOTAL_FAIL + BOOT_FAIL))
fi
echo ""

# Phase 3: Interactive lifecycle test
echo "==> Phase 3: QEMU lifecycle test (12 checks)"
if bash "$SCRIPT_DIR/smoke-tests/run-interactive-test.sh" > /tmp/bingux-lifecycle-result.log 2>&1; then
    LC_PASS=$(grep -c "PASS:" /tmp/bingux-lifecycle-result.log)
    echo "    $LC_PASS checks passed"
    TOTAL_PASS=$((TOTAL_PASS + LC_PASS))
else
    LC_FAIL=$(grep -c "FAIL:" /tmp/bingux-lifecycle-result.log 2>/dev/null || echo 1)
    echo "    FAILED ($LC_FAIL checks)"
    TOTAL_FAIL=$((TOTAL_FAIL + LC_FAIL))
fi
echo ""

# Phase 4: System config test
echo "==> Phase 4: QEMU system config test (7 checks)"
if bash "$SCRIPT_DIR/smoke-tests/run-system-test.sh" > /tmp/bingux-system-result.log 2>&1; then
    SYS_PASS=$(grep -c "PASS:" /tmp/bingux-system-result.log)
    echo "    $SYS_PASS checks passed"
    TOTAL_PASS=$((TOTAL_PASS + SYS_PASS))
else
    SYS_FAIL=$(grep -c "FAIL:" /tmp/bingux-system-result.log 2>/dev/null || echo 1)
    echo "    FAILED ($SYS_FAIL checks)"
    TOTAL_FAIL=$((TOTAL_FAIL + SYS_FAIL))
fi
echo ""

# Phase 5: Permission system test
echo "==> Phase 5: QEMU permission test (5 checks)"
if bash "$SCRIPT_DIR/smoke-tests/run-permissions-test.sh" > /tmp/bingux-perms-result.log 2>&1; then
    PERM_PASS=$(grep -c "PASS:" /tmp/bingux-perms-result.log)
    echo "    $PERM_PASS checks passed"
    TOTAL_PASS=$((TOTAL_PASS + PERM_PASS))
else
    PERM_FAIL=$(grep -c "FAIL:" /tmp/bingux-perms-result.log 2>/dev/null || echo 1)
    echo "    FAILED ($PERM_FAIL checks)"
    TOTAL_FAIL=$((TOTAL_FAIL + PERM_FAIL))
fi
echo ""

# Summary
echo "============================================"
echo "  TOTAL: $TOTAL_PASS passed, $TOTAL_FAIL failed"
echo "============================================"
echo ""
echo "  Rust tests:        $RUST_PASS"
echo "  QEMU boot:         $(grep -c "PASS:" /tmp/bingux-boot-result.log 2>/dev/null || echo 0)"
echo "  QEMU lifecycle:    $(grep -c "PASS:" /tmp/bingux-lifecycle-result.log 2>/dev/null || echo 0)"
echo "  QEMU system:       $(grep -c "PASS:" /tmp/bingux-system-result.log 2>/dev/null || echo 0)"
echo "  QEMU permissions:  $(grep -c "PASS:" /tmp/bingux-perms-result.log 2>/dev/null || echo 0)"
echo ""

[ "$TOTAL_FAIL" -eq 0 ] && echo "ALL TESTS PASSED" || exit 1
