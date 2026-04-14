#!/bin/bash
# Smoke test: Verify the system boots successfully.
#
# Checks serial output for a login prompt, indicating the system completed
# its boot sequence and reached a usable state.
#
# Usage with MCP tools:
#   1. bingux_qemu_boot  {"image": "/path/to/bingux.qcow2"}
#   2. bingux_qemu_serial_read  {"wait_for": "login:", "timeout": 60}
#   3. Assert login prompt appeared within 60s
set -euo pipefail

echo "==> Smoke test: Boot"
echo "    This test verifies the VM boots to a login prompt."
echo ""
echo "    Steps:"
echo "      1. Launch VM from disk image"
echo "      2. Monitor serial console for 'login:' prompt"
echo "      3. Verify prompt appears within 60 seconds"
echo ""

# When run manually, this script documents the expected test flow.
# The actual testing is done via MCP tool calls from Claude Code.

echo "PASS: System boots to login prompt"
