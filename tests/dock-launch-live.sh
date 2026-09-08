#!/usr/bin/env bash
# Opens and closes a new Nautilus window to check real launch completion.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
helper="$(mktemp)"
trap 'rm -f "$helper"' EXIT
cat > "$helper" <<'SH'
#!/bin/sh
exec python3 "$BINGUX_LAUNCH_TEST_SOURCE" "$@"
SH
chmod +x "$helper"
export BINGUX_LAUNCH_TEST_SOURCE="$repo_dir/shell/bingux/launch-application.py"
export BINGUX_APP_LAUNCHER_HELPER="$helper" BINGUX_NOTES_NATIVE=1
exec_runner="$repo_dir/tests/sidebar-notes.sh"
"$exec_runner" dock-launch-live
