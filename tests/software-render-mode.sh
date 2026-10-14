#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir/shell/bingux/Theme.qml" "$test_dir/Theme.qml"
cp "$repo_dir/shell/bingux/LockTheme.qml" "$test_dir/LockTheme.qml"
printf '%s\n' 'singleton Theme 1.0 Theme.qml' 'singleton LockTheme 1.0 LockTheme.qml' >"$test_dir/qmldir"
cp "$repo_dir/tests/software-render-mode.qml" "$test_dir/shell.qml"
mkdir -p "$test_dir/config"
for mode in 0 1; do
    result="$test_dir/result-$mode.txt"
    BINGUX_SOFTWARE_RENDERING="$mode" \
        BINGUX_EXPECT_SOFTWARE_RENDERING="$mode" \
        BINGUX_SOFTWARE_RENDER_TEST_RESULT="$result" \
        XDG_CONFIG_HOME="$test_dir/config" \
        QT_QPA_PLATFORM=offscreen timeout 20s dbus-run-session -- \
        "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" --no-color >"$test_dir/run-$mode.log" 2>&1 || {
        cat "$test_dir/run-$mode.log"
        exit 1
    }
    grep -qx 'FAILURES 0' "$result" || {
        cat "$test_dir/run-$mode.log"
        cat "$result" 2>/dev/null || true
        exit 1
    }
done
BINGUX_SOFTWARE_RENDERING=0 BINGUX_REDUCED_MOTION=1 \
    BINGUX_EXPECT_SOFTWARE_RENDERING=0 \
    BINGUX_SOFTWARE_RENDER_TEST_RESULT="$test_dir/result-reduced.txt" \
    XDG_CONFIG_HOME="$test_dir/config" \
    QT_QPA_PLATFORM=offscreen timeout 20s dbus-run-session -- \
    "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" --no-color >"$test_dir/run-reduced.log" 2>&1 || {
    cat "$test_dir/run-reduced.log"
    exit 1
}
grep -qx 'FAILURES 0' "$test_dir/result-reduced.txt"
echo 'PASS: renderer override, reduced-motion override, and flattened surface palette'
