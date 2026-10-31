# Adwaita names replaced by Bingux icons: the controls family

The shell asks the Adwaita theme for these names today. Each now has a Bingux source in
`scripts/icons/src` (generated into `shell/bingux/icons` and listed in
`shell/bingux/BinguxIconIndex.js`), or its callers were moved to a Bingux icon that already
existed. Names marked "new" have no Adwaita file at all, so the shell's name check
(`scripts/generate-adwaita-icon-index.py`) has to accept names in `BinguxIconIndex.js`.

| Adwaita name | Now | Callers (shell) |
| ------------ | --- | --------------- |
| `go-down` | callers use `pan-down` (the Adwaita name was only a fallback) | `NotificationStack.qml` |
| `process-stop` | new Bingux source `process-stop` (`StopCircle` fill) | `HardwareDetails.qml` Force quit, `ControlCentreDetails.qml` Stop searching |
| `view-pin` | new Bingux source `view-pin` (a drawn thumbtack) | `Dock.qml` pin badge |
| `system-run` | new Bingux source `system-run` (`PlayCircle` fill); the process table's service rows use `preferences-system` instead | `SearchResult.qml` (action results), `ProcessTable.qml` |
| `utilities-terminal` | new Bingux source (`Terminal` bold) | `DesktopLayout.js`, `TerminalSidebar.qml` |
| `accessories-text-editor` | new Bingux source (`Notepad` fill) | `DesktopLayout.js`, `TerminalSidebar.qml` |
| `action-unavailable` | new Bingux source (`Prohibit` bold) | `OsdCard.qml` |
| `dialog-question` | callers use `help-browser` (circled `?`, already Bingux) | `SearchResult.qml` and `searchd`'s quick chat result (chat results) |
| `document-open-recent` | new Bingux source (`ClockCounterClockwise` bold) | `EmojiPicker.qml` |
| `document-save` | new Bingux source (`FloppyDisk` bold) | `RecordingIndicator.qml`, `NotificationStack.qml` (save action) |
| `document-save-as` | callers use `document-save` | `NotificationStack.qml` |

Also changed in this family, so the shell does not name them any more:

- `system-lock-screen`, `security-high`, `security-low` are removed: the shell uses the padlock,
  `network-wireless-encrypted`, for Lock (`ControlCentreOverview.qml`, `ControlLayout.js`) and for
  "Block incoming connections" (`TailscalePage.qml`).
- `document-edit` stays for editing a document only. The Control Centre's "Customise controls..." is
  now `pencil` (new), the desktop "Label" widget is `font-x-generic`.
- `x-office-spreadsheet` is `table` (new) in `NotesCommands.js`; `x-office-document` is gone
  (`SearchResult.qml` uses `application-pdf`).
- `audio-card` stays only because BlueZ names Bluetooth devices with it (`ControlCentreModel.js`);
  the Settings Sound page no longer uses it.
