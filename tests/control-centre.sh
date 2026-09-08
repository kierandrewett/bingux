#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/{*.qml,*.js,*.py,*.json,qmldir} "$test_dir/"
cp -r "$repo_dir/shell/bingux/icons" "$test_dir/"
cp "${BINGUX_CONTROL_TEST_QML:-$repo_dir/tests/control-centre.qml}" "$test_dir/shell.qml"
printf '%s' '<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80"><circle cx="40" cy="40" r="40" fill="#7caaf0"/></svg>' > "$test_dir/avatar.svg"
export XDG_CONFIG_HOME="$test_dir/config"
export XDG_STATE_HOME="$test_dir/state"
export BINGUX_CONTROL_TEST_RESULTS="$test_dir/results.txt"
export BINGUX_REDUCED_MOTION="${BINGUX_REDUCED_MOTION:-0}"
export QS_TEST_BIN="${BINGUX_QUICKSHELL:-quickshell}"
export BINGUX_CONTROL_TEST_NO_QUIT=1
python3 - "$test_dir" "$repo_dir" <<'RUN'
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(sys.argv[2]) / 'tests'))
from private_shell import run_reported_shell
report, output = run_reported_shell(Path(sys.argv[1]), dict(os.environ), 'BINGUX_CONTROL_TEST_RESULTS', complete=lambda text: 'FAILURES ' in text)
print(report)
if not report.endswith('FAILURES 0') or any(error in output for error in ('TypeError', 'ReferenceError', 'has crashed')):
    print(output)
    raise SystemExit(1)
RUN
