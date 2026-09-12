#!/usr/bin/env bash
set -euo pipefail
export PATH="$PATH"
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
for component in Theme ShellPopup PanelOutline; do
    cp "$repo_dir/shell/bingux/$component.qml" "$test_dir/"
done
printf '%s\n' 'singleton Theme 1.0 Theme.qml' 'ShellPopup 1.0 ShellPopup.qml' 'PanelOutline 1.0 PanelOutline.qml' >"$test_dir/qmldir"
sed -i '/id: root/a\    property alias testDismissContentItem: dismissWindow.contentItem' "$test_dir/ShellPopup.qml"
cp "$repo_dir/tests/popup-motion.qml" "$test_dir/shell.qml"
export BINGUX_POPUP_TEST_RESULTS="$test_dir/results.txt"
timeout 20s dbus-run-session -- "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" --no-color >"$test_dir/runtime.log" 2>&1 || {
    cat "$test_dir/runtime.log"
    exit 1
}
if [[ ! -f "$test_dir/results.txt" ]]; then
    cat "$test_dir/runtime.log"
    exit 1
fi
cat "$test_dir/results.txt"
grep -q '^FAILURES 0$' "$test_dir/results.txt"
