import QtQuick
import Quickshell

ActionButton {
    property string dbusPath: "gdbus"
    text: "New notification"
    flat: true
    implicitHeight: Theme.barHeight
    onClicked: Quickshell.execDetached([dbusPath, "call", "--session", "--dest", "org.freedesktop.Notifications", "--object-path", "/org/freedesktop/Notifications", "--method", "org.freedesktop.Notifications.Notify", "Bingux", "0", "preferences-desktop-notification", "Notification preview", "Hover to pause the timer. Drag right to dismiss, or pull back to cancel.", "[]", "{}", "8000"])
}
