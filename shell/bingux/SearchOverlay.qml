import QtQuick
import QtQuick.Window
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets
import "SearchGroups.js" as SearchGroups

PanelWindow {
    id: root

    // Keep the compositor buffer until the actual layer and input field have focus.
    readonly property bool acceptingKeyboard: visible && !closing && searchInput.activeFocus && searchInput.Window.active
    onAcceptingKeyboardChanged: if (acceptingKeyboard)
        inputHandoff.send({
            op: "shortcut-input",
            name: "search",
            state: "ready"
        })
    ShortcutSession {
        id: inputHandoff
        onReadyChanged: if (ready) {
            if (!root.visible)
                send({
                    op: "shortcut-input",
                    name: "search",
                    state: "closed"
                });
            if (root.acceptingKeyboard)
                send({
                    op: "shortcut-input",
                    name: "search",
                    state: "ready"
                });
        }
    }

    property var dockView: null
    function openAppMenu(result, position) {
        if (!dockView || activationPending || awaitingResults || closing || !result.desktopId || result.providerId !== "applications" || result.kind !== "application")
            return;
        const entry = dockView.desktopEntryFor(dockView.normaliseAppId(result.desktopId));
        if (!entry)
            return;
        appMenu.result = result;
        appMenu.group = {
            id: entry.id,
            desktopEntry: entry,
            windows: []
        };
        appMenu.preferredX = position.x;
        appMenu.preferredY = position.y;
        appMenu.visible = true;
        Qt.callLater(appMenuNavigation.focusMenu);
    }
    ShellPopup {
        id: appMenu
        objectName: "searchAppMenu"
        hostItem: root.contentItem
        screen: root.screen
        property var result: null
        property var group: null
        popupWidth: 220
        contentPadding: Theme.gap
        popupHeight: appMenuColumn.implicitHeight + contentPadding * 2
        onVisibleChanged: if (!visible && root.visible && !root.closing)
            root.focusSearchInput()
        MenuNavigator {
            id: appMenuNavigation
            entries: appMenuColumn.children
            focusTarget: appMenuColumn
            onEscapeRequested: appMenu.visible = false
            onActivateRequested: entry => entry.clicked()
        }
        ColumnLayout {
            id: appMenuColumn
            width: parent.width
            spacing: 0
            Keys.forwardTo: [appMenuNavigation]
            ActionButton {
                readonly property bool menuEntry: true
                Layout.fillWidth: true
                flat: true
                alignLeft: true
                text: "Open"
                cornerRadius: appMenu.contentRadius
                Keys.forwardTo: [appMenuNavigation]
                onClicked: {
                    const result = appMenu.result;
                    appMenu.visible = false;
                    root.activateResult(result);
                }
            }
            ActionButton {
                objectName: "searchAppPinAction"
                readonly property bool menuEntry: true
                Layout.fillWidth: true
                flat: true
                alignLeft: true
                text: root.dockView && appMenu.group && root.dockView.isPinned(appMenu.group) ? "Unpin from dock" : "Pin to dock"
                cornerRadius: appMenu.contentRadius
                Keys.forwardTo: [appMenuNavigation]
                onClicked: {
                    const group = appMenu.group;
                    appMenu.visible = false;
                    root.dockView.setPinned(group, !root.dockView.isPinned(group));
                }
            }
        }
    }

    readonly property int resultLimit: 20
    readonly property int maxChatExchanges: 6
    readonly property bool serviceReady: searchSocket.connectionState === "ready"
    readonly property bool loading: activeRequestId !== "" && !queryComplete
    readonly property bool hasQuery: searchInput.text.trim() !== ""
    readonly property string inlineStatusText: {
        if (queryError !== "")
            return queryError;

        if (!serviceReady)
            return "Offline";

        if (chatPending)
            return streamingChatText ? "Answering" : "Connecting";

        if (activationPending)
            return "Opening";

        if (loading && displayedResults.length === 0)
            return "Searching";

        if (queryComplete && results.length === 0 && hasQuery)
            return "No results";

        return "";
    }
    property string activeRequestId: ""
    property string activeActivationRequestId: ""
    property string activatedWebTitle: ""
    property string webFocusOnClose: ""
    property var results: []
    property int selectedIndex: -1
    // Lives above replaceable result delegates: typing may rebuild the model,
    // but only visiting a different index earns another chevron entrance.
    property int lastChevronIndex: -1
    function claimChevronAnimation(index) {
        if (index < 0 || index === lastChevronIndex)
            return false;
        lastChevronIndex = index;
        return true;
    }
    property bool keyboardSelection: false
    property bool awaitingResults: false
    property bool queryComplete: false
    property bool activationPending: false
    readonly property bool launchCursorActive: visible && activationPending && !chatPending && !closing
    property string launchFeedbackToken: ""
    property string launchDesktopId: ""
    function endDockLaunch() {
        if (launchDesktopId !== "" && dockView && typeof dockView.endExternalLaunch === "function")
            dockView.endExternalLaunch(launchDesktopId);
        launchDesktopId = "";
    }
    function endLaunchFeedback() {
        LaunchFeedback.end(launchFeedbackToken);
        launchFeedbackToken = "";
    }
    Component.onDestruction: {
        endDockLaunch();
        endLaunchFeedback();
    }
    Timer {
        interval: Theme.launchTimeout
        running: root.launchCursorActive
        onTriggered: {
            const requestId = root.activeActivationRequestId;
            root.activeActivationRequestId = "";
            root.activationPending = false;
            root.endDockLaunch();
            root.endLaunchFeedback();
            root.queryError = "Opening timed out. Try again.";
            if (requestId !== "")
                searchSocket.cancel(requestId);
        }
    }

    property string queryError: ""
    // Keep quick chat out of search until the feature is enabled again.
    property bool quickChatEnabled: false
    property bool chatMode: false
    property bool chatPending: false
    property string pendingChatPrompt: ""
    signal settingsRequested
    property string streamingChatText: ""
    property var chatTranscript: []
    property var displayedResults: []
    property bool closing: false
    property bool previewOpen: false
    readonly property var previewResult: {
        if (!visible || selectedIndex < 0 || selectedIndex >= displayedResults.length)
            return null;
        const result = displayedResults[selectedIndex];
        return root.canPreview(result) ? result : null;
    }
    function canPreview(result) {
        if (!BinguxPreferences.data.previews.enabled)
            return false;
        if (result.providerId !== "files" || result.kind !== "file" || !String(result.subtitle || "").startsWith("/"))
            return false;
        return /\.(zip|tar|tgz|gz|bz2|xz|epub|eml|tsv|docx?|odt|rtf|pptx?|odp|xlsx?|ods|db|db3|sqlite3?|mp4|mkv|webm|mov|avi|m4v|mpe?g|ogv|mp3|flac|ogg|wav|m4a|opus|markdown|pdf|png|jpe?g|gif|webp|bmp|tiff?|svg|avif|heic|txt|md|log|csv|json|ya?ml|toml|ini|conf|xml|html?|css|js|ts|py|rs|c|h|sh|nix|qml|jsx|tsx|cpp|hpp|go|rb|java|kt|swift|bash|zsh|scss|sql)$/i.test(result.subtitle);
    }
    readonly property var previewCandidates: {
        if (!visible || closing)
            return [];
        const ordered = displayedResults.map((result, index) => ({
                    result: result,
                    distance: Math.abs(index - Math.max(0, selectedIndex))
                }));
        ordered.sort((a, b) => a.distance - b.distance);
        return ordered.filter(entry => root.canPreview(entry.result)).slice(0, 6).map(entry => entry.result.subtitle);
    }
    onPreviewCandidatesChanged: preloadDelay.restart()
    Timer {
        id: preloadDelay
        interval: 160
        onTriggered: PreviewService.preload(root.previewCandidates)
    }

    readonly property bool pointerBlocked: !visible || closing || appMenu.visible || openAnimation.running || surfaceHeightAnimation.running || surfaceYAnimation.running || keyboardScroll.running
    property int pointerResultIndex: -1
    onPointerBlockedChanged: {
        pointerResultIndex = -1;
        resultPointer.lastPosition = resultPointer.point.position;
    }
    onDisplayedResultsChanged: {
        pointerResultIndex = -1;
        appMenu.visible = false;
    }
    // Profile-owned components implement the SearchResult interface.
    property var providerDelegates: ({})

    function clearResults() {
        awaitingResults = false;
        keyboardSelection = false;
        results = [];
        displayedResults = [];
        selectedIndex = -1;
        queryComplete = false;
    }

    function clearChat() {
        const hadChat = chatMode;
        chatMode = false;
        chatPending = false;
        pendingChatPrompt = "";
        chatTranscript = [];
        streamingChatText = "";
        if (hadChat)
            searchSocket.resetChat();
    }

    function cancelPendingRequests() {
        if (activeRequestId !== "")
            searchSocket.cancel(activeRequestId);

        if (activeActivationRequestId !== "")
            searchSocket.cancel(activeActivationRequestId);
    }

    function focusSearchInput() {
        if (!visible)
            return;

        searchInput.forceActiveFocus();
        Qt.callLater(function () {
            if (root.visible)
                searchInput.forceActiveFocus();
        });
    }

    property bool chromeRevealed: false
    function toggleSearch() {
        if (visible && !closing)
            closeSearch(false, true);
        else
            showSearch();
    }

    function showSearch() {
        PopupTransitions.refresh("bingux-search");
        chromeRevealed = true;
        browserFocus.cancel();
        webFocusOnClose = "";
        launchEffect.cancel();
        if (closing) {
            closing = false;
            closeAnimation.stop();
            openAnimation.restart();
        }
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
        Qt.callLater(() => {
            if (root.acceptingKeyboard)
                inputHandoff.send({
                    op: "shortcut-input",
                    name: "search",
                    state: "ready"
                });
        });
    }

    function closeSearch(keepLaunchFeedback = false, keepChrome = false) {
        if (!keepChrome)
            chromeRevealed = false;
        inputHandoff.send({
            op: "shortcut-input",
            name: "search",
            state: "closed"
        });
        if (!visible || closing)
            return;

        if (keepLaunchFeedback)
            launchFeedbackToken = "";
        else
        // Gnoblin waits for the application window.
        {
            endDockLaunch();
            endLaunchFeedback();
        }
        cancelPendingRequests();
        activeRequestId = "";
        activeActivationRequestId = "";
        activationPending = false;
        queryError = "";
        closing = true;
        appMenu.visible = false;
        openAnimation.stop();
        if (Theme.searchMotion === 0 || (!launchEffect.running && PopupTransitions.matches(Theme.searchExitMotion, Easing.OutCubic, "bingux-search")))
            finishClose();
        else
            closeAnimation.restart();
    }

    function finishClose() {
        if (!closing || launchEffect.running)
            return;

        visible = false;
        closing = false;
        clearChat();
        clearResults();
        searchInput.text = "";
        const webTitle = webFocusOnClose;
        webFocusOnClose = "";
        // Unmap the search layer before raising the browser; otherwise the
        // compositor restores the previous window when the layer disappears.
        if (webTitle)
            Qt.callLater(() => {
                if (!root.visible)
                    browserFocus.request(webTitle);
            });
    }

    function isChatResult(result) {
        return result !== null && typeof result === "object" && result.kind === "chat" && result.providerId === "ai";
    }

    function queryForSearch() {
        const trimmed = searchInput.text.trim();
        if (!chatMode || trimmed === "" || (trimmed.charAt(0) === "!" || trimmed.charAt(0) === "?"))
            return searchInput.text;

        return "!" + searchInput.text;
    }

    function chatPrompt() {
        let prompt = searchInput.text.trim();
        if (prompt.charAt(0) === "!" || prompt.charAt(0) === "?")
            prompt = prompt.slice(1).trim();

        return prompt;
    }

    function submitQuery() {
        appMenu.visible = false;
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
        if (!visible || closing || searchInput.text === "") {
            clearResults();
            return;
        }

        if (!searchSocket.isValidQuery(query)) {
            clearResults();
            queryError = query.trim() === "" ? "Enter search text." : "Search text is invalid.";
            return;
        }
        if (!serviceReady) {
            clearResults();
            return;
        }

        const requestId = searchSocket.sendQuery(query, resultLimit);
        if (requestId === "") {
            clearResults();
            queryError = "Search unavailable.";
            return;
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
            if (!quickChatEnabled && isChatResult(result))
                continue;
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
        updated.sort(function (left, right) {
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
        displayedResults = SearchGroups.ordered(updated);
        const retainedIndex = keyboardSelection ? displayedResults.findIndex(result => result.resultId === selectedResultId) : -1;
        selectedIndex = retainedIndex >= 0 ? retainedIndex : displayedResults.length > 0 ? 0 : -1;
        if (!keyboardSelection)
            resultsList.positionViewAtBeginning();
    }

    function moveSelection(delta) {
        if (displayedResults.length === 0)
            return;

        keyboardSelection = true;
        const baseIndex = selectedIndex < 0 ? 0 : selectedIndex;
        selectedIndex = (baseIndex + delta + displayedResults.length) % displayedResults.length;
        resultsList.revealSelection();
    }

    function inputHint(query, title) {
        if (!query || !title)
            return "";
        if (title.toLowerCase().startsWith(query.toLowerCase()))
            return title.slice(query.length);
        return "  ·  " + title;
    }

    function completeSelectedName() {
        if (awaitingResults || activationPending || selectedIndex < 0 || selectedIndex >= displayedResults.length)
            return;
        const title = String(displayedResults[selectedIndex].title || "");
        if (!title)
            return;
        searchInput.text = title;
        searchInput.cursorPosition = searchInput.length;
        searchInput.forceActiveFocus();
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
            return;

        activateResult(displayedResults[selectedIndex]);
    }

    function activateChatResult(result) {
        if (!quickChatEnabled)
            return;
        const prompt = chatPrompt();
        if (prompt === "")
            return;

        activeRequestId = "";
        queryComplete = true;
        queryError = "";
        const requestId = searchSocket.activate(result.resultId);
        if (requestId === "") {
            queryError = "Search unavailable.";
            return;
        }
        chatMode = true;
        clearResults();
        activeActivationRequestId = requestId;
        pendingChatPrompt = prompt;
        activationPending = true;
        chatPending = true;
        streamingChatText = "";
        chatTranscript = chatTranscript.concat([
            {
                prompt: prompt,
                message: "",
                pending: true
            }
        ]).slice(-maxChatExchanges);
    }

    function activateResult(result, position) {
        if (awaitingResults || activationPending || result === null || typeof result !== "object" || typeof result.resultId !== "string" || result.resultId === "")
            return;

        if (isChatResult(result)) {
            activateChatResult(result);
            return;
        }
        activeRequestId = "";
        queryComplete = true;
        queryError = "";
        animateActivation(result, position);
        endDockLaunch();
        endLaunchFeedback();
        activatedWebTitle = result.providerId === "web" || result.providerId === "web-suggestions" ? result.title : "";
        activationPending = true;
        if (result.providerId === "applications" && result.kind === "application" && result.desktopId && dockView && typeof dockView.beginExternalLaunch === "function" && dockView.beginExternalLaunch(result.desktopId, result.title || ""))
            launchDesktopId = result.desktopId;
        const token = LaunchFeedback.begin(result.title || "", () => {
            if (!root.activationPending || root.launchFeedbackToken !== token)
                return;
            const requestId = searchSocket.activate(result.resultId);
            if (requestId === "") {
                root.activationPending = false;
                root.endDockLaunch();
                root.endLaunchFeedback();
                root.queryError = "Search unavailable.";
                return;
            }
            root.activeActivationRequestId = requestId;
        });
        launchFeedbackToken = token;
    }

    function animateActivation(result, position) {
        const index = displayedResults.findIndex(entry => entry.resultId === result.resultId);
        const delegate = resultsList.itemAtIndex(index);
        const item = delegate ? delegate.resultItem : null;
        if (item && item.activationIcon)
            launchEffect.play(item.activationIcon, item.activationIconSource, item.symbolic, position);
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
    WlrLayershell.keyboardFocus: root.visible && !root.closing ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    onVisibleChanged: {
        if (visible) {
            lastChevronIndex = -1;
            closing = false;
            openAnimation.restart();
            focusSearchInput();
        } else {
            appMenu.visible = false;
            previewOpen = false;
            openAnimation.stop();
            closeAnimation.stop();
        }
    }

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }

    SearchSocket {
        id: searchSocket
        objectName: "searchSocket"
    }

    IpcHandler {
        target: "search"
        function toggle(): string {
            root.toggleSearch();
            return JSON.stringify({
                ok: true,
                open: root.visible && !root.closing
            });
        }
        function open(): string {
            root.showSearch();
            return JSON.stringify({
                ok: true,
                open: true
            });
        }
        function close(): string {
            root.closeSearch();
            return JSON.stringify({
                ok: true,
                open: false
            });
        }
        function query(text: string): void {
            root.showSearch();
            searchInput.text = text;
        }
        function move(delta: int): void {
            root.moveSelection(delta);
        }
        function status(): string {
            return JSON.stringify({
                open: root.visible && !root.closing,
                visible: root.visible,
                acceptingKeyboard: root.acceptingKeyboard,
                query: searchInput.text
            });
        }
    }

    Connections {
        function onConnectionStateChanged() {
            if (searchSocket.connectionState !== "ready") {
                root.endDockLaunch();
                root.endLaunchFeedback();
                root.activeRequestId = "";
                root.activeActivationRequestId = "";
                root.activationPending = false;
                root.chatPending = false;
                root.chatTranscript = root.chatTranscript.map(entry => entry.pending ? {
                        prompt: entry.prompt,
                        message: root.streamingChatText || "No answer received."
                    } : entry);
                root.pendingChatPrompt = "";
                root.queryError = "";
                root.clearResults();
                return;
            }
            if (root.visible && searchInput.text !== "")
                root.submitQuery();
        }

        function onResultsReceived(requestId, incoming, complete) {
            if (requestId !== root.activeRequestId || root.queryComplete)
                return;

            root.mergeResults(incoming, complete);
            root.queryComplete = complete;
        }

        function onRequestFailed(requestId, code) {
            if (requestId === root.activeActivationRequestId) {
                const failedChat = root.chatPending;
                root.endDockLaunch();
                root.endLaunchFeedback();
                root.activeActivationRequestId = "";
                root.activationPending = false;
                root.chatPending = false;
                root.chatTranscript = root.chatTranscript.map(entry => entry.pending ? {
                        prompt: entry.prompt,
                        message: (root.streamingChatText ? root.streamingChatText + "\n\n" : "") + "The AI request failed. Check your CLI login and model in Bingux Settings, then try again."
                    } : entry);
                root.pendingChatPrompt = "";
                root.queryError = failedChat ? "AI unavailable" : root.errorMessage(code);
                return;
            }
            if (requestId === root.activeRequestId) {
                if (code === "provider-failed") {
                    root.queryError = "Some search sources are unavailable.";
                    return;
                }
                root.activeRequestId = "";
                root.clearResults();
                root.queryError = root.errorMessage(code);
            }
        }

        function onChatProgress(requestId, message) {
            if (root.visible && root.chatPending && requestId === root.activeActivationRequestId)
                root.streamingChatText = message;
        }

        function onChatReceived(requestId, message) {
            if (!root.visible || !root.chatPending || requestId !== root.activeActivationRequestId)
                return;

            const transcript = root.chatTranscript.filter(entry => !entry.pending);
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
            if (requestId === root.activeActivationRequestId && !root.chatPending) {
                root.webFocusOnClose = root.activatedWebTitle;
                root.activatedWebTitle = "";
                root.closeSearch(true);
            }
        }

        target: searchSocket
    }

    BrowserFocus {
        id: browserFocus
    }

    ParallelAnimation {
        id: openAnimation
        NumberAnimation {
            target: surface
            property: "opacity"
            from: 1
            to: 1
            duration: 0
        }
        NumberAnimation {
            target: surface
            property: "scale"
            from: 1
            to: 1
            duration: 0
            easing.type: Easing.OutCubic
        }
    }

    ParallelAnimation {
        id: closeAnimation
        NumberAnimation {
            target: surface
            property: "opacity"
            to: 0
            duration: Theme.searchExitMotion
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            target: surface
            property: "scale"
            to: 0.99
            duration: Theme.searchExitMotion
            easing.type: Easing.OutCubic
        }
        onFinished: root.finishClose()
    }

    SearchLaunchEffect {
        id: launchEffect
        onFinished: if (root.closing && !closeAnimation.running)
            root.finishClose()
    }

    Rectangle {
        anchors.fill: parent
        color: "transparent"

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton
            onClicked: if (!root.pointerBlocked)
                root.closeSearch()
        }
    }

    Rectangle {
        id: surface
        objectName: "searchSurface"
        z: 1
        readonly property int contentPadding: 6
        readonly property real contentRadius: Theme.insetRadius(radius, contentPadding)

        width: Math.max(0, Math.min(660, root.width - 32))
        height: content.implicitHeight + surface.contentPadding * 2
        radius: Theme.searchCardRadius
        color: Theme.searchSurface
        border.width: 1
        border.color: "#555960"
        PanelOutline {
            surface: surface
        }
        clip: true
        y: Math.max(16, Math.min(root.height * 0.38 - (Theme.searchInputHeight + 16) / 2, root.height - height - 24))
        Behavior on height {
            enabled: root.acceptingKeyboard && !root.closing
            NumberAnimation {
                id: surfaceHeightAnimation
                duration: Theme.searchMotion
                easing.type: Easing.OutCubic
            }
        }
        Behavior on y {
            enabled: root.acceptingKeyboard && !root.closing
            NumberAnimation {
                id: surfaceYAnimation
                duration: Theme.searchMotion
                easing.type: Easing.OutCubic
            }
        }

        anchors {
            horizontalCenter: parent.horizontalCenter
        }

        // Absorb clicks on inactive results instead of dismissing the popup.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onWheel: wheel => {
                wheel.accepted = true;
            }
        }

        ColumnLayout {
            id: content
            width: surface.width - surface.contentPadding * 2

            spacing: 4

            anchors {
                left: parent.left
                top: parent.top
                margins: surface.contentPadding
            }

            Rectangle {
                id: searchFieldSurface

                Layout.fillWidth: true
                Layout.preferredHeight: Theme.searchInputHeight
                radius: surface.contentRadius
                color: "transparent"

                SymbolicIcon {
                    id: searchFieldIcon
                    anchors.left: parent.left
                    anchors.leftMargin: 18
                    anchors.verticalCenter: parent.verticalCenter
                    implicitSize: 20
                    color: Theme.muted
                    source: Quickshell.iconPath("system-search-symbolic")
                }

                Text {
                    visible: searchInput.text === ""
                    color: Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.searchInputFontSize
                    text: root.chatMode ? "Ask a follow-up…" : "Search apps, files, web…"
                    textFormat: Text.PlainText
                    elide: Text.ElideRight

                    anchors {
                        left: parent.left
                        right: parent.right
                        leftMargin: 56
                        rightMargin: inlineStatus.width + (clearQueryButton.visible ? 52 : 28)
                        verticalCenter: parent.verticalCenter
                    }
                }

                TextInput {
                    id: searchInput
                    objectName: "searchInput"
                    HoverHandler {
                        cursorShape: Qt.IBeamCursor
                    }

                    activeFocusOnTab: true
                    focus: root.visible
                    clip: true
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.searchInputFontSize
                    maximumLength: 512
                    readOnly: root.activationPending || root.closing
                    selectByMouse: true
                    selectionColor: Theme.textSelection
                    selectedTextColor: Theme.text
                    verticalAlignment: TextInput.AlignVCenter
                    Accessible.name: root.chatMode ? "Ask a follow-up" : "Search"
                    onTextChanged: root.submitQuery()
                    Keys.onPressed: function (event) {
                        if (event.key === Qt.Key_Menu || (event.key === Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier))) {
                            const row = resultsList.itemAtIndex(root.selectedIndex);
                            if (row)
                                root.openAppMenu(root.displayedResults[root.selectedIndex], row.mapToItem(root.contentItem, 24, row.height));
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Escape) {
                            root.closeSearch();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Left && root.previewOpen) {
                            root.previewOpen = false;
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Right && root.previewResult && searchInput.cursorPosition === searchInput.length) {
                            root.previewOpen = true;
                            event.accepted = true;
                        } else if (event.key === Qt.Key_L && (event.modifiers & Qt.ControlModifier)) {
                            searchInput.selectAll();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Tab && !(event.modifiers & Qt.ShiftModifier)) {
                            root.completeSelectedName();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Backtab || (event.key === Qt.Key_Tab && (event.modifiers & Qt.ShiftModifier))) {
                            root.moveSelection(-1);
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
                        leftMargin: 56
                        rightMargin: inlineStatus.width + (clearQueryButton.visible ? 52 : 28)
                    }
                }

                Text {
                    id: selectionHint
                    objectName: "searchSelectionHint"
                    readonly property string title: root.selectedIndex >= 0 && root.selectedIndex < root.displayedResults.length ? String(root.displayedResults[root.selectedIndex].title || "") : ""
                    text: root.inputHint(searchInput.text, title)
                    textFormat: Text.PlainText
                    font: searchInput.font
                    color: Theme.text
                    opacity: 0.35
                    x: searchInput.x + searchInput.contentWidth
                    anchors.baseline: searchInput.baseline
                    width: Math.max(0, searchInput.x + searchInput.width - x)
                    elide: Text.ElideRight
                    clip: true
                    visible: text !== "" && !root.awaitingResults && !root.closing && searchInput.cursorPosition === searchInput.length && searchInput.selectedText === "" && searchInput.preeditText === "" && searchInput.contentWidth < searchInput.width
                    Accessible.ignored: true
                }

                Text {
                    id: inlineStatus
                    anchors.right: parent.right
                    anchors.rightMargin: clearQueryButton.visible ? 46 : 14
                    anchors.verticalCenter: parent.verticalCenter
                    width: visible ? Math.min(220, implicitWidth) : 0
                    visible: root.inlineStatusText !== ""
                    text: root.inlineStatusText
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: root.queryError !== "" || !root.serviceReady ? "#edb778" : Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: 12
                    Accessible.name: root.inlineStatusText
                }

                Rectangle {
                    id: clearQueryButton
                    visible: searchInput.text.length > 0
                    width: 32
                    height: 32
                    radius: 8
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    color: clearQueryMouse.pressed ? Theme.pressed : clearQueryMouse.containsMouse ? Theme.surface : "transparent"
                    Accessible.role: Accessible.Button
                    Accessible.name: "Clear search"
                    Accessible.onPressAction: {
                        searchInput.clear();
                        root.focusSearchInput();
                    }
                    SymbolicIcon {
                        anchors.centerIn: parent
                        implicitSize: 16
                        source: Quickshell.iconPath("edit-clear-symbolic", "window-close-symbolic")
                        color: clearQueryMouse.containsMouse ? Theme.text : Theme.muted
                    }
                    MouseArea {
                        id: clearQueryMouse
                        anchors.fill: parent
                        enabled: !root.activationPending && !root.closing
                        hoverEnabled: true
                        onClicked: {
                            searchInput.clear();
                            root.focusSearchInput();
                        }
                    }
                    ShellTooltip {
                        parent: clearQueryButton
                        visible: clearQueryMouse.containsMouse
                        text: "Clear search · Ctrl+L selects the query"
                    }
                }
            }

            ActionButton {
                visible: root.quickChatEnabled && searchInput.text.trim().startsWith("!") && root.chatPrompt() !== "" && root.queryComplete && root.results.length === 0 && !root.chatMode
                text: "Set up AI in Bingux Settings"
                iconName: "preferences-system-symbolic"
                Layout.fillWidth: true
                onClicked: {
                    root.closeSearch();
                    root.settingsRequested();
                }
            }

            ListView {
                id: chatTranscriptList

                Layout.fillWidth: true
                Layout.preferredHeight: visible ? Math.min(contentHeight, 360) : 0
                visible: root.chatMode && root.chatTranscript.length > 0
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                interactive: contentHeight > height
                model: root.chatTranscript
                onCountChanged: Qt.callLater(() => positionViewAtEnd())

                delegate: Column {
                    id: chatExchange

                    required property var modelData

                    width: chatTranscriptList.width
                    spacing: 4

                    Text {
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
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
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize
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
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
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
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize
                            text: chatExchange.modelData.pending ? (root.streamingChatText || "Connecting…") : chatExchange.modelData.message
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
                objectName: "searchResultsList"
                // Keep this bounded (20-result) model laid out so navigation can
                // use real provider row bounds without repositioning the list.
                cacheBuffer: Math.max(0, contentHeight)
                function revealSelection() {
                    keyboardScroll.stop();
                    cancelFlick();
                    forceLayout();
                    const row = itemAtIndex(root.selectedIndex);
                    if (!row) {
                        positionViewAtIndex(root.selectedIndex, ListView.Contain);
                        return;
                    }
                    let destination = contentY;
                    if (row.y < contentY)
                        destination = row.y;
                    else if (row.y + row.height > contentY + height)
                        destination = row.y + row.height - height;
                    destination = Math.max(originY, Math.min(destination, originY + Math.max(0, contentHeight - height)));
                    if (Theme.reducedMotion) {
                        contentY = destination;
                        return;
                    }
                    if (Math.abs(destination - contentY) < 1)
                        return;
                    keyboardScroll.to = destination;
                    keyboardScroll.restart();
                }
                NumberAnimation {
                    id: keyboardScroll
                    target: resultsList
                    property: "contentY"
                    duration: 150
                    easing.type: Easing.OutCubic
                }
                onMovementStarted: keyboardScroll.stop()
                onModelChanged: keyboardScroll.stop()

                Layout.fillWidth: true
                Layout.preferredHeight: visible ? Math.min(contentHeight, Math.max(64, Math.min(420, root.height - 160 - (chatTranscriptList.visible ? chatTranscriptList.height : 0)))) : 0
                visible: root.displayedResults.length > 0
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                // Selection belongs to the overlay; ListView's implicit current
                // item tracking would jump before our scroll animation starts.
                currentIndex: -1
                highlightFollowsCurrentItem: false
                interactive: !root.pointerBlocked && root.pointerResultIndex >= 0 && contentHeight > height
                model: root.displayedResults
                spacing: 2
                HoverHandler {
                    id: resultPointer
                    // Track the fixed window, so moving the popup beneath a
                    // stationary pointer cannot count as mouse movement.
                    parent: root.contentItem
                    property point lastPosition: Qt.point(-1, -1)
                    function selectAtPointer() {
                        const previous = Qt.point(lastPosition.x, lastPosition.y);
                        lastPosition = point.position;
                        if (!hovered || root.pointerBlocked || previous.x < 0 || previous.y < 0 || (point.position.x === previous.x && point.position.y === previous.y))
                            return;
                        const local = resultsList.mapFromItem(root.contentItem, point.position.x, point.position.y);
                        const index = local.x >= 0 && local.x < resultsList.width && local.y >= 0 && local.y < resultsList.height ? resultsList.indexAt(local.x + resultsList.contentX, local.y + resultsList.contentY) : -1;
                        root.pointerResultIndex = index;
                        if (index >= 0) {
                            root.keyboardSelection = false;
                            root.selectedIndex = index;
                        }
                    }
                    onPointChanged: selectAtPointer()
                    onHoveredChanged: {
                        lastPosition = point.position;
                        root.pointerResultIndex = -1;
                    }
                }

                delegate: Item {
                    id: groupedRow
                    readonly property var resultItem: resultLoader.item
                    required property int index
                    required property var modelData
                    readonly property string groupKey: SearchGroups.key(modelData)
                    readonly property bool firstInGroup: index === 0 || SearchGroups.key(root.displayedResults[index - 1]) !== groupKey
                    width: resultsList.width
                    height: resultLoader.height + (firstInGroup ? 24 : 0)

                    Text {
                        visible: groupedRow.firstInGroup
                        x: 14
                        y: 3
                        text: SearchGroups.label(groupedRow.groupKey)
                        textFormat: Text.PlainText
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                    }
                    Loader {
                        id: resultLoader
                        enabled: !root.pointerBlocked && root.pointerResultIndex === index
                        property int index: groupedRow.index
                        property var modelData: groupedRow.modelData
                        y: groupedRow.firstInGroup ? 24 : 0
                        width: resultsList.width
                        height: item ? item.implicitHeight : Theme.searchResultHeight
                        sourceComponent: root.providerDelegates[modelData.providerId] || defaultResult
                        Accessible.name: modelData.title + (modelData.subtitle ? ", " + modelData.subtitle : "")
                        Accessible.role: Accessible.Button
                        Accessible.onPressAction: root.activateResult(modelData)
                        onLoaded: {
                            item.result = Qt.binding(() => resultLoader.modelData);
                            item.query = Qt.binding(() => searchInput.text);
                            item.selected = Qt.binding(() => root.selectedIndex === resultLoader.index);
                            item.activationEnabled = Qt.binding(() => !root.activationPending && !root.awaitingResults && !root.closing);
                        }
                        Connections {
                            target: resultLoader.item
                            ignoreUnknownSignals: true
                            function onActivated(position) {
                                const origin = position ? resultLoader.item.mapToItem(root.contentItem, position.x, position.y) : null;
                                root.activateResult(resultLoader.modelData, origin);
                            }
                            function onContextMenuRequested(position) {
                                root.openAppMenu(resultLoader.modelData, resultLoader.item.mapToItem(root.contentItem, position.x, position.y));
                            }
                            function onPreviewToggled() {
                                root.previewOpen = !root.previewOpen;
                                root.focusSearchInput();
                            }
                        }
                        Component {
                            id: defaultResult
                            SearchResult {
                                previewOpen: root.previewOpen
                                previewAvailable: root.previewResult !== null && root.selectedIndex === resultLoader.index
                                claimChevronAnimation: () => root.claimChevronAnimation(resultLoader.index)
                                result: resultLoader.modelData
                                query: searchInput.text
                                selected: root.selectedIndex === resultLoader.index
                            }
                        }
                    }
                }
            }
        }
    }

    Rectangle {
        id: previewSurface
        objectName: "searchPreviewSurface"
        property var presentedResult: null
        readonly property var selectedResult: root.previewResult
        onSelectedResultChanged: if (selectedResult)
            presentedResult = selectedResult
        Component.onCompleted: if (selectedResult)
            presentedResult = selectedResult
        property real reveal: root.previewOpen && root.previewResult !== null ? 1 : 0
        Behavior on reveal {
            NumberAnimation {
                duration: root.previewOpen ? Theme.previewOpenMotion : Theme.previewCloseMotion
                easing.type: Easing.OutCubic
            }
        }
        readonly property bool besideSearch: root.width - surface.x - surface.width >= 344
        visible: root.visible && (reveal > 0 || (root.previewOpen && root.previewResult !== null))
        width: besideSearch ? Math.min(500, root.width - surface.x - surface.width - 24) : surface.width
        height: Math.min(480, root.height - 48)
        x: (besideSearch ? surface.x + surface.width + 10 : surface.x) - (1 - reveal) * 18
        y: Math.max(16, Math.min(surface.y, root.height - height - 24))
        opacity: surface.opacity * reveal
        radius: Theme.searchCardRadius
        color: Theme.searchSurface
        border.width: 1
        border.color: "#555960"
        PanelOutline {
            surface: previewSurface
        }
        clip: true
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onWheel: wheel => {
                wheel.accepted = true;
            }
        }
        SearchPreview {
            id: filePreview
            objectName: "searchPreview"
            anchors.fill: parent
            anchors.margins: 1
            enabled: !root.pointerBlocked && previewSurface.reveal === 1
            path: previewSurface.visible && previewSurface.presentedResult ? previewSurface.presentedResult.subtitle : ""
            title: previewSurface.presentedResult ? previewSurface.presentedResult.title : ""
        }
    }

    Item {
        parent: root.contentItem
        anchors.fill: parent
        z: 2
        Shortcut {
            sequence: "Escape"
            context: Qt.ApplicationShortcut
            enabled: root.visible && !root.closing
            onActivated: if (appMenu.visible)
                appMenu.visible = false
            else
                root.closeSearch()
        }
        Shortcut {
            sequence: "Ctrl+0"
            context: Qt.ApplicationShortcut
            enabled: root.visible && root.previewOpen
            onActivated: filePreview.resetZoom()
        }
        WheelHandler {
            enabled: root.previewOpen && root.previewResult !== null
            target: null
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            acceptedModifiers: Qt.ControlModifier
            onWheel: event => {
                const steps = event.pixelDelta.y ? event.pixelDelta.y / 40 : event.angleDelta.y / 120;
                if (steps !== 0)
                    filePreview.zoomBy(steps);
            }
        }
    }
}
