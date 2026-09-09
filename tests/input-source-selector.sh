#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d /tmp/bingux-keyboard-test.XXXXXX)"
trap 'rm -rf "$fixture"' EXIT
cp "$repo_dir"/shell/bingux/{AnimatedCount,InputSourceSelector,Theme,ShellPopup,PanelOutline,MenuNavigator,ShortcutSession,BarControlSurface,BarTooltip,ShellTooltip,TooltipBubble,SymbolicIcon,WidgetFace}.qml "$fixture/"
cat > "$fixture/DesktopEditing.qml" <<'EOF'
pragma Singleton
import QtQuick
QtObject {
    readonly property bool active: false
    function observeGeometry(item) {}
}
EOF
cat > "$fixture/CompositorEnvironment.qml" <<'EOF'
pragma Singleton
import QtQuick
QtObject { readonly property bool gnoblin: false }
EOF
cat > "$fixture/PopupTransitions.qml" <<'EOF'
pragma Singleton
import QtQuick
QtObject {
    function refresh() {}
    function matches(duration, easing) { return false; }
}
EOF
printf '%s\n' 'singleton Theme 1.0 Theme.qml' 'singleton DesktopEditing 1.0 DesktopEditing.qml' 'singleton CompositorEnvironment 1.0 CompositorEnvironment.qml' 'singleton PopupTransitions 1.0 PopupTransitions.qml' > "$fixture/qmldir"
for component in AnimatedCount InputSourceSelector ShellPopup PanelOutline MenuNavigator ShortcutSession BarControlSurface BarTooltip ShellTooltip TooltipBubble SymbolicIcon WidgetFace; do
    printf '%s 1.0 %s.qml\n' "$component" "$component" >> "$fixture/qmldir"
done
cp "$repo_dir/tests/input-source-selector.qml" "$fixture/shell.qml"
printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" > "$BINGUX_KEYBOARD_TEST_SELECTION"\nsleep 0.08\n' > "$fixture/source-cli"
chmod +x "$fixture/source-cli"
export BINGUX_KEYBOARD_TEST_CLI="$fixture/source-cli"
export BINGUX_KEYBOARD_TEST_RESULTS="$fixture/result.txt"
export BINGUX_KEYBOARD_TEST_SELECTION="$fixture/selection.txt"
timeout 15s "${BINGUX_QUICKSHELL:-quickshell}" -p "$fixture" --no-color > "$fixture/runtime.log" 2>&1 || true
cat "$fixture/runtime.log"
cat "$fixture/result.txt"
grep -q '^PASS:' "$fixture/result.txt"
