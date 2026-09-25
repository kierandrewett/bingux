import QtQuick

// Notification gesture tests do not exercise compositor blur registration.
QtObject {
    required property var window
    required property string surfaceNamespace
    required property rect region
    property bool enabled: true
}
