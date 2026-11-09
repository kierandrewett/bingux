#!/usr/bin/env bash
# Run one QML test fixture against a copy of the shell components.
# Usage: tests/qml/run.sh TestName.qml [qs options...]
#
# Two kinds of fixture:
# - assertion tests call Qt.quit() themselves; exit status decides pass/fail.
# - scene fixtures draw a state for a screenshot harness and print a *_DONE marker
#   instead of quitting. They pass when the marker appears and no "FAIL:" line is logged.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/../.." && pwd)"
name="${1:?usage: tests/qml/run.sh TestName.qml [qs options...]}"
shift
test_dir="$(mktemp -d)"
log="$test_dir/qs.log"
trap 'rm -rf "$test_dir"' EXIT
cp -R "$repo_dir"/shell/bingux/. "$test_dir/"
cp "$repo_dir/tests/qml/$name" "$test_dir/$name"
marker="$(grep -o '[A-Z_]*_DONE' "$repo_dir/tests/qml/$name" | head -1 || true)"
status=0
if [[ -n "$marker" ]]; then
    timeout "${QS_SCENE_SECONDS:-30}" "${QS_TEST_BIN:-qs}" -p "$test_dir/$name" "$@" >"$log" 2>&1 || true
    cat "$log"
    if grep -q "FAIL:" "$log" || ! grep -q "$marker" "$log"; then
        status=1
    fi
else
    "${QS_TEST_BIN:-qs}" -p "$test_dir/$name" "$@" || status=$?
fi
exit "$status"
