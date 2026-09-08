import QtQuick
import QtQuick.Layouts
import Quickshell

// GNOME panel activity styling with the bar's full-height pointer target.
Item {
    id: root
    property string label: ""
    property string iconName: ""
    property string trailingIcon: ""
    property string tooltip: ""
    property bool filled: false
    property bool interactive: false
    property bool reorderable: false
    property color activityColor: Theme.privacyIndicator
    property var barWindow: null
    readonly property bool hovered: mouse.containsMouse
    readonly property bool pressed: mouse.pressed
    signal clicked()
    implicitHeight: Theme.barHeight
    implicitWidth: row.implicitWidth + Theme.activityIndicatorPadding * 2
    activeFocusOnTab: interactive
    Accessible.role: interactive ? Accessible.Button : Accessible.StaticText
    Accessible.name: tooltip
    Accessible.onPressAction: if (interactive) clicked()
    Keys.onReturnPressed: if (interactive) clicked()
    Keys.onSpacePressed: if (interactive) clicked()

    Rectangle {
        anchors.fill: parent
        anchors.margins: Theme.barControlInset
        radius: Theme.barControlRadius
        color: root.filled ? root.activityColor : "transparent"
        border.width: root.activeFocus ? 1 : 0
        border.color: Theme.text
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: root.pressed ? "#30000000" : root.interactive && root.hovered ? "#20ffffff" : "transparent"
        }
    }
    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: Theme.spaceSmall
        SymbolicIcon {
            visible: root.iconName !== ""
            implicitSize: Theme.iconSize
            source: root.iconName ? Quickshell.iconPath(root.iconName) : ""
            color: root.filled ? Theme.text : root.activityColor
        }
        Text {
            visible: root.label !== ""
            text: root.label
            color: root.filled ? Theme.text : root.activityColor
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
            font.features: {"tnum": 1}
            textFormat: Text.PlainText
        }
        Item {
            visible: root.trailingIcon !== ""
            implicitWidth: Theme.iconSize
            implicitHeight: Theme.iconSize
            // GNOME's screencast stop glyph is a 10px rounded square in a 16px box.
            Rectangle {
                anchors.centerIn: parent
                width: 10
                height: 10
                radius: 1.5
                visible: root.trailingIcon === "screencast-stop-symbolic"
                color: root.filled ? Theme.text : root.activityColor
            }
            SymbolicIcon {
                anchors.fill: parent
                visible: root.trailingIcon !== "screencast-stop-symbolic"
                implicitSize: Theme.iconSize
                source: visible && root.trailingIcon ? Quickshell.iconPath(root.trailingIcon) : ""
                color: root.filled ? Theme.text : root.activityColor
            }
        }
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: root.interactive ? Qt.LeftButton : Qt.NoButton
        cursorShape: Qt.ArrowCursor
        onClicked: root.clicked()
    }
    BarTooltip {
        anchorItem: root
        barWindow: root.barWindow
        requested: mouse.containsMouse && root.barWindow !== null
        text: root.tooltip
        reorderable: root.reorderable
    }
}
