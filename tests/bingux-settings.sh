#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/{BinguxSettings,BinguxPreferences,Theme,ActionButton,SymbolicIcon,ControlSwitch,ControlRow,ControlCentreButtonSurface,MarqueeText,IconButton,ShellTooltip,TooltipBubble,FilterField,SettingsGroup,SettingsHeading,SettingsField,SettingsChoice,SearchSettings,DesktopCustomise,SeekSlider}.qml "$test_dir/"
cp "$repo_dir/shell/bingux/DesktopLayout.js" "$test_dir/"
cp "$repo_dir/shell/bingux/settings-backend.py" "$test_dir/"
cp "$repo_dir/tests/bingux-settings.qml" "$test_dir/shell.qml"
printf '%s\n' 'singleton Theme 1.0 Theme.qml' 'singleton BinguxPreferences 1.0 BinguxPreferences.qml' 'BinguxSettings 1.0 BinguxSettings.qml' 'ActionButton 1.0 ActionButton.qml' 'SymbolicIcon 1.0 SymbolicIcon.qml' 'ControlSwitch 1.0 ControlSwitch.qml' 'ControlRow 1.0 ControlRow.qml' 'ControlCentreButtonSurface 1.0 ControlCentreButtonSurface.qml' 'MarqueeText 1.0 MarqueeText.qml' 'IconButton 1.0 IconButton.qml' 'ShellTooltip 1.0 ShellTooltip.qml' 'TooltipBubble 1.0 TooltipBubble.qml' > "$test_dir/qmldir"
for component in FilterField SettingsGroup SettingsHeading SettingsField SettingsChoice SearchSettings DesktopCustomise SeekSlider; do
    printf '%s 1.0 %s.qml\n' "$component" "$component" >> "$test_dir/qmldir"
done
# Test the actual content in a native window that cannot interrupt the user's typing.
sed -i 's/flags: Qt.Window | Qt.FramelessWindowHint/flags: Qt.Window | Qt.FramelessWindowHint | Qt.WindowDoesNotAcceptFocus/' "$test_dir/BinguxSettings.qml"
mkdir -p "$test_dir/bin" "$test_dir/config/bingux"
printf '#!/bin/sh\nexit 0\n' > "$test_dir/bin/systemctl"
chmod +x "$test_dir/bin/systemctl"
printf '%s' '{}' > "$test_dir/config/bingux/settings.json"
BINGUX_SETTINGS_REPORT="$test_dir/report" NO_AT_BRIDGE=1 PATH="$test_dir/bin:$PATH" XDG_CONFIG_HOME="$test_dir/config" timeout 25s dbus-run-session -- "${BINGUX_QUICKSHELL:-quickshell}" -p "$test_dir" --no-color > "$test_dir/log" 2>&1 || { cat "$test_dir/log"; exit 1; }
cat "$test_dir/report"
grep 'BINGUX_SETTINGS_PASS' "$test_dir/report" || { cat "$test_dir/log"; exit 1; }
if grep -E 'ReferenceError|TypeError|Binding loop|FAIL!' "$test_dir/log"; then cat "$test_dir/log"; exit 1; fi
