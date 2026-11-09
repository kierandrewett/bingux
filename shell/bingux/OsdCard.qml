import QtQuick
import QtQuick.Layouts

// One OSD card: icon, label, optional level bar and percentage. The desktop
// OSD surface and the lock screen both draw it, so a volume change looks the
// same on either. The host places the card and passes the request to show
// (an OsdState record, or null). The card fades in and out by itself and
// keeps the last request until the fade ends.
//
// The colours default to the desktop theme. The lock screen overrides them
// with its own translucent glass, because its wallpaper is arbitrary.
Rectangle {
    id: card
    objectName: "osdCard"

    property var request: null
    // Width the card may use, for example the part of an output that a
    // sidebar leaves free. The card keeps one padding unit at each side.
    property real availableWidth: Number.POSITIVE_INFINITY
    property color ink: Theme.text
    property color inkMuted: Theme.muted
    property color accent: Theme.accent
    property color tileColor: Theme.elevated
    property color trackColor: Theme.outline
    // Draws the shell's outer stroke around the card.
    property bool panelOutline: true
    // Retain the content until the close fade has finished.
    property var presentedRequest: null
    readonly property bool hasLevel: presentedRequest !== null && presentedRequest.maxLevel >= 0 && presentedRequest.level >= 0
    readonly property bool unavailable: presentedRequest !== null && presentedRequest.icon === "action-unavailable-symbolic"
    readonly property real maximum: presentedRequest && presentedRequest.maxLevel > 0 ? presentedRequest.maxLevel : 1
    readonly property real levelFraction: hasLevel ? Math.min(1, presentedRequest.level / maximum) : 0
    readonly property string percentLabel: hasLevel ? Math.round(Math.max(0, presentedRequest.level) * 100) + "%" : ""

    function labelFor(request) {
        // An action the hardware cannot do says which one, in words.
        if (request.icon === "action-unavailable-symbolic")
            return request.label ? request.label + " unavailable" : "Action unavailable";
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
        if (request.icon === "action-unavailable-symbolic")
            return "";
        if (request.icon.includes("muted"))
            return "Muted";
        return request.label !== labelFor(request) ? request.label : "";
    }

    onRequestChanged: {
        if (request) {
            presentedRequest = request;
            if (opacity === 0) {
                entrance.from = Theme.reducedMotion ? 1 : Theme.popupInitialScale;
                entrance.restart();
            }
        } else {
            // Closing changes opacity only, including during entry.
            entrance.stop();
        }
    }

    width: Math.max(0, Math.min(Theme.osdWidth, availableWidth - Theme.osdPadding * 2))
    // One height for every OSD, with or without a detail line or a level bar,
    // so the card does not jump between a volume change and a status message.
    height: Theme.osdHeight
    radius: Theme.shellRadius
    color: Theme.popupSurface
    border.width: 1
    border.color: Theme.outline
    opacity: request !== null ? 1 : 0
    Behavior on opacity {
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : (card.request !== null ? Theme.popupOpenMotion : Theme.osdCloseMotion)
            easing.type: Easing.OutCubic
        }
    }
    NumberAnimation {
        id: entrance
        target: card
        property: "scale"
        to: 1
        duration: Theme.reducedMotion ? 0 : (Theme.popupOpenMotion)
        easing.type: Easing.OutCubic
    }
    Accessible.role: Accessible.Indicator
    Accessible.name: (presentedRequest ? labelFor(presentedRequest) : "") + (percentLabel !== "" ? " " + percentLabel : "")

    Loader {
        active: card.panelOutline
        sourceComponent: PanelOutline {
            surface: card
        }
    }

    ColumnLayout {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: Theme.osdPadding
        anchors.rightMargin: Theme.osdPadding
        spacing: Theme.padding

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.padding

            Rectangle {
                implicitWidth: Theme.osdIconTileSize
                implicitHeight: implicitWidth
                radius: Theme.insetRadius(card.radius, Theme.osdPadding)
                color: card.tileColor
                SymbolicIcon {
                    anchors.centerIn: parent
                    implicitSize: Theme.osdIconSize
                    color: card.unavailable || card.presentedRequest?.icon.includes("muted") ? card.inkMuted : card.accent
                    source: BinguxIcons.source(card.presentedRequest ? card.presentedRequest.icon : "dialog-information-symbolic", "dialog-information-symbolic")
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceSmall
                Text {
                    objectName: "osdTitle"
                    Layout.fillWidth: true
                    color: card.ink
                    elide: Text.ElideRight
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontHeading
                    font.weight: Font.DemiBold
                    text: card.presentedRequest ? card.labelFor(card.presentedRequest) : ""
                    textFormat: Text.PlainText
                }
                Text {
                    objectName: "osdDetail"
                    Layout.fillWidth: true
                    visible: text.length > 0
                    color: card.inkMuted
                    elide: Text.ElideRight
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    text: card.presentedRequest ? card.detailFor(card.presentedRequest) : ""
                    textFormat: Text.PlainText
                }
            }
            AnimatedCount {
                objectName: "osdPercent"
                value: card.hasLevel ? Math.round(Math.max(0, card.presentedRequest.level) * 100) : 0
                formatter: value => String(value) + "%"
                stepValues: false
                color: card.ink
                font.family: Theme.fontFamily
                font.pixelSize: Theme.osdPercentSize
                font.weight: Font.DemiBold
                font.features: ({
                        "tnum": 1
                    })
                visible: card.hasLevel
            }
        }
        Rectangle {
            objectName: "osdTrack"
            Layout.fillWidth: true
            visible: card.hasLevel
            implicitHeight: Theme.sliderActiveTrackHeight
            radius: height / 2
            color: card.trackColor

            Rectangle {
                objectName: "osdLevel"
                width: parent.width * card.levelFraction
                height: parent.height
                radius: parent.radius
                color: card.accent
                Behavior on width {
                    enabled: card.opacity > 0 && card.request !== null
                    NumberAnimation {
                        duration: Theme.reducedMotion ? 0 : (Theme.osdLevelMotion)
                        easing.type: Easing.OutCubic
                    }
                }
            }
            Rectangle {
                visible: card.maximum > 1
                x: parent.width / card.maximum
                width: 2
                height: parent.height
                color: card.color
            }
        }
    }
}
