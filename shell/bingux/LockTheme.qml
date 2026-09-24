pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Lock appearance is deliberately independent of BinguxPreferences. The
// client reads only this small, local JSON file and never writes policy.
Scope {
    id: root

    readonly property string path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/lock-theme.json"
    property color background: "#111318"
    property color surface: "#261f232b"
    property color text: "#ffffff"
    property color mutedText: "#c7cbd5"
    property color accent: "#3584e4"
    property string backgroundImage: ""
    property bool useTwelveHourClock: false

    function isColour(value) {
        return typeof value === "string" && /^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(value);
    }

    function isLocalAbsolutePath(value) {
        return typeof value === "string" && value.startsWith("/") && !value.includes("\u0000");
    }

    function apply(data) {
        if (!data || typeof data !== "object" || Array.isArray(data))
            return;
        if (isColour(data.background))
            background = data.background;
        if (isColour(data.surface))
            surface = data.surface;
        if (isColour(data.text))
            text = data.text;
        if (isColour(data.mutedText))
            mutedText = data.mutedText;
        if (isColour(data.accent))
            accent = data.accent;
        if (isLocalAbsolutePath(data.backgroundImage))
            backgroundImage = data.backgroundImage;
        if (typeof data.useTwelveHourClock === "boolean")
            useTwelveHourClock = data.useTwelveHourClock;
    }

    FileView {
        path: root.path
        watchChanges: true
        printErrors: false
        onLoaded: {
            try {
                root.apply(JSON.parse(text()));
            } catch (_) {
                console.warn("bingux-lock: ignoring invalid lock-theme.json");
            }
        }
        onFileChanged: reload()
    }
}
