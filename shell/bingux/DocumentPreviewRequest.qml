import QtQuick

QtObject {
    id: root
    property var command: []
    property bool active: true
    property bool pending: false
    property string requestId: ""
    property bool completed: false
    signal response(var data)
    onCommandChanged: schedule()
    onActiveChanged: schedule()
    Component.onCompleted: schedule()
    Component.onDestruction: PreviewService.cancel(requestId)
    function schedule() {
        PreviewService.cancel(requestId);
        requestId = "";
        pending = active;
        restart.restart();
    }
    property Timer restart: Timer {
        // Only raster resizing needs debouncing. Cached facts are immediate.
        interval: root.completed && root.command.indexOf("page") >= 0 ? 80 : 0
        onTriggered: {
            if (!root.active || root.command.length === 0) return;
            const mode = root.command.findIndex(value => ["info", "page", "table"].includes(value));
            if (mode < 0) return;
            root.requestId = PreviewService.request(root.command.slice(mode));
        }
    }
    property Connections responses: Connections {
        target: PreviewService
        function onResponse(requestId, data) {
            if (requestId !== root.requestId || !root.active) return;
            root.requestId = "";
            root.pending = false;
            root.completed = true;
            root.response(data);
        }
    }
}
