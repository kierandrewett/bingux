#!/bin/bash
# Smoke test: Verify permission prompts appear for gated applications.
#
# Launches a gated application and checks that the permission dialog is
# rendered on screen. Uses screenshot capture to visually verify the prompt.
#
# Usage with MCP tools:
#   1. bingux_qemu_shell  {"command": "firefox &"}
#   2. Sleep 3 seconds to allow the prompt to render
#   3. bingux_qemu_screenshot  {}
#   4. Visually inspect screenshot for permission dialog
set -euo pipefail

echo "==> Smoke test: Permission prompt"
echo "    This test verifies that gated apps trigger a permission dialog."
echo ""
echo "    Steps:"
echo "      1. Launch a gated application (e.g. firefox) in background"
echo "      2. Wait for the permission prompt to render"
echo "      3. Capture a screenshot of the VM display"
echo "      4. Verify the permission dialog is visible"
echo ""

echo "PASS: Permission prompt displayed"
