import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root
    property bool active: false
    property bool started: false
    property date month: new Date()
    property bool ready: false
    property bool loading: false
    property bool available: false
    property string error: ""
    property var events: []
    property var cachedRanges: ({})
    readonly property string rangeKey: Math.floor(rangeStart.getTime() / 1000) + ":" + Math.floor(rangeEnd.getTime() / 1000)
    function rememberRange(entries) {
        const cache = Object.assign({}, cachedRanges);
        delete cache[rangeKey];
        cache[rangeKey] = entries;
        while (Object.keys(cache).length > 6) delete cache[Object.keys(cache)[0]];
        cachedRanges = cache;
    }
    function applyResponse(data) {
        if (data.since !== Math.floor(rangeStart.getTime() / 1000)) return;
        available = !!data.available;
        loading = !!data.loading;
        error = data.error || "";
        // Loading acknowledgements contain an empty working set, not a new agenda.
        if (!loading && !error) {
            events = data.events || [];
            rememberRange(events);
        }
    }
    readonly property date rangeStart: new Date(month.getFullYear(), month.getMonth(), -6)
    readonly property date rangeEnd: new Date(month.getFullYear(), month.getMonth() + 1, 8)
    function refresh() {
        if (!active || !ready) return;
        loading = true;
        if (cachedRanges[rangeKey]) events = cachedRanges[rangeKey];
        worker.write(JSON.stringify({since: Math.floor(rangeStart.getTime() / 1000), until: Math.floor(rangeEnd.getTime() / 1000)}) + "\n");
    }
    function forDay(day, sourceEvents) {
        const start = new Date(day.getFullYear(), day.getMonth(), day.getDate()).getTime() / 1000;
        const end = new Date(day.getFullYear(), day.getMonth(), day.getDate() + 1).getTime() / 1000;
        return (sourceEvents || events).filter(event => event.start < end && Math.max(event.end, event.start + 1) > start);
    }
    onMonthChanged: Qt.callLater(refresh)
    onActiveChanged: if (active) {
        started = true;
        error = "";
        if (ready) refresh();
        else loading = true;
    }
    Process {
        id: worker
        command: Quickshell.env("BINGUX_CALENDAR_HELPER") ? [Quickshell.env("BINGUX_CALENDAR_HELPER")]
            : ["python3", "-u", decodeURIComponent(Qt.resolvedUrl("calendar-events.py").toString().replace(/^file:\/\//, ""))]
        running: root.started
        stdinEnabled: true
        stdout: SplitParser {
            onRead: line => {
                let data;
                try { data = JSON.parse(line); } catch (_) { return; }
                if (data.ready) { root.ready = true; root.refresh(); return; }
                root.applyResponse(data);
            }
        }
        onExited: code => {
            root.ready = false;
            root.loading = false;
            if (root.active) root.error = "Calendar service unavailable";
            root.started = false;
        }
    }
}
