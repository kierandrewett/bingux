import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The centred card of a kiosk screen (login, setup): a title row with an
// optional back button, a body slot, and a footer with a secondary button on
// the left and the primary action on the right. Colours come from the shell
// theme; the card is a translucent popup surface over the wallpaper.
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
    default property alias body: content.data

    signal backRequested
    signal primaryRequested
    signal secondaryRequested

    implicitWidth: Math.min(420, parent ? parent.width - Theme.paddingLarge * 2 : 420)
    implicitHeight: layout.implicitHeight + Theme.paddingLarge * 2
    radius: Theme.cardRadius
    color: Theme.popupSurface
    border.width: 1
    border.color: Qt.rgba(1, 1, 1, 0.12)

    ColumnLayout {
        id: layout
        anchors.fill: parent
        anchors.margins: Theme.paddingLarge
        spacing: Theme.gap

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            ActionButton {
                objectName: "kioskBack"
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
                objectName: "kioskTitle"
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
            id: content
            Layout.fillWidth: true
            Layout.preferredHeight: root.bodyHeight
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            ActionButton {
                objectName: "kioskSecondary"
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
                objectName: "kioskPrimary"
                visible: root.primaryVisible
                enabled: root.primaryEnabled
                text: root.primaryText
                Layout.minimumWidth: 110
                onClicked: root.primaryRequested()
            }
        }
    }
}
