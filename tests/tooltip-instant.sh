#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
for component in Theme BarTooltip ShellTooltip DockTooltip TooltipBubble; do
    cp "$repo_dir/shell/bingux/$component.qml" "$test_dir/"
done
printf '%s\n' 'singleton Theme 1.0 Theme.qml' > "$test_dir/qmldir"
for component in BarTooltip ShellTooltip DockTooltip TooltipBubble; do
    printf '%s 1.0 %s.qml\n' "$component" "$component" >> "$test_dir/qmldir"
done
sed -i '/id: root/a\    property alias testPopup: popup' "$test_dir/BarTooltip.qml"
cp "$repo_dir/tests/tooltip-instant.qml" "$test_dir/shell.qml"
if ! timeout 15s "${BINGUX_QUICKSHELL:-quickshell}" -p "$test_dir" --no-color > "$test_dir/log" 2>&1; then
    cat "$test_dir/log"
    exit 1
fi
cat "$test_dir/log"
grep -q 'PASS: initial tooltip delay and fade, instant cross-surface switching, idle reset' "$test_dir/log"
if grep -E 'FAIL!|ERROR|FATAL|Binding loop' "$test_dir/log"; then exit 1; fi
