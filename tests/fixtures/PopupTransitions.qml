pragma Singleton
import QtQuick

QtObject {
    readonly property var policies: ({})
    function refresh(namespace) {
    }
    function matches(duration, easing, namespace) {
        return false;
    }
    function fadesIn(namespace) {
        return false;
    }
}
