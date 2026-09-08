import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

AbstractButton {
    id: root
    property bool flat: false
    property bool alignLeft: false
    property string iconName: ""
    property string trailingIconName: ""
    property bool wrapLabel: false
    horizontalPadding: Theme.padding
    property bool showFocusRing: true
    property real cornerRadius: Theme.insetRadius(Theme.cardRadius, Theme.padding)
    implicitWidth: Math.max(36, label.implicitWidth + horizontalPadding * 2 + (iconName ? Theme.iconSize + Theme.gap : 0) + (trailingIconName ? Theme.iconSize + Theme.gap : 0))
    implicitHeight: root.wrapLabel ? Math.max(36, label.implicitHeight + 16) : 36
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    background: Rectangle {
        radius: root.cornerRadius
        color: root.down ? Theme.pressed : root.hovered ? Theme.hover : root.flat ? "transparent" : Theme.elevated
        border.width: root.showFocusRing && root.activeFocus ? 2 : 0
        border.color: Theme.accent
    }
    contentItem: Item {
      RowLayout {
        anchors.verticalCenter: parent.verticalCenter
        x: root.alignLeft ? root.horizontalPadding : (parent.width - width) / 2
        width: Math.min(parent.width, implicitWidth)
        spacing: root.iconName || root.trailingIconName ? Theme.gap : 0
        SymbolicIcon {
            visible: root.iconName !== ""
            Layout.preferredWidth: Theme.iconSize
            Layout.preferredHeight: Theme.iconSize
            Layout.alignment: Qt.AlignVCenter
            implicitSize: Theme.iconSize
            source: root.iconName ? Quickshell.iconPath(root.iconName) : ""
            color: root.enabled ? Theme.text : Theme.muted
        }
        Text {
            id: label
            Layout.alignment: Qt.AlignVCenter
            Layout.maximumWidth: Math.max(0, root.width - root.horizontalPadding * 2 - (root.iconName ? Theme.iconSize + Theme.gap : 0) - (root.trailingIconName ? Theme.iconSize + Theme.gap : 0))
            text: root.text
            textFormat: Text.PlainText
            wrapMode: root.wrapLabel ? Text.Wrap : Text.NoWrap
            elide: root.wrapLabel ? Text.ElideNone : Text.ElideRight
            color: root.enabled ? Theme.text : Theme.muted
            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        SymbolicIcon {
            visible: root.trailingIconName !== ""
            Layout.preferredWidth: Theme.iconSize
            Layout.preferredHeight: Theme.iconSize
            Layout.alignment: Qt.AlignVCenter
            implicitSize: Theme.iconSize
            source: root.trailingIconName ? Quickshell.iconPath(root.trailingIconName) : ""
            color: root.enabled ? Theme.text : Theme.muted
        }
      }
    }
}
