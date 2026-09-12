import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets

QtObject {
    id: root

    required property QtObject state
    property var osdWindows
    property real dockSafeInset: Theme.dockExclusiveHeight
    property var sidebarScreen: null
    property real leftInset: 0
    property real rightInset: 0

    function labelFor(request) {
        if (request.icon.startsWith("microphone-") || request.icon.startsWith("audio-input-"))
            return "Microphone";
        if (request.icon.startsWith("audio-"))
            return "Volume";
        if (request.icon.startsWith("display-brightness"))
            return "Brightness";
        if (request.icon.startsWith("keyboard-brightness"))
            return "Keyboard brightness";
        return request.label || "System control";
    }

    function detailFor(request) {
        if (request.icon.includes("muted"))
            return "Muted";
        return request.label !== labelFor(request) ? request.label : "";
    }

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
            // Retain the content until the close fade has finished.
            property var presentedRequest: null
            readonly property bool hasLevel: presentedRequest !== null && presentedRequest.maxLevel >= 0 && presentedRequest.level >= 0
            readonly property bool iconOnly: presentedRequest !== null && (presentedRequest.icon === "action-unavailable-symbolic" || (!hasLevel && !presentedRequest.label && root.labelFor(presentedRequest) === "System control"))
            readonly property real maximum: presentedRequest && presentedRequest.maxLevel > 0 ? presentedRequest.maxLevel : 1
            readonly property real levelFraction: hasLevel ? Math.min(1, presentedRequest.level / maximum) : 0
            readonly property string percentLabel: hasLevel ? Math.round(Math.max(0, presentedRequest.level) * 100) + "%" : ""
            onRequestChanged: {
                if (request) {
                    presentedRequest = request;
                    if (osdCard.opacity === 0) {
                        entrance.from = Theme.reducedMotion ? 1 : Theme.popupInitialScale;
                        entrance.restart();
                    }
                } else {
                    // Closing changes opacity only, including during entry.
                    entrance.stop();
                }
            }

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

            Rectangle {
                id: osdCard
                objectName: "osdCard"

                width: Math.max(0, Math.min(osdWindow.iconOnly ? Theme.osdIconTileSize + Theme.osdPadding * 2 : Theme.osdWidth, osdWindow.availableWidth - Theme.osdPadding * 2))
                height: content.implicitHeight + Theme.osdPadding * 2
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.horizontalCenterOffset: (osdWindow.leftInset - osdWindow.rightInset) / 2
                anchors.bottom: parent.bottom
                anchors.bottomMargin: root.dockSafeInset + Theme.gap * 2
                radius: Theme.shellRadius
                color: Theme.popupSurface
                border.width: 1
                border.color: Theme.outline
                PanelOutline {
                    surface: osdCard
                }
                opacity: osdWindow.request !== null ? 1 : 0
                Behavior on opacity {
                    NumberAnimation {
                        duration: osdWindow.request !== null ? Theme.popupOpenMotion : Theme.osdCloseMotion
                        easing.type: Easing.OutCubic
                    }
                }
                NumberAnimation {
                    id: entrance
                    target: osdCard
                    property: "scale"
                    to: 1
                    duration: Theme.popupOpenMotion
                    easing.type: Easing.OutCubic
                }
                Accessible.role: Accessible.Indicator
                Accessible.name: osdWindow.iconOnly ? "Action unavailable" : (osdWindow.presentedRequest ? root.labelFor(osdWindow.presentedRequest) : "") + " " + osdWindow.percentLabel

                ColumnLayout {
                    id: content
                    anchors.fill: parent
                    anchors.margins: Theme.osdPadding
                    spacing: Theme.padding

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.padding

                        Rectangle {
                            implicitWidth: Theme.osdIconTileSize
                            implicitHeight: implicitWidth
                            radius: Theme.insetRadius(osdCard.radius, Theme.osdPadding)
                            color: osdWindow.iconOnly ? "transparent" : Theme.elevated
                            SymbolicIcon {
                                anchors.centerIn: parent
                                implicitSize: Theme.osdIconSize
                                color: osdWindow.presentedRequest?.icon.includes("muted") ? Theme.muted : Theme.accent
                                source: Quickshell.iconPath(osdWindow.presentedRequest ? osdWindow.presentedRequest.icon : "dialog-information-symbolic", "dialog-information-symbolic")
                            }
                        }
                        ColumnLayout {
                            visible: !osdWindow.iconOnly
                            Layout.fillWidth: true
                            spacing: Theme.spaceSmall
                            Text {
                                objectName: "osdTitle"
                                Layout.fillWidth: true
                                color: Theme.text
                                elide: Text.ElideRight
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontHeading
                                font.weight: Font.DemiBold
                                text: osdWindow.presentedRequest ? root.labelFor(osdWindow.presentedRequest) : ""
                                textFormat: Text.PlainText
                            }
                            Text {
                                objectName: "osdDetail"
                                Layout.fillWidth: true
                                visible: text.length > 0
                                color: Theme.muted
                                elide: Text.ElideRight
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                text: osdWindow.presentedRequest ? root.detailFor(osdWindow.presentedRequest) : ""
                                textFormat: Text.PlainText
                            }
                        }
                        AnimatedCount {
                            objectName: "osdPercent"
                            value: osdWindow.hasLevel ? Math.round(Math.max(0, osdWindow.presentedRequest.level) * 100) : 0
                            formatter: value => String(value) + "%"
                            stepValues: false
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.osdPercentSize
                            font.weight: Font.DemiBold
                            font.features: ({
                                    "tnum": 1
                                })
                            visible: osdWindow.hasLevel && !osdWindow.iconOnly
                        }
                    }
                    Rectangle {
                        objectName: "osdTrack"
                        Layout.fillWidth: true
                        visible: osdWindow.hasLevel && !osdWindow.iconOnly
                        implicitHeight: Theme.sliderActiveTrackHeight
                        radius: height / 2
                        color: Theme.outline

                        Rectangle {
                            objectName: "osdLevel"
                            width: parent.width * osdWindow.levelFraction
                            height: parent.height
                            radius: parent.radius
                            color: Theme.accent
                            Behavior on width {
                                enabled: osdCard.opacity > 0 && osdWindow.request !== null
                                NumberAnimation {
                                    duration: Theme.osdLevelMotion
                                    easing.type: Easing.OutCubic
                                }
                            }
                        }
                        Rectangle {
                            visible: osdWindow.maximum > 1
                            x: parent.width / osdWindow.maximum
                            width: 2
                            height: parent.height
                            color: Theme.popupSurface
                        }
                    }
                }
            }

            mask: Region {
                item: clickThroughTarget
            }
        }
    }
}
