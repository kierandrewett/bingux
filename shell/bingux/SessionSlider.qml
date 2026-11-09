import QtQuick

// Horizontal slider on the lock screen's glass and on installer cards: a
// translucent track, a fill and a round handle. Drag or click the track, or
// use the arrow, Page, Home and End keys; moved(value) is emitted with each
// value the person picks. The caller stores it and passes it back as value,
// so the slider never moves on its own.
//
// from and to bound the value (0 to 1 by default). caption and valueText put
// a label above the track, the caption on the left and the current value on
// the right. fillColor colours the fill and handle: white on the lock
// screen's glass, the accent where the slider is a setting.
Item {
    id: root

    property real from: 0
    property real to: 1
    property real value: from
    // How far an arrow key moves the value; 0 means a twentieth of the range.
    property real stepSize: 0
    property string caption: ""
    property string valueText: ""
    property color fillColor: "#ffffff"
    // Scales the label type for a surface that scales its type (the installer).
    property real unit: 1
    readonly property real fraction: to > from ? Math.max(0, Math.min(1, (value - from) / (to - from))) : 0
    readonly property real step: stepSize > 0 ? stepSize : (to - from) / 20
    readonly property color activeFill: enabled ? fillColor : Theme.muted

    signal moved(real value)

    implicitHeight: (labels.visible ? labels.height + Math.round(10 * unit) : 0) + 20
    implicitWidth: 120
    activeFocusOnTab: true
    Accessible.role: Accessible.Slider
    Accessible.name: caption
    Accessible.description: valueText

    function valueAt(x) {
        const f = Math.max(0, Math.min(1, (x - handle.width / 2) / (width - handle.width)));
        return from + f * (to - from);
    }
    function nudge(amount) {
        moved(Math.max(from, Math.min(to, value + amount)));
    }

    Keys.onPressed: event => {
        if (!enabled)
            return;
        const big = Math.max(step, (to - from) / 10);
        if (event.key === Qt.Key_Left || event.key === Qt.Key_Down)
            nudge(-step);
        else if (event.key === Qt.Key_Right || event.key === Qt.Key_Up)
            nudge(step);
        else if (event.key === Qt.Key_PageDown)
            nudge(-big);
        else if (event.key === Qt.Key_PageUp)
            nudge(big);
        else if (event.key === Qt.Key_Home)
            moved(from);
        else if (event.key === Qt.Key_End)
            moved(to);
        else
            return;
        event.accepted = true;
    }

    Row {
        id: labels
        visible: root.caption !== "" || root.valueText !== ""
        width: parent.width
        Text {
            width: parent.width - valueLabel.implicitWidth
            text: root.caption
            textFormat: Text.PlainText
            color: Theme.muted
            elide: Text.ElideRight
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(15 * root.unit)
        }
        Text {
            id: valueLabel
            text: root.valueText
            textFormat: Text.PlainText
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(15 * root.unit)
        }
    }
    Item {
        id: track
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 20

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            height: 4
            radius: 2
            color: Qt.rgba(1, 1, 1, 0.22)
            Rectangle {
                width: Math.round(handle.x + handle.width / 2)
                height: parent.height
                radius: 2
                color: root.activeFill
            }
        }
        Rectangle {
            id: handle
            width: 16
            height: 16
            radius: 8
            anchors.verticalCenter: parent.verticalCenter
            x: Math.round(root.fraction * (root.width - width))
            color: root.activeFill
            border.width: root.activeFocus ? 2 : 1
            border.color: root.activeFocus ? Theme.text : Qt.rgba(0, 0, 0, 0.12)
        }
        MouseArea {
            anchors.fill: parent
            anchors.topMargin: -8
            enabled: root.enabled
            cursorShape: Qt.PointingHandCursor
            onPressed: mouse => {
                root.forceActiveFocus(Qt.MouseFocusReason);
                root.moved(root.valueAt(mouse.x));
            }
            onPositionChanged: mouse => {
                if (pressed)
                    root.moved(root.valueAt(mouse.x));
            }
        }
    }
}
