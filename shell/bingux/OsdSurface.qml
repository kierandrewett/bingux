import QtQuick
import Quickshell
import Quickshell.Wayland

QtObject {
    id: root

    required property QtObject state
    property var osdWindows
    property real dockSafeInset: Theme.dockExclusiveHeight
    property var sidebarScreen: null
    property real leftInset: 0
    property real rightInset: 0

    osdWindows: Variants {
        model: Quickshell.screens

        PanelWindow {
            id: osdWindow

            property var modelData
            readonly property bool hasSidebar: root.sidebarScreen !== null && root.sidebarScreen.name === modelData.name
            readonly property real leftInset: hasSidebar ? root.leftInset : 0
            readonly property real rightInset: hasSidebar ? root.rightInset : 0
            readonly property real availableWidth: Math.max(0, width - leftInset - rightInset)
            readonly property var request: root.state.requestForOutputName(modelData.name)

            screen: modelData
            color: "transparent"
            focusable: false
            visible: request !== null || osdCard.opacity > 0
            exclusionMode: ExclusionMode.Ignore
            surfaceFormat.opaque: false
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-osd"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }

            Item {
                id: clickThroughTarget

                width: 0
                height: 0
            }

            OsdCard {
                id: osdCard

                request: osdWindow.request
                availableWidth: osdWindow.availableWidth
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.horizontalCenterOffset: (osdWindow.leftInset - osdWindow.rightInset) / 2
                anchors.bottom: parent.bottom
                anchors.bottomMargin: root.dockSafeInset + Theme.gap * 2
            }

            mask: Region {
                item: clickThroughTarget
            }
        }
    }
}
