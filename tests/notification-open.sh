#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$repo_dir"/shell/bingux/*.js "$repo_dir/shell/bingux/qmldir" "$test_dir/"
cp "$repo_dir/tests/notification-open.qml" "$test_dir/shell.qml"
if ! timeout 20s "${BINGUX_QUICKSHELL:-quickshell}" -p "$test_dir" --no-color >"$test_dir/log" 2>&1; then
    cat "$test_dir/log"
    exit 1
fi
cat "$test_dir/log"
grep -q 'PASS: retained history opens with intermediate animation frames' "$test_dir/log"
if grep -E 'FAIL:|ERROR|FATAL|Binding loop' "$test_dir/log"; then exit 1; fi
