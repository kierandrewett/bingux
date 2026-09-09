#!/usr/bin/env bash
# Send Qt pointer events to the real components with their full dependencies.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
export BINGUX_CONTROL_TEST_QML="$repo_dir/tests/top-bar-hover.qml"
export BINGUX_QUICKSHELL="${QUICKSHELL_BIN:-quickshell}"
dbus-run-session -- bash "$repo_dir/tests/control-centre.sh"
