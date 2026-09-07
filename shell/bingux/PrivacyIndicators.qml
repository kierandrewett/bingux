import QtQuick
import QtQuick.Layouts

Item {
    id: root

    required property var metrics

    implicitWidth: privacyRow.implicitWidth
    implicitHeight: privacyRow.implicitHeight
    width: implicitWidth
    height: implicitHeight

    component PrivacyIndicator: Item {
        id: indicator

        required property bool active
        required property string label
        required property string accessibleName

        visible: active
        implicitWidth: indicatorRow.implicitWidth
        implicitHeight: 24
        Accessible.name: accessibleName
        Accessible.role: Accessible.StaticText

        RowLayout {
            id: indicatorRow

            anchors.verticalCenter: parent.verticalCenter
            spacing: 4

            Rectangle {
                Layout.preferredWidth: 6
                Layout.preferredHeight: 6
                radius: width / 2
                color: "#f4a340"
            }

            Text {
                color: "#f4a340"
                font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                text: indicator.label
            }
        }
    }

    RowLayout {
        id: privacyRow

        spacing: 8

        PrivacyIndicator {
            active: root.metrics.screenSharing
            label: "SHARE"
            accessibleName: "Screen sharing is active"
        }

        PrivacyIndicator {
            active: root.metrics.microphoneInUse
            label: "MIC"
            accessibleName: "Microphone is active"
        }

        PrivacyIndicator {
            active: root.metrics.locationInUse
            label: "LOC"
            accessibleName: "Location access is active"
        }
    }
}
