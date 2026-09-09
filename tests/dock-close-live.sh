#!/usr/bin/env bash
set -euo pipefail
export PATH="$PATH"
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$repo_dir"/shell/bingux/*.js "$repo_dir"/shell/bingux/qmldir "$fixture/"
cp "$repo_dir/tests/dock-close-live.qml" "$fixture/shell.qml"
python3 - "$fixture/Dock.qml" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text().replace('id: root', 'id: root\n    property alias testItems: dockItems', 1)
s = s.replace('id: dockButton', 'id: dockButton\n                    property alias testMenuEntries: menuColumn', 1)
p.write_text(s)
PY
cat > "$fixture/launcher" <<EOF2
#!/bin/sh
exec python3 "$repo_dir/shell/bingux/launch-application.py" "\$@"
EOF2
chmod +x "$fixture/launcher"
export BINGUX_APP_LAUNCHER_HELPER="$fixture/launcher" BINGUX_NOTES_TEST_RESULTS="$fixture/results.txt"
timeout 25s "${QUICKSHELL_BIN:-quickshell}" -p "$fixture" > "$fixture/runtime.log" 2>&1 || { cat "$fixture/runtime.log"; exit 1; }
if [[ ! -f "$fixture/results.txt" ]]; then cat "$fixture/runtime.log"; exit 1; fi
cat "$fixture/results.txt"
grep -q '^FAILURES 0$' "$fixture/results.txt"
