#!/usr/bin/env bash
# Send real Qt pointer events to top-bar components on a private bus.
set -euo pipefail
export PATH="$PATH"
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
for component in Theme Pill BarControlSurface Tray TrayMenu InputSourceSelector ShellPopup MenuNavigator ActionButton; do
    cp "$repo_dir/shell/bingux/$component.qml" "$test_dir/"
done
printf '%s\n' 'singleton Theme 1.0 Theme.qml' > "$test_dir/qmldir"
for component in Pill BarControlSurface Tray TrayMenu InputSourceSelector ShellPopup MenuNavigator ActionButton; do
    printf '%s 1.0 %s.qml\n' "$component" "$component" >> "$test_dir/qmldir"
done
cp "$repo_dir/tests/top-bar-hover.qml" "$test_dir/shell.qml"
export BINGUX_TOP_BAR_TEST_RESULTS="$test_dir/results.txt"
timeout 15s dbus-run-session -- "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" --no-color > "$test_dir/runtime.log" 2>&1 || { cat "$test_dir/runtime.log"; exit 1; }
if [[ ! -f "$test_dir/results.txt" ]]; then cat "$test_dir/runtime.log"; exit 1; fi
cat "$test_dir/results.txt"
grep -q '^FAILURES 0$' "$test_dir/results.txt"
