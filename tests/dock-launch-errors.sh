#!/usr/bin/env bash
set -euo pipefail
export PATH="$PATH"
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$repo_dir"/shell/bingux/*.js "$repo_dir"/shell/bingux/qmldir "$fixture/"
cp "$repo_dir"/shell/bingux/shell_notify.py "$repo_dir"/shell/bingux/launch-application.py "$fixture/"
cp -r "$repo_dir/shell/bingux/icons" "$fixture/"
cp "$repo_dir/tests/dock-launch-errors.qml" "$fixture/shell.qml"
python3 - "$fixture/Dock.qml" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
# Supply deterministic groups while retaining production launch state and UI.
s = p.read_text().replace('function refreshAppGroupsNow() {', 'function refreshAppGroupsNow() { return;', 1)
p.write_text(s)
PY
cat >"$fixture/launcher" <<EOF2
#!/bin/sh
exec python3 "$repo_dir/shell/bingux/launch-application.py" "\$@"
EOF2
chmod +x "$fixture/launcher"
export BINGUX_APP_LAUNCHER_HELPER="$fixture/launcher" BINGUX_NOTES_TEST_RESULTS="$fixture/results.txt"
export BINGUX_CAPTURE_HELPER="$repo_dir/tests/customise-capture-helper.py"
export BINGUX_CAPTURE_SETTINGS_PATH="file://$fixture/capture.ini"
export XDG_STATE_HOME="$fixture/state"
timeout 35s dbus-run-session -- "${QUICKSHELL_BIN:-quickshell}" -p "$fixture" >"$fixture/runtime.log" 2>&1 || {
    cat "$fixture/runtime.log"
    exit 1
}
if [[ ! -f "$fixture/results.txt" ]]; then
    cat "$fixture/runtime.log"
    exit 1
fi
cat "$fixture/results.txt"
grep -q '^FAILURES 0$' "$fixture/results.txt"
