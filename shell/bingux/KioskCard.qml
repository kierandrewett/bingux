import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The centred card of a kiosk screen (login, setup). The default layout is a
// compact card: a title row with an optional back button, a body slot, and a
// footer with a secondary button on the left and the primary action on the
// right. Set `wide` for a 16:9 window-like surface sized from the parent: the
// back button, title and `lead` text run down the left, the body fills the
// right, and the footer buttons sit along the bottom. Colours come from the
// shell theme; the card is a translucent popup surface over the wallpaper.
Rectangle {
    id: root

    property string title: ""
    property string iconName: ""
    property bool backVisible: false
    property string primaryText: ""
    property bool primaryEnabled: true
    property bool primaryVisible: true
    property string secondaryText: ""
    property string secondaryIcon: ""
    property bool secondaryVisible: false
    property int bodyHeight: 220
    // Wide mode: a 16:9 card with the title and lead on the left.
    property bool wide: false
    property string lead: ""
    // Wide mode, full body: the title and lead run across the top and the
    // body takes the whole width (the time zone map). Off: title left, body right.
    property bool bodyFull: false
    // Height the parent reserves above the card (the top bar).
    property real verticalReserve: 0
    // The wide card is 16:9, at most 90% of the parent width, and fits the
    // room left under the top bar with a margin of 4% of the screen height.
    readonly property real frameWidth: wide && parent ? Math.round(Math.min(parent.width * 0.9, (parent.height - verticalReserve - 2 * Math.round(parent.height * 0.04)) * 16 / 9)) : 0
    readonly property real frameHeight: Math.round(frameWidth * 9 / 16)
    // Scales type and spacing with the card: 1.08 at 1280x800, 1.5 from 1080 lines up.
    readonly property real uiScale: wide ? Math.max(1, Math.min(1.5, frameHeight / 600)) : 1
    readonly property real framePad: Math.round(frameHeight * 0.06)
    readonly property real footerHeight: Math.round(44 * uiScale)
    readonly property real leftWidthRatio: 0.34
    default property alias body: content.data

    signal backRequested
    signal primaryRequested
    signal secondaryRequested

    implicitWidth: wide ? frameWidth : Math.min(420, parent ? parent.width - Theme.paddingLarge * 2 : 420)
    implicitHeight: wide ? Math.round(frameHeight) : layout.implicitHeight + Theme.paddingLarge * 2
    radius: Theme.cardRadius
    color: Theme.popupSurface
    border.width: 1
    border.color: Qt.rgba(1, 1, 1, 0.12)

    ColumnLayout {
        id: layout
        visible: !root.wide
        anchors.fill: parent
        anchors.margins: Theme.paddingLarge
        spacing: Theme.gap

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            ActionButton {
                objectName: root.wide ? "" : "kioskBack"
                visible: root.backVisible
                flat: true
                iconName: "go-previous-symbolic"
                Accessible.name: "Back"
                onClicked: root.backRequested()
            }
            Item {
                visible: !root.backVisible
                implicitWidth: 36
                implicitHeight: 36
            }
            Text {
                Layout.fillWidth: true
                objectName: root.wide ? "" : "kioskTitle"
                text: root.title
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall + 3
                font.weight: Font.DemiBold
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }
            Item {
                implicitWidth: 36
                implicitHeight: 36
            }
        }

        Item {
            id: narrowBody
            Layout.fillWidth: true
            Layout.preferredHeight: root.bodyHeight
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            ActionButton {
                objectName: root.wide ? "" : "kioskSecondary"
                visible: root.secondaryVisible
                flat: true
                text: root.secondaryText
                iconName: root.secondaryIcon
                onClicked: root.secondaryRequested()
            }
            Item {
                Layout.fillWidth: true
            }
            ActionButton {
                objectName: root.wide ? "" : "kioskPrimary"
                visible: root.primaryVisible
                enabled: root.primaryEnabled
                text: root.primaryText
                Layout.minimumWidth: 110
                onClicked: root.primaryRequested()
            }
        }
    }

    // The wide layout: left column (back, title, lead, secondary action) and a
    // right column (body above the primary action).
    Item {
        id: wideLayout
        visible: root.wide
        anchors.fill: parent
        anchors.margins: root.framePad
        readonly property real leftWidth: Math.round(width * root.leftWidthRatio)
        readonly property real rightX: leftWidth + root.framePad
        readonly property real rightWidth: width - rightX

        ActionButton {
            id: wideBack
            objectName: root.wide ? "kioskBack" : ""
            visible: root.backVisible
            flat: true
            iconName: "go-previous-symbolic"
            Accessible.name: "Back"
            implicitHeight: Math.round(40 * root.uiScale)
            implicitWidth: Math.round(40 * root.uiScale)
            anchors.left: parent.left
            anchors.top: parent.top
            onClicked: root.backRequested()
        }

        Column {
            id: wideTitle
            x: root.bodyFull ? Math.round(44 * root.uiScale) : 0
            y: root.bodyFull ? 0 : Math.round(wideLayout.height * 0.2)
            width: root.bodyFull ? wideLayout.width - x : wideLayout.leftWidth - root.framePad * 0.5
            spacing: Math.round(14 * root.uiScale)

            Text {
                objectName: root.wide ? "kioskTitle" : ""
                width: parent.width
                text: root.title
                wrapMode: Text.WordWrap
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Math.round((root.bodyFull ? 26 : 34) * root.uiScale)
                font.weight: Font.DemiBold
                lineHeight: 1.05
            }
            Text {
                visible: root.lead !== ""
                width: parent.width
                text: root.lead
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Math.round(17 * root.uiScale)
                lineHeight: 1.25
            }
        }

        ActionButton {
            id: wideSecondary
            objectName: root.wide ? "kioskSecondary" : ""
            visible: root.secondaryVisible
            flat: true
            text: root.secondaryText
            iconName: root.secondaryIcon
            height: root.footerHeight
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            onClicked: root.secondaryRequested()
        }

        ActionButton {
            id: widePrimary
            objectName: root.wide ? "kioskPrimary" : ""
            visible: root.primaryVisible
            enabled: root.primaryEnabled
            text: root.primaryText
            height: root.footerHeight
            background: Rectangle {
                radius: widePrimary.cornerRadius
                color: !widePrimary.enabled ? Theme.elevated : widePrimary.down ? Qt.darker(Theme.accent, 1.2) : widePrimary.hovered ? Qt.lighter(Theme.accent, 1.1) : Theme.accent
                border.width: widePrimary.activeFocus ? 2 : 0
                border.color: Theme.text
            }
            implicitWidth: Math.round(150 * root.uiScale)
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            onClicked: root.primaryRequested()
        }

        Item {
            id: wideBody
            readonly property real headerBottom: root.bodyFull ? wideTitle.y + wideTitle.height + root.framePad * 0.6 : 0
            x: root.bodyFull ? 0 : wideLayout.rightX
            y: headerBottom
            width: root.bodyFull ? wideLayout.width : wideLayout.rightWidth
            height: wideLayout.height - root.footerHeight - root.framePad * 0.6 - headerBottom
        }
    }

    // The page content lives in the wide body or the compact slot.
    Item {
        id: content
        parent: root.wide ? wideBody : narrowBody
        anchors.fill: parent
    }
}
