pragma Singleton
import QtQuick

// Which kind of session this Quickshell process serves. The desktop shell
// leaves it at "desktop"; LockShell sets "locked" and GreeterShell sets
// "greeter" before any widget loads. Widget tables and SessionServices read
// it to decide what may appear and what may act.
QtObject {
    id: root

    // desktop | locked | greeter
    property string mode: "desktop"
    readonly property bool restricted: mode !== "desktop"
    readonly property bool locked: mode === "locked"
    readonly property bool greeter: mode === "greeter"

    function enter(value) {
        if (!["desktop", "locked", "greeter"].includes(value)) {
            console.warn("[session] unknown session context " + value + "; staying " + root.mode);
            return;
        }
        root.mode = value;
    }
}
