pragma Singleton
import QtQuick
import Quickshell

// Interpreter and helper paths shared by shell processes. install-shell.py
// exports BINGUX_PYTHON, BINGUX_CONFIG_BIN and BINGUX_SETTINGS_BIN.
QtObject {
    readonly property string python: Quickshell.env("BINGUX_PYTHON") || "python3"
    readonly property string applicationSpawn: decodeURIComponent(Qt.resolvedUrl("application_spawn.py").toString().replace(/^file:\/\//, ""))
    readonly property string settings: Quickshell.env("BINGUX_SETTINGS_BIN") || "bingux-settings"
    readonly property string config: Quickshell.env("BINGUX_CONFIG_BIN") || "bingux-config"
}
