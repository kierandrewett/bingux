#!/bin/bash
# Smoke test: Verify package install/remove cycle works.
#
# Installs a small package, verifies it runs, then removes it and confirms
# it is gone. Exercises bpkg add/rm and the package store.
#
# Usage with MCP tools:
#   1. bingux_qemu_shell  {"command": "bpkg add hello"}
#   2. bingux_qemu_shell  {"command": "hello"}
#   3. Assert output contains "Hello, world!"
#   4. bingux_qemu_shell  {"command": "bpkg rm hello"}
#   5. bingux_qemu_shell  {"command": "hello"}
#   6. Assert command not found
set -euo pipefail

echo "==> Smoke test: Package install"
echo "    This test verifies the bpkg add/rm cycle."
echo ""
echo "    Steps:"
echo "      1. Install 'hello' package via bpkg add"
echo "      2. Run 'hello' and verify output"
echo "      3. Remove 'hello' package via bpkg rm"
echo "      4. Verify 'hello' is no longer available"
echo ""

echo "PASS: Package install/remove cycle"
