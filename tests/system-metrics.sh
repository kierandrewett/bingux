#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
components=(DataTable TableRowSurface ServicesPanel FilterField ProcessTable ProcessorDetails ActionButton HardwareDetails RollingNumber SystemMetrics SystemMetricsPopup SystemPerformance MetricGraph Pill BarControlSurface ShellPopup ControlRow ControlSwitch ControlCentreButtonSurface SymbolicIcon IconButton MarqueeText ShellTooltip TooltipBubble OsIconImage SegmentedControl)
for component in "${components[@]}"; do
    cp "$repo_dir/shell/bingux/$component.qml" "$test_dir/"
    printf '%s 1.0 %s.qml\n' "$component" "$component" >> "$test_dir/qmldir"
done
cp "$repo_dir"/shell/bingux/{Theme,OsIcons}.qml "$repo_dir/shell/bingux/MetricsHistory.js" "$test_dir/"
printf '%s\n' 'singleton Theme 1.0 Theme.qml' 'singleton OsIcons 1.0 OsIcons.qml' >> "$test_dir/qmldir"
cp "$repo_dir/shell/bingux/process-action.py" "$repo_dir/shell/bingux/render-os-icons.py" "$test_dir/"
cp "$repo_dir/tests/${1:-system-metrics}.qml" "$test_dir/shell.qml"
export BINGUX_METRICS_RESULTS="$test_dir/results"
if ! timeout 35s quickshell -p "$test_dir" --no-color > "$test_dir/runtime.log" 2>&1; then
    cat "$test_dir/runtime.log"
    exit 1
fi
cat "$test_dir/results"
grep -q '^PASS:' "$test_dir/results"
if rg 'ERROR|FATAL|WARN scene' "$test_dir/runtime.log" | grep -v 'Read of .*results failed'; then exit 1; fi
