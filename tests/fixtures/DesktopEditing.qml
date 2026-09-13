pragma Singleton
import QtQuick

QtObject {
    readonly property bool active: false
    function observeGeometry(item) {
    }
    function point(item, window, x, y) {
        return item.mapToItem(window.contentItem, x, y);
    }
}
