import QtQuick

// A horizontal slider for a value between from and to. Drag or click the track;
// moved(value) is emitted with each value the user picks. The caller stores it
// and passes it back as value.
Item {
    id: root

    property real from: 0
    property real to: 1
    property real value: from
    property string caption: ""
    property string valueText: ""
    readonly property real fraction: to > from ? Math.max(0, Math.min(1, (value - from) / (to - from))) : 0

    signal moved(real value)

    width: parent ? parent.width : 300
    implicitHeight: caption === "" ? track.height + 26 : captionRow.implicitHeight + track.height + 26

    function pick(x) {
        const f = Math.max(0, Math.min(1, x / track.width));
        root.moved(root.from + f * (root.to - root.from));
    }

    Row {
        id: captionRow
        visible: root.caption !== ""
        width: parent.width
        Text {
            width: parent.width - valueLabel.implicitWidth
            text: root.caption
            color: Theme.muted
            elide: Text.ElideRight
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(15 * InstallerState.uiScale)
        }
        Text {
            id: valueLabel
            text: root.valueText
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(15 * InstallerState.uiScale)
        }
    }

    Item {
        id: track
        anchors.top: captionRow.bottom
        anchors.topMargin: 14
        anchors.left: parent.left
        anchors.right: parent.right
        height: 6

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Theme.elevated
            border.width: 1
            border.color: Qt.rgba(1, 1, 1, 0.12)
        }
        Rectangle {
            width: Math.max(height, track.width * root.fraction)
            height: parent.height
            radius: height / 2
            color: root.enabled ? Theme.accent : Theme.muted
        }
        Rectangle {
            x: track.width * root.fraction - width / 2
            y: -6
            width: 18
            height: 18
            radius: width / 2
            color: root.enabled ? Theme.accent : Theme.muted
        }
        MouseArea {
            anchors.fill: parent
            anchors.topMargin: -12
            anchors.bottomMargin: -12
            enabled: root.enabled
            cursorShape: Qt.PointingHandCursor
            onPressed: mouse => root.pick(mouse.x)
            onPositionChanged: mouse => {
                if (pressed)
                    root.pick(mouse.x);
            }
        }
    }
}
