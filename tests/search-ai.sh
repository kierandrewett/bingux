#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$repo_dir"/shell/bingux/*.js "$repo_dir/shell/bingux/qmldir" "$test_dir/"
cp -R "$repo_dir/shell/bingux/preview-assets" "$test_dir/"
cp "$repo_dir/tests/search-ai.qml" "$test_dir/shell.qml"
cp "$repo_dir/tests/search-launch-socket.qml" "$test_dir/SearchSocket.qml"
NO_AT_BRIDGE=1 timeout 15s "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" --no-color > "$test_dir/log" 2>&1 || { cat "$test_dir/log"; exit 1; }
grep 'SEARCH_AI_PASS' "$test_dir/log" || { cat "$test_dir/log"; exit 1; }
if grep -E 'ReferenceError|TypeError|Binding loop' "$test_dir/log"; then exit 1; fi
