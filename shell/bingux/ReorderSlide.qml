import QtQuick

// Shared displacement motion for dock and top-bar rearranging.
Translate {
    property bool animate: true
    Behavior on x {
        enabled: animate
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 180
            easing.type: Easing.OutCubic
        }
    }
    Behavior on y {
        enabled: animate
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 180
            easing.type: Easing.OutCubic
        }
    }
}
