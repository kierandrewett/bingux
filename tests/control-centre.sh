#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
cp "$repo_dir/shell/bingux/NotificationHistory.js" "$test_dir/"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/{SeekSlider,SystemIndicators,ControlCentreExtras,ControlCentreServices,NotificationHistoryPopup,SegmentedControl,AudioLevel,Theme,ShellPopup,ControlCentre,ControlCentreMedia,ControlCentreDetails,ControlCentreButtonSurface,ControlRow,ControlSwitch,IconButton,MediaControls,PlayPauseGlyph,AlbumArtwork,MarqueeText,ActionButton,ShellTooltip,TooltipBubble,AlbumArtCache,NotificationState,NotificationStack,NotificationSurface,SymbolicIcon}.qml "$test_dir/"
printf '%s\n' 'SeekSlider 1.0 SeekSlider.qml' 'SystemIndicators 1.0 SystemIndicators.qml' 'ControlCentreExtras 1.0 ControlCentreExtras.qml' 'singleton ControlCentreServices 1.0 ControlCentreServices.qml' 'NotificationHistoryPopup 1.0 NotificationHistoryPopup.qml' 'SegmentedControl 1.0 SegmentedControl.qml' 'AudioLevel 1.0 AudioLevel.qml' 'singleton Theme 1.0 Theme.qml' 'ShellPopup 1.0 ShellPopup.qml' 'ControlCentre 1.0 ControlCentre.qml' 'ControlCentreMedia 1.0 ControlCentreMedia.qml' 'ControlCentreDetails 1.0 ControlCentreDetails.qml' 'ControlCentreButtonSurface 1.0 ControlCentreButtonSurface.qml' 'ControlRow 1.0 ControlRow.qml' 'ControlSwitch 1.0 ControlSwitch.qml' 'IconButton 1.0 IconButton.qml' 'NotificationState 1.0 NotificationState.qml' 'NotificationSurface 1.0 NotificationSurface.qml' 'NotificationStack 1.0 NotificationStack.qml' 'SymbolicIcon 1.0 SymbolicIcon.qml' > "$test_dir/qmldir"
cp "${BINGUX_CONTROL_TEST_QML:-$repo_dir/tests/control-centre.qml}" "$test_dir/shell.qml"
printf '%s' '<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80"><circle cx="40" cy="40" r="40" fill="#7caaf0"/></svg>' > "$test_dir/avatar.svg"
cp "$repo_dir"/shell/bingux/{OsIconImage.qml,OsIcons.qml,render-os-icons.py} "$test_dir/"
printf '%s\n' 'OsIconImage 1.0 OsIconImage.qml' 'singleton OsIcons 1.0 OsIcons.qml' >> "$test_dir/qmldir"
cp "$repo_dir/shell/bingux/MediaMatch.js" "$repo_dir/shell/bingux/ControlCentreModel.js" "$test_dir/"
cp "$repo_dir/shell/bingux/StatusIndicator.qml" "$test_dir/"
cp "$repo_dir"/shell/bingux/{DataTable,TableRowSurface,TailscalePage,TrayMenu,MenuNavigator}.qml "$test_dir/"
printf '%s\n' 'DataTable 1.0 DataTable.qml' 'TableRowSurface 1.0 TableRowSurface.qml' >> "$test_dir/qmldir"
cp "$repo_dir/shell/bingux/FilterField.qml" "$test_dir/"
printf '%s\n' 'FilterField 1.0 FilterField.qml' >> "$test_dir/qmldir"
cp "$repo_dir"/shell/bingux/{EmojiPicker.qml,EmojiSearch.js,PopupPlacement.js,caret-anchor.py,emoji-data.json,ShortcutSession.qml} "$test_dir/"
printf '%s\n' 'EmojiPicker 1.0 EmojiPicker.qml' 'ShortcutSession 1.0 ShortcutSession.qml' >> "$test_dir/qmldir"
printf '%s\n' 'TailscalePage 1.0 TailscalePage.qml' >> "$test_dir/qmldir"
printf '%s\n' 'StatusIndicator 1.0 StatusIndicator.qml' >> "$test_dir/qmldir"
printf '%s\n' 'MediaControls 1.0 MediaControls.qml' 'PlayPauseGlyph 1.0 PlayPauseGlyph.qml' 'AlbumArtwork 1.0 AlbumArtwork.qml' 'MarqueeText 1.0 MarqueeText.qml' 'ActionButton 1.0 ActionButton.qml' 'ShellTooltip 1.0 ShellTooltip.qml' 'TooltipBubble 1.0 TooltipBubble.qml' 'singleton AlbumArtCache 1.0 AlbumArtCache.qml' >> "$test_dir/qmldir"
export BINGUX_CONTROL_TEST_RESULTS="$test_dir/results.txt"
export BINGUX_REDUCED_MOTION="${BINGUX_REDUCED_MOTION:-0}"
timeout 25s dbus-run-session -- "${BINGUX_QUICKSHELL:-quickshell}" -p "$test_dir" --no-color > "$test_dir/runtime.log" 2>&1 || { if [[ -f "$test_dir/results.txt" ]]; then cat "$test_dir/results.txt"; fi; cat "$test_dir/runtime.log"; exit 1; }
if [[ ! -f "$test_dir/results.txt" ]]; then cat "$test_dir/runtime.log"; exit 1; fi
cat "$test_dir/results.txt"
if ! grep -q '^FAILURES 0$' "$test_dir/results.txt"; then cat "$test_dir/runtime.log"; exit 1; fi
