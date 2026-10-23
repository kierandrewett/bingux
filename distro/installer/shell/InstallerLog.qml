import QtQuick

// The installer log: one line per entry in InstallerState.logModel, newest
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
        spacing: 4
        model: InstallerState.logModel
        onCountChanged: positionViewAtEnd()

        delegate: Text {
            required property string line
            width: list.width
            text: line
            wrapMode: Text.WrapAnywhere
            color: Theme.text
            font.family: "monospace"
            font.pixelSize: Math.round(18 * InstallerState.uiScale)
            lineHeight: 1.25
        }
    }
}
