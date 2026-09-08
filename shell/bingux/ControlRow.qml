import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

// Rows can navigate as a whole, or present independent controls on a passive surface.
AbstractButton {
    id: root
    required property string title
    property string subtitle: ""
    required property string iconName
    property bool selected: false
    property bool navigation: false
    property real navigationRotation: 0
    property bool rowInteractive: true
    hoverEnabled: rowInteractive
    HoverHandler { id: rowHover }
    focusPolicy: rowInteractive ? Qt.StrongFocus : Qt.NoFocus
    Accessible.role: rowInteractive ? Accessible.Button : Accessible.Grouping
    signal navigationRequested(var trigger)
    property bool tileLayout: false
    property bool compactTile: false
    property bool tileSurface: tileLayout
    property bool leadingBadge: false
    property bool toggleVisible: false
    property bool toggleEnabled: true
    property bool toggleChecked: selected
    property string toggleLabel: title
    property string actionLabel: ""
    property string valueText: ""
    signal toggleRequested()
    signal actionTriggered()
    Layout.fillWidth: true
    // Tiles should group related controls, not look like oversized app icons.
    // Their compact height also lets the primary controls breathe as a set.
    implicitHeight: tileLayout ? (compactTile ? (subtitle.length > 0 ? 96 : 84) : 104) : tileSurface || leadingBadge || subtitle.length > 0 ? 56 : 40
    padding: tileLayout ? (compactTile ? Theme.compactTilePadding : Theme.controlTilePadding) : 0
    Accessible.name: title + (subtitle ? ", " + subtitle : "")
    background: ControlCentreButtonSurface {
        visible: root.rowInteractive || root.tileSurface
        control: root
        // Selection stays quiet: a device row receives a restrained fill and
        // its existing checkmark, while quick-control tiles use icon/switch
        // colour rather than a full blue card.
        selected: root.selected && !root.tileSurface
        baseColor: root.tileSurface ? Theme.elevated : "transparent"
        outlined: false
        radius: root.tileSurface ? 12 : root.leadingBadge ? 10 : 8
    }
    contentItem: GridLayout {
        columns: root.tileLayout ? 2 : 3
        columnSpacing: root.tileLayout ? Theme.gap : 10
        rowSpacing: root.tileLayout ? Theme.gap : 0
        Rectangle {
            visible: root.iconName.length > 0
            Layout.row: 0
            Layout.column: 0
            Layout.leftMargin: root.tileLayout ? 0 : 12
            implicitWidth: root.tileLayout ? 24 : root.leadingBadge ? 30 : 18
            implicitHeight: implicitWidth
            radius: root.tileLayout ? 7 : 10
            color: root.leadingBadge && !root.tileLayout ? (root.selected ? Theme.selection : Theme.elevated) : "transparent"
            SymbolicIcon { anchors.centerIn: parent; implicitSize: root.tileLayout ? 18 : 18; source: root.iconName.length > 0 ? Quickshell.iconPath(root.iconName) : ""; color: root.selected ? Theme.accent : Theme.muted }
        }
        ColumnLayout {
            Layout.row: root.tileLayout ? 1 : 0
            Layout.column: root.tileLayout ? 0 : 1
            Layout.columnSpan: root.tileLayout ? 2 : 1
            Layout.leftMargin: root.tileLayout || root.iconName.length > 0 ? 0 : 12
            Layout.fillWidth: true
            spacing: 1
            MarqueeText {
                objectName: "controlRowTitle"
                Layout.fillWidth: true
                text: root.title
                textFormat: Text.PlainText
                active: root.visible && (rowHover.hovered || root.visualFocus)
                color: root.enabled ? Theme.text : Theme.muted
                fontWeight: root.leadingBadge || root.tileLayout ? Font.Medium : Font.Normal
            }
            MarqueeText {
                objectName: "controlRowSubtitle"
                visible: root.subtitle.length > 0
                Layout.fillWidth: true
                text: root.subtitle
                textFormat: Text.PlainText
                active: root.visible && (rowHover.hovered || root.visualFocus)
                color: Theme.muted
                pixelSize: Theme.fontSmall
            }
        }
        RowLayout {
            Layout.row: 0
            Layout.column: root.tileLayout ? 1 : 2
            // An empty layout otherwise expands and steals the label's space
            // on rows without a checkmark or trailing action.
            Layout.fillWidth: false
            Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
            Layout.rightMargin: root.tileLayout ? 0 : Theme.padding
            spacing: root.tileLayout ? 4 : 10
            Text { visible: root.valueText.length > 0; text: root.valueText; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            AbstractButton {
                id: secondary
                visible: root.actionLabel.length > 0
                implicitWidth: actionText.implicitWidth + Theme.gap * 2
                implicitHeight: Theme.controlHeight
                hoverEnabled: true
                Accessible.name: root.actionLabel + " " + root.title
                onClicked: root.actionTriggered()
                background: ControlCentreButtonSurface { control: secondary; radius: 7 }
                contentItem: Text { id: actionText; text: root.actionLabel; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            }
            ControlSwitch {
                visible: root.toggleVisible
                enabled: root.toggleEnabled
                checked: root.toggleChecked
                Accessible.name: root.toggleLabel
                objectName: root.objectName + "Switch"
                onToggled: root.toggleRequested()
            }
            SymbolicIcon {
                visible: (root.navigation && root.rowInteractive) || (root.selected && !root.toggleVisible && !root.navigation)
                implicitSize: 12
                rotation: root.navigation ? root.navigationRotation : 0
                Behavior on rotation { NumberAnimation { duration: Theme.reducedMotion ? 0 : Theme.motion; easing.type: Easing.OutCubic } }
                color: root.navigation ? Theme.muted : Theme.accent
                source: Quickshell.iconPath(root.navigation ? "go-next-symbolic" : "object-select-symbolic")
            }
            IconButton {
                objectName: root.objectName + "Navigation"
                visible: root.navigation && !root.rowInteractive
                iconName: "go-next-symbolic"
                label: root.title + " settings"
                onClicked: root.navigationRequested(this)
            }
        }
    }
}
