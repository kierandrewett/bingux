pragma Singleton
import QtQuick
import Quickshell

// The Rust helper binary. One executable serves every helper that has left
// Python: `bingux-helper <name> [arguments]`. bingux-shell exports
// BINGUX_HELPER_BIN; a checkout run without it finds bingux-helper on PATH.
QtObject {
    readonly property string binary: Quickshell.env("BINGUX_HELPER_BIN") || "bingux-helper"

    // The command line for a helper, ready for Process.command.
    function command(name, args) {
        return [binary, name].concat(args || []);
    }
}
