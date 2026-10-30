pragma Singleton
import QtQuick
import Quickshell

// Interpreter and helper paths shared by shell processes. bingux-shell
// exports BINGUX_PYTHON, BINGUX_HELPER_DIR, BINGUX_CONFIG_BIN and
// BINGUX_SETTINGS_BIN; a checkout run without them falls back to the
// helpers beside this file.
QtObject {
    readonly property string python: Quickshell.env("BINGUX_PYTHON") || "python3"
    readonly property string helperDir: Quickshell.env("BINGUX_HELPER_DIR")
    readonly property string applicationSpawn: helper("application_spawn.py")
    readonly property string settings: Quickshell.env("BINGUX_SETTINGS_BIN") || "bingux-settings"
    readonly property string config: Quickshell.env("BINGUX_CONFIG_BIN") || "bingux-config"

    // The absolute path of a Python helper. Installed shells run the
    // helpers from libexec; the packed QML archive carries no Python.
    function helper(name) {
        if (helperDir !== "")
            return helperDir + "/" + name;
        return decodeURIComponent(Qt.resolvedUrl(name).toString().replace(/^file:\/\//, ""));
    }
}
