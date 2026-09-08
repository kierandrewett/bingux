#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
for component in DockTooltip TooltipBubble Theme; do
    cp "$repo_dir/shell/bingux/$component.qml" "$test_dir/"
done
printf '%s\n' 'DockTooltip 1.0 DockTooltip.qml' 'TooltipBubble 1.0 TooltipBubble.qml' 'singleton Theme 1.0 Theme.qml' > "$test_dir/qmldir"
cp "$repo_dir/tests/dock-tooltip-resize.qml" "$test_dir/shell.qml"
# Use the actual Wayland layer window: an offscreen Item misses configure latency.
if ! timeout 25s "${BINGUX_QUICKSHELL:-quickshell}" -p "$test_dir" --no-color > "$test_dir/log" 2>&1; then
    cat "$test_dir/log"
    exit 1
fi
cat "$test_dir/log"
grep -q 'PASS: 80 dock tooltip switches' "$test_dir/log"
if grep -E 'ERROR|FATAL|Binding loop|FAIL:' "$test_dir/log"; then exit 1; fi
