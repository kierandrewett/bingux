#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT

cp "$repo_dir"/shell/bingux/{NotificationHistory.js,NotificationState.qml} "$test_dir/"
cp "$repo_dir/tests/notification-afk.qml" "$test_dir/shell.qml"
printf '%s\n' 'NotificationState 1.0 NotificationState.qml' >"$test_dir/qmldir"

export BINGUX_NOTIFICATION_TEST_RESULTS="$test_dir/results.txt"
timeout 15s dbus-run-session -- quickshell -p "$test_dir" --no-color >"$test_dir/runtime.log" 2>&1 || {
    cat "$test_dir/runtime.log"
    exit 1
}
cat "$test_dir/results.txt"
grep -q '^FAILURES 0$' "$test_dir/results.txt"
