import QtQuick

// Preference pages share one geometry; shell tiles keep their own sizing.
ControlRow {
    iconName: ""
    implicitHeight: subtitle.length > 0 ? 64 : 48
    leftPadding: 0
    rightPadding: 0
}
