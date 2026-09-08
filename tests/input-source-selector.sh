#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d /tmp/bingux-keyboard-test.XXXXXX)"
trap 'rm -rf "$fixture"' EXIT
cp "$repo_dir"/shell/bingux/{AnimatedCount,InputSourceSelector,Theme,ShellPopup,PanelOutline,MenuNavigator,ShortcutSession,BarControlSurface,BarTooltip,ShellTooltip,TooltipBubble,SymbolicIcon}.qml "$fixture/"
printf '%s\n' 'singleton Theme 1.0 Theme.qml' > "$fixture/qmldir"
for component in AnimatedCount InputSourceSelector ShellPopup PanelOutline MenuNavigator ShortcutSession BarControlSurface BarTooltip ShellTooltip TooltipBubble SymbolicIcon; do
    printf '%s 1.0 %s.qml\n' "$component" "$component" >> "$fixture/qmldir"
done
cp "$repo_dir/tests/input-source-selector.qml" "$fixture/shell.qml"
printf '#!/usr/bin/env bash\nsleep 0.08\n' > "$fixture/source-cli"
chmod +x "$fixture/source-cli"
export BINGUX_KEYBOARD_TEST_CLI="$fixture/source-cli"
export BINGUX_KEYBOARD_TEST_RESULTS="$fixture/result.txt"
timeout 15s "${BINGUX_QUICKSHELL:-quickshell}" -p "$fixture" --no-color > "$fixture/runtime.log" 2>&1 || true
cat "$fixture/runtime.log"
cat "$fixture/result.txt"
grep -q '^PASS:' "$fixture/result.txt"
