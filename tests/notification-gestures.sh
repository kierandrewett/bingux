#!/usr/bin/env bash
# Run real Qt pointer events against the notification surface on a private bus.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
cp "$repo_dir/shell/bingux/NotificationHistory.js" "$test_dir/"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/{NotificationState.qml,NotificationSurface.qml,NotificationStack.qml,PanelOutline.qml,NotificationTestButton.qml,ActionButton.qml,SymbolicIcon.qml,Theme.qml} "$test_dir/"
cp "$repo_dir/tests/notification-gestures.qml" "$test_dir/shell.qml"
printf '%s\n' 'singleton Theme 1.0 Theme.qml' 'NotificationState 1.0 NotificationState.qml' 'NotificationSurface 1.0 NotificationSurface.qml' 'NotificationStack 1.0 NotificationStack.qml' 'PanelOutline 1.0 PanelOutline.qml' 'SymbolicIcon 1.0 SymbolicIcon.qml' 'NotificationTestButton 1.0 NotificationTestButton.qml' 'ActionButton 1.0 ActionButton.qml' >"$test_dir/qmldir"
mkdir -p "$test_dir/data/applications"
printf '%s\n' '[Desktop Entry]' 'Type=Application' 'Name=Files' 'Exec=true' 'Icon=system-file-manager' >"$test_dir/data/applications/org.bingux.NotificationTest.desktop"
export XDG_DATA_HOME="$test_dir/data"
cp "$repo_dir"/shell/bingux/{OsIconImage.qml,OsIcons.qml,render-os-icons.py} "$test_dir/"
printf '%s\n' 'OsIconImage 1.0 OsIconImage.qml' 'singleton OsIcons 1.0 OsIcons.qml' >>"$test_dir/qmldir"
export BINGUX_NOTIFICATION_TEST_RESULTS="$test_dir/results.txt"
export BINGUX_REDUCED_MOTION=0
timeout 30s dbus-run-session -- quickshell -p "$test_dir" --no-color >"$test_dir/runtime.log" 2>&1 || {
    cat "$test_dir/runtime.log"
    exit 1
}
cat "$test_dir/results.txt"
grep -q '^FAILURES 0$' "$test_dir/results.txt"
