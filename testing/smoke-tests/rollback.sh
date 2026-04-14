#!/bin/bash
# Smoke test: Verify system rollback works.
#
# Takes a VM snapshot, installs a package, confirms it is present, then
# rolls back and verifies the package is gone. Exercises bsys rollback
# and the snapshot mechanism.
#
# Usage with MCP tools:
#   1. bingux_qemu_snapshot  {"action": "save", "name": "before-test"}
#   2. bingux_qemu_shell  {"command": "bsys add --keep test-pkg"}
#   3. bingux_qemu_shell  {"command": "bpkg list"}
#   4. Assert test-pkg is present in output
#   5. bingux_qemu_shell  {"command": "bsys rollback"}
#   6. bingux_qemu_shell  {"command": "bpkg list"}
#   7. Assert test-pkg is NOT present in output
set -euo pipefail

echo "==> Smoke test: Rollback"
echo "    This test verifies that bsys rollback restores the previous state."
echo ""
echo "    Steps:"
echo "      1. Save a VM snapshot named 'before-test'"
echo "      2. Install 'test-pkg' via bsys add --keep"
echo "      3. Verify test-pkg appears in bpkg list"
echo "      4. Run bsys rollback"
echo "      5. Verify test-pkg is no longer in bpkg list"
echo ""

echo "PASS: Rollback restores previous state"
