import QtQuick
import Quickshell
import Quickshell.Wayland

// A dialog the shell shows over everything: one layer window, centred, with a
// ShellDialogCard inside. Every shell dialog (not responding, shortcut
// requests, the first-login introduction) is one of these, so titles, body
// text, buttons, danger tone, wrapping and Escape behave the same in each.
//
//   ShellDialog {
//       open: window !== null
//       namespace: "bingux-not-responding"
//       title: "“Notes” is not responding"
//       body: "You can wait for it to respond, or force it to quit."
//       tone: "danger"
//       secondaryText: "Wait"
//       primaryText: "Force Quit"
//       onSecondaryTriggered: wait()
//       onPrimaryTriggered: forceQuit()
//       onDismissed: wait()
//   }
//
// Children go between the body and the buttons. The keyboard is taken on
// demand: the dialog never holds it away from other windows, but the default
// button has focus while the dialog is open.
PanelWindow {
    id: root

    property bool open: false
    // The layer-shell namespace the compositor knows this dialog by.
    property string namespace: "bingux-dialog"
    property real dialogWidth: 420
    property alias title: card.title
    property alias body: card.body
    property alias tone: card.tone
    property alias primaryText: card.primaryText
    property alias secondaryText: card.secondaryText
    property alias primaryEnabled: card.primaryEnabled
    property alias focusPrimary: card.focusPrimary
    default property alias content: card.content
    readonly property alias card: card

    signal primaryTriggered
    signal secondaryTriggered
    signal dismissed

    visible: open
    color: "transparent"
    implicitWidth: dialogWidth + Theme.padding * 2
    implicitHeight: card.implicitHeight + Theme.padding * 2
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: root.namespace
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    onVisibleChanged: if (visible)
        Qt.callLater(card.focusDefault)

    ShellDialogCard {
        id: card
        anchors.centerIn: parent
        width: parent.width - Theme.padding * 2
        focus: root.visible
        onPrimaryTriggered: root.primaryTriggered()
        onSecondaryTriggered: root.secondaryTriggered()
        onDismissed: root.dismissed()
    }
}
