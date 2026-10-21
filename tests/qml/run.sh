#!/usr/bin/env bash
# Run one QML test fixture against a copy of the shell components.
# Usage: tests/qml/run.sh TestName.qml [qs options...]
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/../.." && pwd)"
name="${1:?usage: tests/qml/run.sh TestName.qml [qs options...]}"
shift
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp -R "$repo_dir"/shell/bingux/. "$test_dir/"
cp "$repo_dir/tests/qml/$name" "$test_dir/$name"
status=0
"${QS_TEST_BIN:-qs}" -p "$test_dir/$name" "$@" || status=$?
exit "$status"
