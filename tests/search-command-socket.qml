import QtQuick

QtObject {
    property string connectionState: "ready"
    property string integrationState: "ready"
    property int sequence: 0
    property var queries: []
    property var activations: []
    signal showSearch
    signal resultsReceived(string requestId, var results, bool complete)
    signal requestFailed(string requestId, string code)
    signal activationCompleted(string requestId)
    signal chatProgress(string requestId, string message)
    signal chatReceived(string requestId, string message)
    function resetChat() {
    }
    function activate(resultId) {
        activations = activations.concat([resultId]);
        return "activation-" + (++sequence);
    }
    function cancel(requestId) {
    }
    function isValidQuery(query) {
        return true;
    }
    function sendQuery(query, limit) {
        queries = queries.concat([query]);
        return "query-" + (++sequence);
    }
}
