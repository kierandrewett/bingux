import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets

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

        if (loading && displayedResults.length === 0)
            return "Searching…";

        if (queryComplete && results.length === 0)
            return "No matches.";

        return "";
    }
    property string activeRequestId: ""
    property string activeActivationRequestId: ""
    property var results: []
    property int selectedIndex: -1
    property bool keyboardSelection: false
    property bool awaitingResults: false
    property bool queryComplete: false
    property bool activationPending: false
    readonly property bool launchCursorActive: visible && activationPending && !chatPending
    property string launchFeedbackToken: ""
    function endLaunchFeedback() {
        LaunchFeedback.end(launchFeedbackToken);
        launchFeedbackToken = "";
    }
    Component.onDestruction: endLaunchFeedback()
    Timer {
        interval: Theme.launchTimeout
        running: root.launchCursorActive
        onTriggered: {
            const requestId = root.activeActivationRequestId;
            root.activeActivationRequestId = "";
            root.activationPending = false;
            root.endLaunchFeedback();
            root.queryError = "Opening timed out. Try again.";
            if (requestId !== "")
                searchSocket.cancel(requestId);
        }
    }

    property string queryError: ""
    property bool chatMode: false
    property bool chatPending: false
    property string pendingChatPrompt: ""
    property var chatTranscript: []
    property var displayedResults: []

    function clearResults() {
        awaitingResults = false;
        keyboardSelection = false;
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

    function focusSearchInput() {
        if (!visible)
            return ;

        searchInput.forceActiveFocus();
        Qt.callLater(function() {
            if (root.visible)
                searchInput.forceActiveFocus();
        });
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
        focusSearchInput();
    }

    function closeSearch(keepLaunchFeedback = false) {
        if (keepLaunchFeedback)
            launchFeedbackToken = ""; // Gnoblin waits for the application window.
        else
            endLaunchFeedback();
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
        results = [];
        queryComplete = false;
        keyboardSelection = false;
        awaitingResults = true;
        selectedIndex = displayedResults.length > 0 ? 0 : -1;
        if (!visible || searchInput.text === "") {
            clearResults();
            return;
        }

        if (!searchSocket.isValidQuery(query)) {
            clearResults();
            queryError = query.trim() === "" ? "Enter search text." : "Search text is invalid.";
            return ;
        }
        if (!serviceReady) {
            clearResults();
            return;
        }

        const requestId = searchSocket.sendQuery(query, resultLimit);
        if (requestId === "") {
            clearResults();
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

    function mergeResults(incoming, complete = false) {
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
        const preferChat = queryForSearch().trim().charAt(0) === "?";
        updated.sort(function(left, right) {
            if (preferChat && isChatResult(left) !== isChatResult(right))
                return isChatResult(left) ? -1 : 1;
            return compareResults(left, right);
        });
        if (updated.length > resultLimit)
            updated.splice(resultLimit);

        results = updated;
        // Empty provider batches must not erase the previous query's rows.
        if (updated.length === 0 && !complete)
            return;
        awaitingResults = false;
        displayedResults = updated;
        const retainedIndex = keyboardSelection
            ? displayedResults.findIndex(result => result.resultId === selectedResultId)
            : -1;
        selectedIndex = retainedIndex >= 0 ? retainedIndex : displayedResults.length > 0 ? 0 : -1;
        if (!keyboardSelection)
            resultsList.positionViewAtBeginning();
    }

    function moveSelection(delta) {
        if (displayedResults.length === 0)
            return ;

        keyboardSelection = true;
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
        if (awaitingResults || activationPending || result === null || typeof result !== "object" || typeof result.resultId !== "string" || result.resultId === "")
            return ;

        if (isChatResult(result)) {
            activateChatResult(result);
            return ;
        }
        activeRequestId = "";
        queryComplete = true;
        queryError = "";
        endLaunchFeedback();
        activationPending = true;
        const token = LaunchFeedback.begin(result.title || "", () => {
            if (!root.activationPending || root.launchFeedbackToken !== token)
                return;
            const requestId = searchSocket.activate(result.resultId);
            if (requestId === "") {
                root.activationPending = false;
                root.endLaunchFeedback();
                root.queryError = "Search unavailable.";
                return;
            }
            root.activeActivationRequestId = requestId;
        });
        launchFeedbackToken = token;
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
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "bingux-search"
    WlrLayershell.keyboardFocus: root.visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    onVisibleChanged: {
        if (visible)
            focusSearchInput();
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
                root.endLaunchFeedback();
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

            root.mergeResults(incoming, complete);
            root.queryComplete = complete;
        }

        function onRequestFailed(requestId, code) {
            if (requestId === root.activeActivationRequestId) {
                root.endLaunchFeedback();
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
                root.closeSearch(true);

        }

        target: searchSocket
    }

    Rectangle {
        anchors.fill: parent
        color: "transparent"

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton
            onClicked: root.closeSearch()
        }

    }

    Rectangle {
        id: surface
        readonly property int contentPadding: Theme.padding
        readonly property real contentRadius: Theme.insetRadius(radius, contentPadding)

        width: Math.max(0, Math.min(680, root.width - 32))
        height: content.implicitHeight + surface.contentPadding * 2
        radius: Theme.cardRadius
        color: Theme.surface
        border.width: 1
        border.color: Theme.outline

        anchors {
            top: parent.top
            topMargin: Math.max(48, Math.min(128, root.height / 6))
            horizontalCenter: parent.horizontalCenter
        }

        ColumnLayout {
            id: content

            spacing: 8

            anchors {
                fill: parent
                margins: surface.contentPadding
            }

            Rectangle {
                id: searchFieldSurface

                Layout.fillWidth: true
                Layout.preferredHeight: 52
                radius: surface.contentRadius
                color: "transparent"

                SymbolicIcon {
                    id: searchFieldIcon
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.gap
                    anchors.verticalCenter: parent.verticalCenter
                    implicitSize: 20
                    source: Quickshell.iconPath("system-search-symbolic")
                }

                Text {
                    visible: searchInput.text === ""
                    color: Theme.muted
                    font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                    text: root.chatMode ? "Ask a follow-up" : "Search applications, files and more"
                    textFormat: Text.PlainText
                    elide: Text.ElideRight

                    anchors {
                        left: parent.left
                        right: parent.right
                        leftMargin: Theme.gap + 20 + Theme.padding
                        rightMargin: Theme.gap
                        verticalCenter: parent.verticalCenter
                    }

                }

                TextInput {
                    id: searchInput
                    HoverHandler { cursorShape: Qt.ArrowCursor }

                    activeFocusOnTab: true
                    focus: root.visible
                    clip: true
                    color: Theme.text
                    font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                    maximumLength: 512
                    readOnly: root.activationPending
                    selectByMouse: true
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.text
                    verticalAlignment: TextInput.AlignVCenter
                    Accessible.name: root.chatMode ? "Ask a follow-up" : "Search"
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
                        leftMargin: Theme.gap + 20 + Theme.padding
                        rightMargin: Theme.gap
                    }

                }

            }

            ListView {
                id: chatTranscriptList

                Layout.fillWidth: true
                Layout.preferredHeight: visible ? Math.min(contentHeight, 192) : 0
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
                        color: Theme.muted
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        text: "You"
                        textFormat: Text.PlainText
                    }

                    Rectangle {
                        width: parent.width
                        height: promptText.implicitHeight + 16
                        radius: surface.contentRadius
                        color: Theme.background
                        border.width: 1
                        border.color: Theme.outline

                        Text {
                            id: promptText

                            color: Theme.text
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
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
                        color: Theme.muted
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        text: "AI"
                        textFormat: Text.PlainText
                    }

                    Rectangle {
                        width: parent.width
                        height: responseText.implicitHeight + 16
                        radius: surface.contentRadius
                        color: Theme.surface
                        border.width: 1
                        border.color: Theme.outline

                        Text {
                            id: responseText

                            color: Theme.text
                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
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

                Layout.fillWidth: true
                Layout.preferredHeight: visible ? Math.min(contentHeight, 360) : 0
                visible: root.displayedResults.length > 0
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                currentIndex: root.selectedIndex
                interactive: contentHeight > height
                model: root.displayedResults

                delegate: Item {
                    id: resultRow

                    required property int index
                    required property var modelData

                    width: resultsList.width
                    height: 54
                    Accessible.name: resultRow.modelData.title + (resultRow.modelData.subtitle === "" ? "" : ", " + resultRow.modelData.subtitle)
                    Accessible.role: Accessible.Button

                    Rectangle {
                        anchors.fill: parent
                        radius: surface.contentRadius
                        color: resultMouse.pressed ? Theme.pressed : root.selectedIndex === resultRow.index ? Theme.selection : resultMouse.containsMouse ? Theme.hover : "transparent"
                    }

                    IconImage {
                        id: resultIcon
                        anchors.left: parent.left
                        anchors.leftMargin: Theme.padding
                        anchors.verticalCenter: parent.verticalCenter
                        implicitSize: 32
                        source: Quickshell.iconPath(resultRow.modelData.icon || "system-search-symbolic", "application-x-executable")
                    }
                    ColumnLayout {
                        spacing: Theme.spaceSmall

                        anchors {
                            left: parent.left
                            right: parent.right
                            leftMargin: Theme.padding * 2 + 32
                            rightMargin: Theme.padding
                            verticalCenter: parent.verticalCenter
                        }

                        Text {
                            Layout.fillWidth: true
                            color: Theme.text
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                            text: resultRow.modelData.title
                            textFormat: Text.PlainText
                        }

                        Text {
                            width: parent.width
                            visible: resultRow.modelData.subtitle !== ""
                            color: Theme.muted
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
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
                        cursorShape: Qt.ArrowCursor
                        onClicked: root.activateResult(resultRow.modelData)
                    }

                }

            }

            Text {
                Layout.fillWidth: true
                visible: root.statusText !== ""
                color: root.queryError !== "" || !root.serviceReady ? "#f4a340" : Theme.muted
                font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                text: root.statusText
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

        }

    }

}
