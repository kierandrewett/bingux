import QtQuick

QtObject {
    property string connectionState: "ready"
    property string integrationState: "ready"
    property int sequence: 0
    property var cancelled: []
    signal showSearch
    signal resultsReceived(string requestId, var results, bool complete)
    signal requestFailed(string requestId, string code)
    signal activationCompleted(string requestId)
    signal chatProgress(string requestId, string message)
    signal chatReceived(string requestId, string message)
    function resetChat() {
    }
    function activate(resultId) {
        return "activation-" + (++sequence);
    }
    function cancel(requestId) {
        cancelled = cancelled.concat([requestId]);
    }
    function isValidQuery(query) {
        return true;
    }
    function sendQuery(query, limit) {
        return "query-" + (++sequence);
    }
}
