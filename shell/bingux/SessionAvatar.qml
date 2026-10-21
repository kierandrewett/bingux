import QtQuick
import Quickshell.Widgets

// Round account picture with an initial as the fallback, for the lock screen
// and the greeter. Drawn over wallpaper, so the fallback is translucent white.
Item {
    id: root
    property url source: ""
    property string name: ""
    property int size: 64
    property bool dimmed: false
    implicitWidth: size
    implicitHeight: size
    opacity: dimmed ? 0.6 : 1
    Behavior on opacity {
        NumberAnimation {
            duration: Theme.motion
        }
    }
    // Apple-style monogram: first letters of the first and last name on a
    // soft vertical gradient. The hue comes from the name so accounts on the
    // greeter stay distinct; an empty name falls back to the neutral grey.
    readonly property string initials: {
        const parts = root.name.trim().split(/\s+/).filter(part => part !== "");
        if (parts.length === 0)
            return "";
        const first = parts[0].charAt(0);
        const last = parts.length > 1 ? parts[parts.length - 1].charAt(0) : "";
        return (first + last).toUpperCase();
    }
    readonly property real hue: {
        let hash = 0;
        for (let index = 0; index < root.name.length; index++)
            hash = (hash * 31 + root.name.charCodeAt(index)) % 360;
        return hash / 360;
    }
    readonly property color monogramTop: root.name === "" ? "#b8bec6" : Qt.hsla(hue, 0.42, 0.72, 1)
    readonly property color monogramBottom: root.name === "" ? "#8d949c" : Qt.hsla(hue, 0.48, 0.52, 1)
    ClippingRectangle {
        anchors.fill: parent
        radius: root.size / 2
        color: "transparent"
        Rectangle {
            anchors.fill: parent
            visible: image.status !== Image.Ready
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: root.monogramTop
                }
                GradientStop {
                    position: 1
                    color: root.monogramBottom
                }
            }
        }
        Image {
            id: image
            anchors.fill: parent
            source: root.source
            fillMode: Image.PreserveAspectCrop
            sourceSize.width: root.size * 2
            sourceSize.height: root.size * 2
            asynchronous: true
        }
        Text {
            anchors.centerIn: parent
            visible: image.status !== Image.Ready
            text: root.initials
            color: "#ffffff"
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(root.size * (root.initials.length > 1 ? 0.36 : 0.42))
            font.weight: Font.Medium
            font.letterSpacing: root.initials.length > 1 ? -1 : 0
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: root.size / 2
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.18)
    }
}
