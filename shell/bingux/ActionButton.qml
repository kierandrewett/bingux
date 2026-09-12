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
    property var presentation: null
    readonly property string displayedText: presentation?.label ?? text
    readonly property string displayedIcon: presentation?.icon ?? iconName
    readonly property bool showLabel: presentation?.showText ?? true
    readonly property bool showIcon: presentation?.showIcon ?? !!iconName
    property bool wrapLabel: false
    horizontalPadding: Theme.padding
    property bool showFocusRing: true
    property real cornerRadius: Theme.insetRadius(Theme.cardRadius, Theme.padding)
    implicitWidth: Math.max(36, (showLabel ? label.implicitWidth : 0) + horizontalPadding * 2 + (showIcon ? Theme.iconSize + Theme.gap : 0) + (trailingIconName ? Theme.iconSize + Theme.gap : 0))
    implicitHeight: root.wrapLabel ? Math.max(36, label.implicitHeight + 16) : 36
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: displayedText
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
            spacing: root.showIcon || root.trailingIconName ? Theme.gap : 0
            SymbolicIcon {
                visible: root.showIcon
                Layout.preferredWidth: Theme.iconSize
                Layout.preferredHeight: Theme.iconSize
                Layout.alignment: Qt.AlignVCenter
                implicitSize: Theme.iconSize
                source: root.displayedIcon ? Quickshell.iconPath(root.displayedIcon) : ""
                color: root.enabled ? Theme.text : Theme.muted
            }
            Text {
                id: label
                visible: root.showLabel
                Layout.alignment: Qt.AlignVCenter
                Layout.maximumWidth: Math.max(0, root.width - root.horizontalPadding * 2 - (root.showIcon ? Theme.iconSize + Theme.gap : 0) - (root.trailingIconName ? Theme.iconSize + Theme.gap : 0))
                text: root.displayedText
                textFormat: Text.PlainText
                wrapMode: root.wrapLabel ? Text.Wrap : Text.NoWrap
                elide: root.wrapLabel ? Text.ElideNone : Text.ElideRight
                color: root.enabled ? Theme.text : Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize
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
