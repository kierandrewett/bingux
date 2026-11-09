import QtQuick
import Quickshell
import ".."

// The date and the time, large, with a soft shadow. The clock format comes
// from the theme (the account's published choice, else the locale).
SessionShadow {
    id: root

    property AuthTheme theme: AuthTheme {}
    property date now: new Date()

    // Qt's "h" is a 12 hour field only together with an AM/PM marker, so the
    // 12 hour clock face is composed by hand; the marker is drawn beside it.
    readonly property string periodText: root.theme.twelveHourClock ? Qt.formatTime(root.now, "AP") : ""
    readonly property string timeText: {
        if (!root.theme.twelveHourClock)
            return Qt.formatTime(root.now, "HH:mm");
        const hour = root.now.getHours() % 12;
        return (hour === 0 ? 12 : hour) + Qt.formatTime(root.now, ":mm");
    }

    Accessible.role: Accessible.StaticText
    Accessible.name: Qt.formatDate(root.now, "dddd d MMMM") + ", " + Qt.formatTime(root.now, root.theme.twelveHourClock ? "h:mm AP" : "HH:mm")

    Column {
        spacing: 2
        Text {
            textFormat: Text.PlainText
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.formatDate(root.now, "ddd d MMM")
            color: root.theme.ink
            font.family: Theme.fontFamily
            font.pixelSize: 20
            font.weight: Font.DemiBold
            opacity: 0.9
        }
        Item {
            // The time stays centred; AM or PM hangs off its right edge on the
            // same baseline, so 5:58 is never ambiguous.
            anchors.horizontalCenter: parent.horizontalCenter
            width: timeLabel.width
            height: timeLabel.height
            Text {
                id: timeLabel
                objectName: "authClockTime"
                textFormat: Text.PlainText
                text: root.timeText
                color: root.theme.ink
                font.family: Theme.fontFamily
                font.pixelSize: 120
                font.weight: Font.Light
                font.letterSpacing: -3
                lineHeight: 0.85
            }
            Text {
                objectName: "authClockPeriod"
                visible: root.periodText !== ""
                x: timeLabel.width + 8
                anchors.baseline: timeLabel.baseline
                textFormat: Text.PlainText
                text: root.periodText
                color: root.theme.ink
                font.family: Theme.fontFamily
                font.pixelSize: 32
                font.weight: Font.DemiBold
                opacity: 0.8
            }
        }
    }

    SystemClock {
        precision: SystemClock.Minutes
        onDateChanged: root.now = date
    }
}
