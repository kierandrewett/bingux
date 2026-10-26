import QtQuick
import Quickshell
import ".."

// The date and the time, large, with a soft shadow. The clock format comes
// from the theme (the account's published choice, else the locale).
SessionShadow {
    id: root

    property AuthTheme theme: AuthTheme {}
    property date now: new Date()

    Accessible.role: Accessible.StaticText
    Accessible.name: Qt.formatDate(root.now, "dddd d MMMM") + ", " + Qt.formatTime(root.now, root.theme.twelveHourClock ? "h:mm AP" : "HH:mm")

    Column {
        spacing: 2
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.formatDate(root.now, "ddd d MMM")
            color: root.theme.ink
            font.family: Theme.fontFamily
            font.pixelSize: 20
            font.weight: Font.DemiBold
            opacity: 0.9
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.formatTime(root.now, root.theme.twelveHourClock ? "h:mm" : "HH:mm")
            color: root.theme.ink
            font.family: Theme.fontFamily
            font.pixelSize: 120
            font.weight: Font.Light
            font.letterSpacing: -3
            lineHeight: 0.85
        }
    }

    SystemClock {
        precision: SystemClock.Minutes
        onDateChanged: root.now = date
    }
}
