pragma Singleton
import QtQuick

// Background helpers (camera and microphone watchers, Steam discovery, the
// audio meter, idle tracking) are not needed for the first frame. They wait
// until the desktop shell has mapped its dock, so they do not compete with it
// for CPU while the bar and dock are loading. A timer starts them anyway if
// the dock never reports ready.
QtObject {
    id: root
    property bool settled: false
    function dockMapped() {
        settleDelay.start();
    }
    property Timer settleDelay: Timer {
        interval: 300
        onTriggered: root.settled = true
    }
    property Timer deadline: Timer {
        interval: 7000
        running: true
        onTriggered: root.settled = true
    }
}
