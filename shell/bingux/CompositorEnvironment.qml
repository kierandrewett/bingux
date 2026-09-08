pragma Singleton
import QtQuick
import Quickshell

QtObject {
    // Portable mode deliberately avoids the host compositor's private services.
    readonly property bool portable: Quickshell.env("BINGUX_COMPOSITOR") === "portable"
    readonly property bool gnoblin: !portable
}
