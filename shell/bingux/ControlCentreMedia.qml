import QtQuick

// Both surfaces use the same MPRIS controls, seek bar and artwork cache.
MediaControls {
    property bool active: false
    player: null
    menuActive: active
    compact: true
    cornerRadius: 16
}
