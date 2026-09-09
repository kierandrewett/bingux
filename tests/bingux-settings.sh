#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$test_dir/"
cp "$repo_dir/shell/bingux/"*.js "$test_dir/"
cp "$repo_dir/shell/bingux/settings-backend.py" "$test_dir/"
cp "$repo_dir/tests/bingux-settings.qml" "$test_dir/shell.qml"
cp "$repo_dir/shell/bingux/qmldir" "$test_dir/"
# Test the actual content in a native window that cannot interrupt the user's typing.
sed -i 's/flags: Qt.Window | Qt.FramelessWindowHint/flags: Qt.Window | Qt.FramelessWindowHint | Qt.WindowDoesNotAcceptFocus/' "$test_dir/BinguxSettings.qml"
mkdir -p "$test_dir/bin" "$test_dir/config/bingux"
printf '#!/bin/sh\nexit 0\n' > "$test_dir/bin/systemctl"
chmod +x "$test_dir/bin/systemctl"
printf '%s' '{}' > "$test_dir/config/bingux/settings.json"
BINGUX_SETTINGS_REPORT="$test_dir/report" NO_AT_BRIDGE=1 PATH="$test_dir/bin:$PATH" XDG_CONFIG_HOME="$test_dir/config" timeout 25s dbus-run-session -- "${BINGUX_QUICKSHELL:-quickshell}" -p "$test_dir" --no-color > "$test_dir/log" 2>&1 || { cat "$test_dir/log"; exit 1; }
cat "$test_dir/report"
grep 'BINGUX_SETTINGS_PASS' "$test_dir/report" || { cat "$test_dir/log"; exit 1; }
if grep -E 'ReferenceError|TypeError|Binding loop|FAIL!' "$test_dir/log"; then cat "$test_dir/log"; exit 1; fi
