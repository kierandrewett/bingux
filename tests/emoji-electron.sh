#!/usr/bin/env bash
# Real Electron, picker, compositor and input events in a private Wayland session.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
export GNOBLIN_SOURCE="${GNOBLIN_SOURCE:-$(dirname "$repo_dir")/gnoblin}"
: "${BINGUX_TEST_ELECTRON:?Set BINGUX_TEST_ELECTRON to an Electron executable}"
test_dir="$(mktemp -d /tmp/bingux-emoji-test.XXXXXX)"
trap 'rmdir "$test_dir"' EXIT
export GNOBLIN_COMPOSITOR_SOCKET="$test_dir/compositor.sock"
# IBus can discover the host daemon outside the private session bus. Prevent
# that connection so test keys never enter the host input method.
export IBUS_ADDRESS="unix:path=$test_dir/no-ibus"
export GNOBLIN_TEST_DBUS_CLIENT="$repo_dir/tests/emoji-electron-input.py"
export MONITOR=1920x1080
"$GNOBLIN_SOURCE/scripts/run-gnome-shell.sh"
