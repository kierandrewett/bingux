import QtQuick

// The installer log: one line per entry from InstallerState.logLines, newest
// at the bottom. Shared by the progress and error pages.
Rectangle {
    id: root

    property int lineHeight: 160

    implicitHeight: lineHeight
    height: lineHeight
    width: parent ? parent.width : 300
    radius: Theme.radius
    color: Theme.elevated
    border.width: 1
    border.color: Qt.rgba(1, 1, 1, 0.12)
    clip: true

    ListView {
        id: list
        anchors.fill: parent
        anchors.margins: Theme.gap
        clip: true
        model: InstallerState.logLines
        onCountChanged: positionViewAtEnd()

        delegate: Text {
            required property string modelData
            width: list.width
            text: modelData
            wrapMode: Text.WrapAnywhere
            color: Theme.muted
            font.family: "monospace"
            font.pixelSize: Theme.fontSmall
        }
    }
}
