import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Widgets

AbstractButton {
    id: root
    required property string iconName
    required property string label
    property url imageSource: ""
    property bool barStyle: false
    property var barWindow: null
    property var presentation: null
    readonly property bool customPresentation: !!presentation?.custom
    readonly property string displayedLabel: presentation?.label || label
    property string tooltipText: displayedLabel
    property bool highlighted: false
    implicitWidth: customPresentation ? Math.max(32, face.implicitWidth + Theme.gap * 2) : 32
    implicitHeight: 32
    hoverEnabled: true
    Accessible.name: displayedLabel
    background: Item {
        ControlCentreButtonSurface { anchors.fill: parent; visible: !root.barStyle; control: root; selected: root.highlighted }
        BarControlSurface { visible: root.barStyle; hovered: root.hovered; pressed: root.down; selected: root.highlighted; focused: root.visualFocus }
    }
    contentItem: Item {
        scale: root.down ? 0.88 : 1
        Behavior on scale { NumberAnimation { duration: Theme.reducedMotion ? 0 : 80; easing.type: Easing.OutCubic } }
        SymbolicIcon { visible: !root.customPresentation && avatar.status !== Image.Ready; anchors.centerIn: parent; implicitSize: 16; source: Quickshell.iconPath(root.iconName); color: root.enabled ? Theme.text : Theme.muted }
        WidgetFace { id: face; anchors.centerIn: parent; visible: root.customPresentation; presentation: root.presentation; iconColor: root.enabled ? Theme.text : Theme.muted }
        ClippingRectangle {
            anchors.centerIn: parent
            width: 24; height: 24; radius: 12
            visible: !root.customPresentation && avatar.status === Image.Ready
            color: "transparent"
            Image { id: avatar; objectName: "iconButtonImage"; anchors.fill: parent; source: root.imageSource; fillMode: Image.PreserveAspectCrop; sourceSize.width: 48; sourceSize.height: 48 }
        }
    }
    BarTooltip { objectName: "iconButtonBarTooltip"; anchorItem: root; barWindow: root.barWindow; requested: root.barStyle && root.visible && root.enabled && (root.hovered || root.visualFocus); text: root.tooltipText }
    ShellTooltip { parent: root; visible: !root.barStyle && root.visible && root.enabled && (root.hovered || root.visualFocus); text: root.tooltipText }
}
