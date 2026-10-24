import QtQuick

// A large choice row for installer pages: a label, a one-line detail under it
// and a tick when selected. Built on the shell's SessionRow, sized for touch.
SessionRow {
    id: root

    width: parent ? parent.width : 300
    rowHeight: Math.round(58 * InstallerState.uiScale)
    labelPixelSize: Math.round(19 * InstallerState.uiScale)
    contentInset: Math.round(16 * InstallerState.uiScale)
}
