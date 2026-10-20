#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$repo_dir"/shell/bingux/*.js "$repo_dir/shell/bingux/qmldir" "$test_dir/"
cp -R "$repo_dir/shell/bingux/preview-assets" "$test_dir/"
cp "$repo_dir/tests/search-app-menu.qml" "$test_dir/shell.qml"
sed -i 's@return runtimeDirectory ? runtimeDirectory + "/bingux/search-v1.sock" : "";@return "";@' "$test_dir/SearchSocket.qml"
export XDG_CONFIG_HOME="$test_dir/config" XDG_DATA_HOME="$test_dir/data"
mkdir -p "$XDG_CONFIG_HOME/bingux" "$XDG_CONFIG_HOME/gnoblin" "$XDG_DATA_HOME/applications"
export BINGUX_CONFIG_BIN="${BINGUX_CONFIG_BIN:-bingux-config}"
printf '%s\n' '[desktop.dock_apps]' 'pinned_apps = []' 'order = []' >"$XDG_CONFIG_HOME/bingux/bingux.toml"
printf '%s\n' '[Desktop Entry]' 'Type=Application' 'Name=Menu test app' 'Exec=true' >"$XDG_DATA_HOME/applications/bingux-menu-test.desktop"
for unpin in 0 1; do
    QT_LOGGING_RULES="quickshell.desktopentry.warning=false" BINGUX_MENU_UNPIN="$unpin" NO_AT_BRIDGE=1 timeout 20s "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" --no-color >"$test_dir/log" 2>&1 || {
        cat "$test_dir/log"
        exit 1
    }
    grep -q 'SEARCH_APP_MENU_PASS' "$test_dir/log" || {
        cat "$test_dir/log"
        exit 1
    }
    if grep -E 'TypeError|ReferenceError|Binding loop|FAIL!' "$test_dir/log"; then exit 1; fi
    python3 - "$BINGUX_CONFIG_BIN" "$unpin" <<'PY'
import json, subprocess, sys
reply = subprocess.run([sys.argv[1], 'get', 'desktop.dock_apps'], check=True, capture_output=True, text=True)
pins = json.loads(reply.stdout)['pinned_apps']
assert ('bingux-menu-test' in pins) == (sys.argv[2] == '0'), pins
print('PASS: search app menu and persisted ' + ('unpin' if sys.argv[2] == '1' else 'pin'))
PY
done
