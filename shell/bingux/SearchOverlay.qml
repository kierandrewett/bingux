import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root

    readonly property int resultLimit: 20
    readonly property int maxChatExchanges: 6
    readonly property bool serviceReady: searchSocket.connectionState === "ready"
    readonly property bool loading: activeRequestId !== "" && !queryComplete
    readonly property string statusText: {
        if (queryError !== "")
            return queryError;

        if (!serviceReady)
            return "Search unavailable.";

        if (chatPending)
            return "Asking AI…";

        if (activationPending)
            return "Opening…";

        if (searchInput.text === "")
            return chatMode ? "Type a follow-up." : "Type to search.";

        if (loading)
            return "Searching…";

        if (queryComplete && results.length === 0)
            return "No matches.";

        return "";
    }
    property string activeRequestId: ""
    property string activeActivationRequestId: ""
    property var results: []
    property int selectedIndex: -1
    property bool queryComplete: false
    property bool activationPending: false
    property string queryError: ""
    property bool chatMode: false
    property bool chatPending: false
    property string pendingChatPrompt: ""
    property var chatTranscript: []
    property var displayedResults: []

    function clearResults() {
        results = [];
        displayedResults = [];
        selectedIndex = -1;
        queryComplete = false;
    }

    function clearChat() {
        chatMode = false;
        chatPending = false;
        pendingChatPrompt = "";
        chatTranscript = [];
    }

    function cancelPendingRequests() {
        if (activeRequestId !== "")
            searchSocket.cancel(activeRequestId);

        if (activeActivationRequestId !== "")
            searchSocket.cancel(activeActivationRequestId);

    }

    function showSearch() {
        if (!visible) {
            activeRequestId = "";
            activeActivationRequestId = "";
            activationPending = false;
            queryError = "";
            clearChat();
            clearResults();
            searchInput.text = "";
            visible = true;
        }
        searchInput.forceActiveFocus();
    }

    function closeSearch() {
        cancelPendingRequests();
        activeRequestId = "";
        activeActivationRequestId = "";
        activationPending = false;
        queryError = "";
        clearChat();
        clearResults();
        searchInput.text = "";
        visible = false;
    }

    function isChatResult(result) {
        return result !== null && typeof result === "object" && result.kind === "chat" && result.providerId === "ai";
    }

    function queryForSearch() {
        const trimmed = searchInput.text.trim();
        if (!chatMode || trimmed === "" || trimmed.charAt(0) === "?")
            return searchInput.text;

        return "?" + searchInput.text;
    }

    function chatPrompt() {
        let prompt = searchInput.text.trim();
        if (prompt.charAt(0) === "?")
            prompt = prompt.slice(1).trim();

        return prompt;
    }

    function submitQuery() {
        const query = queryForSearch();
        cancelPendingRequests();
        activeRequestId = "";
        activeActivationRequestId = "";
        activationPending = false;
        queryError = "";
        clearResults();
        if (!visible || searchInput.text === "")
            return ;

        if (!searchSocket.isValidQuery(query)) {
            queryError = "Search text is too long.";
            return ;
        }
        if (!serviceReady)
            return ;

        const requestId = searchSocket.sendQuery(query, resultLimit);
        if (requestId === "") {
            queryError = "Search unavailable.";
            return ;
        }
        activeRequestId = requestId;
    }

    function compareResults(left, right) {
        if (left.score !== right.score)
            return left.score > right.score ? -1 : 1;

        if (left.providerId !== right.providerId)
            return left.providerId < right.providerId ? -1 : 1;

        if (left.title !== right.title)
            return left.title < right.title ? -1 : 1;

        return left.resultId < right.resultId ? -1 : left.resultId > right.resultId ? 1 : 0;
    }

    function mergeResults(incoming) {
        const selectedResultId = selectedIndex >= 0 && selectedIndex < displayedResults.length ? displayedResults[selectedIndex].resultId : "";
        const updated = results.slice();
        for (let incomingIndex = 0; incomingIndex < incoming.length; incomingIndex += 1) {
            const result = incoming[incomingIndex];
            let existingIndex = -1;
            for (let resultIndex = 0; resultIndex < updated.length; resultIndex += 1) {
                if (updated[resultIndex].resultId === result.resultId) {
                    existingIndex = resultIndex;
                    break;
                }
            }
            if (existingIndex >= 0)
                updated[existingIndex] = result;
            else
                updated.push(result);
        }
        updated.sort(compareResults);
        if (updated.length > resultLimit)
            updated.splice(resultLimit);

        results = updated;
        displayedResults = updated;
        let retainedIndex = -1;
        let chatIndex = -1;
        for (let resultIndex = 0; resultIndex < displayedResults.length; resultIndex += 1) {
            if (displayedResults[resultIndex].resultId === selectedResultId)
                retainedIndex = resultIndex;

            if (isChatResult(displayedResults[resultIndex]))
                chatIndex = resultIndex;
        }
        if (queryForSearch().trim().charAt(0) === "?" && chatIndex >= 0)
            selectedIndex = chatIndex;
        else if (retainedIndex >= 0)
            selectedIndex = retainedIndex;
        else if (displayedResults.length > 0)
            selectedIndex = 0;
        else
            selectedIndex = -1;
    }

    function moveSelection(delta) {
        if (displayedResults.length === 0)
            return ;

        const baseIndex = selectedIndex < 0 ? 0 : selectedIndex;
        selectedIndex = (baseIndex + delta + displayedResults.length) % displayedResults.length;
        resultsList.positionViewAtIndex(selectedIndex, ListView.Contain);
    }

    function chatCandidate() {
        for (let resultIndex = 0; resultIndex < results.length; resultIndex += 1) {
            if (isChatResult(results[resultIndex]))
                return results[resultIndex];

        }
        return null;
    }

    function activateSelected() {
        if (selectedIndex < 0 || selectedIndex >= displayedResults.length || activationPending)
            return ;

        activateResult(displayedResults[selectedIndex]);
    }

    function activateChatResult(result) {
        const prompt = chatPrompt();
        if (prompt === "")
            return ;

        activeRequestId = "";
        queryComplete = true;
        queryError = "";
        const requestId = searchSocket.activate(result.resultId);
        if (requestId === "") {
            queryError = "Search unavailable.";
            return ;
        }
        chatMode = true;
        clearResults();
        activeActivationRequestId = requestId;
        pendingChatPrompt = prompt;
        activationPending = true;
        chatPending = true;
    }

    function activateResult(result) {
        if (activationPending || result === null || typeof result !== "object" || typeof result.resultId !== "string" || result.resultId === "")
            return ;

        if (isChatResult(result)) {
            activateChatResult(result);
            return ;
        }
        activeRequestId = "";
        queryComplete = true;
        queryError = "";
        const requestId = searchSocket.activate(result.resultId);
        if (requestId === "") {
            queryError = "Search unavailable.";
            return ;
        }
        activeActivationRequestId = requestId;
        activationPending = true;
    }

    function errorMessage(code) {
        if (code === "unavailable")
            return "Search unavailable.";

        if (code === "unknown-result")
            return "Result unavailable.";

        if (code === "invalid-request")
            return "Search failed.";

        if (code === "unsupported-protocol")
            return "Search needs an update.";

        return "Search failed.";
    }

    visible: false
    color: "transparent"
    exclusiveZone: 0
    focusable: visible
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "bingux-search"
    WlrLayershell.keyboardFocus: root.visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    onVisibleChanged: {
        if (visible)
            searchInput.forceActiveFocus();

    }

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }

    SearchSocket {
        id: searchSocket
    }

    Connections {
        function onShowSearch() {
            root.showSearch();
        }

        function onConnectionStateChanged() {
            if (searchSocket.connectionState !== "ready") {
                root.activeRequestId = "";
                root.activeActivationRequestId = "";
                root.activationPending = false;
                root.chatPending = false;
                root.pendingChatPrompt = "";
                root.queryError = "";
                root.clearResults();
                return ;
            }
            if (root.visible && searchInput.text !== "")
                root.submitQuery();

        }

        function onResultsReceived(requestId, incoming, complete) {
            if (requestId !== root.activeRequestId || root.queryComplete)
                return ;

            root.mergeResults(incoming);
            root.queryComplete = complete;
        }

        function onRequestFailed(requestId, code) {
            if (requestId === root.activeActivationRequestId) {
                root.activeActivationRequestId = "";
                root.activationPending = false;
                root.chatPending = false;
                root.pendingChatPrompt = "";
                root.queryError = root.errorMessage(code);
                return ;
            }
            if (requestId === root.activeRequestId) {
                if (code === "provider-failed") {
                    root.queryError = "Some search sources are unavailable.";
                    return ;
                }
                root.activeRequestId = "";
                root.clearResults();
                root.queryError = root.errorMessage(code);
            }
        }

        function onChatReceived(requestId, message) {
            if (!root.visible || !root.chatPending || requestId !== root.activeActivationRequestId)
                return ;

            const transcript = root.chatTranscript.slice();
            transcript.push({
                "prompt": root.pendingChatPrompt,
                "message": message
            });
            if (transcript.length > root.maxChatExchanges)
                transcript.splice(0, transcript.length - root.maxChatExchanges);

            root.chatTranscript = transcript;
            root.activeActivationRequestId = "";
            root.activationPending = false;
            root.chatPending = false;
            root.pendingChatPrompt = "";
            root.queryError = "";
            searchInput.text = "";
            searchInput.forceActiveFocus();
        }

        function onActivationCompleted(requestId) {
            if (requestId === root.activeActivationRequestId && !root.chatPending)
                root.closeSearch();

        }

        target: searchSocket
    }

    Rectangle {
        anchors.fill: parent
        color: "#8c000000"

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton
            onClicked: root.closeSearch()
        }

    }

    Rectangle {
        id: surface

        width: Math.max(0, Math.min(560, root.width - 32))
        height: content.implicitHeight + 24
        radius: 10
        color: "#202632"
        border.width: 1
        border.color: "#344158"

        anchors {
            top: parent.top
            topMargin: Math.max(48, Math.min(128, root.height / 6))
            horizontalCenter: parent.horizontalCenter
        }

        Column {
            id: content

            spacing: 8

            anchors {
                fill: parent
                margins: 12
            }

            Rectangle {
                id: searchFieldSurface

                width: parent.width
                height: 46
                radius: 6
                color: "#171a21"
                border.width: 1
                border.color: searchInput.activeFocus ? "#d9dee8" : "#344158"

                Text {
                    visible: searchInput.text === ""
                    color: "#8b94a3"
                    font.pixelSize: 14
                    text: root.chatMode ? "Ask a follow-up" : "Search Bingux or ? ask AI"
                    textFormat: Text.PlainText
                    elide: Text.ElideRight

                    anchors {
                        left: parent.left
                        right: parent.right
                        leftMargin: 14
                        rightMargin: 14
                        verticalCenter: parent.verticalCenter
                    }

                }

                TextInput {
                    id: searchInput

                    activeFocusOnTab: true
                    focus: root.visible
                    clip: true
                    color: "#f5f7fa"
                    font.pixelSize: 14
                    maximumLength: 512
                    readOnly: root.activationPending
                    selectByMouse: true
                    selectionColor: "#344158"
                    selectedTextColor: "#f5f7fa"
                    verticalAlignment: TextInput.AlignVCenter
                    Accessible.name: root.chatMode ? "Ask a follow-up" : "Search Bingux"
                    onTextChanged: root.submitQuery()
                    Keys.onPressed: function(event) {
                        if (event.key === Qt.Key_Escape) {
                            root.closeSearch();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Up) {
                            root.moveSelection(-1);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Down) {
                            root.moveSelection(1);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            if (root.selectedIndex >= 0 && root.selectedIndex < root.displayedResults.length) {
                                root.activateSelected();
                                event.accepted = true;
                            } else if (root.chatMode && root.queryComplete && searchInput.text.trim() !== "" && root.chatCandidate() !== null) {
                                root.activateResult(root.chatCandidate());
                                event.accepted = true;
                            } else if (root.queryError !== "" && searchInput.text !== "") {
                                root.submitQuery();
                                event.accepted = true;
                            }
                        }
                    }

                    anchors {
                        fill: parent
                        leftMargin: 14
                        rightMargin: 14
                    }

                }

            }

            ListView {
                id: chatTranscriptList

                width: parent.width
                height: visible ? Math.min(contentHeight, 192) : 0
                visible: root.chatMode && root.chatTranscript.length > 0
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                interactive: contentHeight > height
                model: root.chatTranscript

                delegate: Column {
                    id: chatExchange

                    required property var modelData

                    width: chatTranscriptList.width
                    spacing: 4

                    Text {
                        color: "#aeb8ca"
                        font.pixelSize: 12
                        text: "You"
                        textFormat: Text.PlainText
                    }

                    Rectangle {
                        width: parent.width
                        height: promptText.implicitHeight + 16
                        radius: 6
                        color: "#171a21"
                        border.width: 1
                        border.color: "#344158"

                        Text {
                            id: promptText

                            color: "#edf1f7"
                            elide: Text.ElideRight
                            font.pixelSize: 13
                            maximumLineCount: 3
                            text: chatExchange.modelData.prompt
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap

                            anchors {
                                left: parent.left
                                right: parent.right
                                top: parent.top
                                leftMargin: 10
                                rightMargin: 10
                                topMargin: 8
                            }

                        }

                    }

                    Text {
                        color: "#aeb8ca"
                        font.pixelSize: 12
                        text: "AI"
                        textFormat: Text.PlainText
                    }

                    Rectangle {
                        width: parent.width
                        height: responseText.implicitHeight + 16
                        radius: 6
                        color: "#202632"
                        border.width: 1
                        border.color: "#344158"

                        Text {
                            id: responseText

                            color: "#edf1f7"
                            font.pixelSize: 13
                            text: chatExchange.modelData.message
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap

                            anchors {
                                left: parent.left
                                right: parent.right
                                top: parent.top
                                leftMargin: 10
                                rightMargin: 10
                                topMargin: 8
                            }

                        }

                    }

                }

            }

            ListView {
                id: resultsList

                width: parent.width
                height: visible ? Math.min(contentHeight, 300) : 0
                visible: root.displayedResults.length > 0
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                currentIndex: root.selectedIndex
                interactive: contentHeight > height
                model: root.displayedResults

                delegate: Item {
                    id: resultRow

                    required property var modelData

                    width: resultsList.width
                    height: 54
                    Accessible.name: resultRow.modelData.title + (resultRow.modelData.subtitle === "" ? "" : ", " + resultRow.modelData.subtitle)
                    Accessible.role: Accessible.Button

                    Rectangle {
                        anchors.fill: parent
                        radius: 6
                        color: resultMouse.containsMouse || root.selectedIndex === index ? "#344158" : "transparent"
                    }

                    Column {
                        spacing: 2

                        anchors {
                            left: parent.left
                            right: parent.right
                            leftMargin: 10
                            rightMargin: 10
                            verticalCenter: parent.verticalCenter
                        }

                        Text {
                            width: parent.width
                            color: "#edf1f7"
                            elide: Text.ElideRight
                            font.pixelSize: 13
                            text: resultRow.modelData.title
                            textFormat: Text.PlainText
                        }

                        Text {
                            width: parent.width
                            visible: resultRow.modelData.subtitle !== ""
                            color: "#aeb8ca"
                            elide: Text.ElideRight
                            font.pixelSize: 12
                            text: resultRow.modelData.subtitle
                            textFormat: Text.PlainText
                        }

                    }

                    MouseArea {
                        id: resultMouse

                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton
                        enabled: !root.activationPending
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onEntered: {
                            root.selectedIndex = index;
                            resultsList.positionViewAtIndex(index, ListView.Contain);
                        }
                        onClicked: root.activateResult(resultRow.modelData)
                    }

                }

            }

            Text {
                width: parent.width
                visible: root.statusText !== ""
                color: root.queryError !== "" || !root.serviceReady ? "#f4a340" : "#8b94a3"
                font.pixelSize: 12
                text: root.statusText
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

        }

    }

}
