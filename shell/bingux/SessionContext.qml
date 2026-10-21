pragma Singleton
import QtQuick

// Which kind of session this Quickshell process serves. The desktop shell
// leaves it at "desktop"; LockShell sets "locked", GreeterShell sets
// "greeter" and a kiosk host sets "kiosk" before any widget loads. Widget
// tables and SessionServices read it to decide what may appear and what may
// act.
QtObject {
    id: root

    // desktop | locked | greeter | kiosk
    property string mode: "desktop"
    readonly property bool restricted: mode !== "desktop"
    readonly property bool locked: mode === "locked"
    readonly property bool greeter: mode === "greeter"
    readonly property bool kiosk: mode === "kiosk"

    function enter(value) {
        if (!["desktop", "locked", "greeter", "kiosk"].includes(value)) {
            console.warn("[session] unknown session context " + value + "; staying " + root.mode);
            return;
        }
        root.mode = value;
    }
}
