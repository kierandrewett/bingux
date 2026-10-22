pragma Singleton
import QtQuick
import Quickshell

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

    // True when the previous stage (the login greeter) already showed the
    // wallpaper and the bar. The greeter exports BINGUX_CONTINUING=1 to the
    // user session it launches, so this session's first frame matches the one
    // the greeter left on screen and nothing enters again.
    // Only the desktop shell continues the greeter's frame; a lock shell started
    // from that session inherits the variable but enters from its own first frame.
    readonly property bool continuing: mode === "desktop" && Quickshell.env("BINGUX_CONTINUING") === "1"

    function enter(value) {
        if (!["desktop", "locked", "greeter", "kiosk"].includes(value)) {
            console.warn("[session] unknown session context " + value + "; staying " + root.mode);
            return;
        }
        root.mode = value;
    }
}
