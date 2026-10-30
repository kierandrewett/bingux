import QtQuick

// Pages inside a kiosk card. Each direct child carries a `pageId`; `current`
// names the visible one. go() pushes the current page onto a history and
// back() pops it, so a back button and Escape return to the page that opened
// the current one. Moves slide and fade; under Theme.reducedMotion they cut.
Item {
    id: root

    property string current: ""
    property var history: []
    readonly property int depth: history.length
    readonly property bool canGoBack: history.length > 0
    property bool forward: true
    default property alias pages: body.data

    function go(pageId) {
        if (pageId === current)
            return;
        forward = true;
        history = history.concat([current]);
        current = pageId;
    }
    function back() {
        if (history.length === 0)
            return false;
        forward = false;
        const previous = history[history.length - 1];
        history = history.slice(0, history.length - 1);
        current = previous;
        return true;
    }
    function replace(pageId) {
        current = pageId;
    }

    Item {
        id: body
        anchors.fill: parent
    }
}
