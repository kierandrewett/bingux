#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$repo_dir"/shell/bingux/*.js "$repo_dir"/shell/bingux/qmldir "$test_dir/"
cp -r "$repo_dir/shell/bingux/icons" "$test_dir/icons"
python3 - "$test_dir" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
p = root / 'TerminalSidebar.qml'
p.write_text(p.read_text().replace('"file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar.ini"', '"' + (root / 'sidebar.ini').as_uri() + '"'))
p = root / 'SidebarNotes.qml'
p.write_text(p.read_text().replace('"file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar-notes.ini"', '"' + (root / 'notes.ini').as_uri() + '"'))
p = root / 'SidebarMonitor.qml'
p.write_text(p.read_text().replace('"file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar-system.ini"', '"' + (root / 'system.ini').as_uri() + '"'))
PY
# Layout tests run offscreen; native popup behaviour is covered by system-metrics.sh.
if [[ "${1:-}" == sidebar-system ]]; then
    cat >"$test_dir/ShellPopup.qml" <<'QML'
import QtQuick
Item {
    default property alias contents: body.data
    visible: false
    property Item hostItem: null
    property Item contentItem: hostItem
    property int popupWidth: 216
    property int contentPadding: 6
    property real preferredX: 0
    property real preferredY: 0
    Item { id: body; width: parent.popupWidth }
}
QML
fi
cp "$repo_dir/tests/${1:-sidebar-notes}.qml" "$test_dir/shell.qml"
export BINGUX_NOTES_TEST_RESULTS="$test_dir/results.txt"
if [[ "${BINGUX_NOTES_NATIVE:-0}" == 1 || "${1:-sidebar-notes}" == "notes-context" || "${1:-sidebar-notes}" == "sidebar-popout" || "${1:-sidebar-notes}" == "sidebar-calendar" ]]; then
    timeout 20s "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" >"$test_dir/runtime.log" 2>&1 || {
        cat "$test_dir/runtime.log"
        exit 1
    }
else
    QT_QPA_PLATFORM=offscreen timeout 20s dbus-run-session -- "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" >"$test_dir/runtime.log" 2>&1 || {
        cat "$test_dir/runtime.log"
        exit 1
    }
fi
if [[ ! -f "$test_dir/results.txt" ]]; then
    cat "$test_dir/runtime.log"
    exit 1
fi
cat "$test_dir/results.txt"
grep -q '^FAILURES 0$' "$test_dir/results.txt" || {
    cat "$test_dir/runtime.log"
    exit 1
}
if grep -E '(TypeError|ReferenceError):|Failed to load configuration' "$test_dir/runtime.log"; then
    exit 1
fi
