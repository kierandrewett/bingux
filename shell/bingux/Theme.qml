pragma Singleton
import QtQuick

QtObject {
    readonly property color background: "#242424"
    readonly property color surface: "#303030"
    readonly property color elevated: "#383838"
    readonly property color hover: "#454545"
    readonly property color pressed: "#505050"
    readonly property color border: "#ffffff"
    readonly property color outline: "#505050"
    readonly property color text: "#fafafa"
    readonly property color muted: "#b0b0b0"
    readonly property color accent: "#99c1f1"
    readonly property color selection: "#31445d"
    readonly property color success: "#8ff0a4"
    readonly property int spaceSmall: 4
    readonly property int gap: 8
    readonly property int padding: 12
    readonly property int paddingLarge: 20
    readonly property int radius: 12
    readonly property int cardRadius: 20
    readonly property int barHeight: 48
    readonly property int controlHeight: 34
    readonly property int iconSize: 18
    readonly property int fontSize: 14
    readonly property string fontFamily: "Adwaita Sans"
    readonly property int dockHeight: 88
    readonly property int dockItemSize: 76
    readonly property int dockIconSize: 56
    readonly property int motion: 120
}
