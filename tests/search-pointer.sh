#!/usr/bin/env bash
set -euo pipefail
export PATH="$PATH"
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$repo_dir"/shell/bingux/*.js "$repo_dir/shell/bingux/qmldir" "$test_dir/"
cp -R "$repo_dir/shell/bingux/preview-assets" "$test_dir/"
cp "$repo_dir/tests/search-pointer.qml" "$test_dir/shell.qml"
# Keep the fixture independent of daemon replies and application launches.
sed -i 's@return runtimeDirectory ? runtimeDirectory + "/bingux/search-v1.sock" : "";@return "";@' "$test_dir/SearchSocket.qml"
for reduced in 0 1; do
    BINGUX_REDUCED_MOTION="$reduced" NO_AT_BRIDGE=1 timeout 15s "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" --no-color >"$test_dir/runtime.log" 2>&1 || {
        cat "$test_dir/runtime.log"
        exit 1
    }
    grep -q 'SEARCH_POINTER_PASS' "$test_dir/runtime.log" || {
        cat "$test_dir/runtime.log"
        exit 1
    }
    echo "PASS: search pointer interaction (reduced motion: $reduced)"
done
