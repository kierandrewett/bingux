import QtQuick
import QtQuick.Effects
import QtQuick.Window
import Quickshell.Widgets
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

Window {
    id: root
    title: "Bingux Settings"
    visible: false
    width: 1000
    height: 740
    minimumWidth: 520
    minimumHeight: 420
    flags: Qt.Window | Qt.FramelessWindowHint
    color: "transparent"
    readonly property int shadowMargin: maximised ? 0 : 20
    readonly property int headerHeight: 47
    readonly property bool maximised: visibility === Window.Maximized
    function toggleMaximised() { if (maximised) showNormal(); else showMaximized(); }
    property var dockView: null
    property var systemMetrics: null
    property bool shellHosted: false
    property var currentLayout: null
    property string currentSidebarEdge: "right"
    readonly property alias searchPage: searchSettings
    signal saved()
    property var customiser: standaloneCustomiser
    property string page: "Desktop"
    property bool searchOpen: false
    function toggleSearch() { searchOpen = !searchOpen; if (searchOpen) { navigationOpen = true; Qt.callLater(() => globalSearch.forceActiveFocus()); } else { globalSearch.clear(); } }
    property string settingsQuery: ""
    readonly property bool searching: settingsQuery.trim().length > 0
    readonly property string category: page === "Controls" ? "Controls" : ["Desktop", "TopBar", "Dock", "Sidebar"].includes(page) ? "Desktop" : "Search"
    readonly property bool canGoBack: searchSubpage || !["Desktop", "Search", "Controls"].includes(page)
    function goBack() { if (searchSubpage) searchSettings.goBack(); else page = category; }
    readonly property var destinations: [
        {title: "Control Centre", description: "Control Centre", page: "Controls", words: "vpn tailscale bluetooth night light power awake notifications do not disturb"},
        {title: "Top Bar", description: "Desktop", page: "TopBar", words: "widgets icons labels status cpu memory"},
        {title: "Dock", description: "Desktop", page: "Dock", words: "applications pinned running icons alignment click scroll"},
        {title: "Sidebar", description: "Desktop", page: "Sidebar", words: "notes terminal calendar media position edge"},
        {title: "System monitors", description: "Desktop › Top Bar", page: "TopBar", target: "settingsMetrics", words: "cpu memory performance top bar"},
        {title: "Customise desktop", description: "Desktop", page: "Desktop", target: "customiseDesktop", words: "widgets layout control centre vpn bluetooth network"},
        {title: "Search results", description: "Search", page: "Providers", words: "applications files calculator conversions folders"},
        {title: "Search engines", description: "Search", page: "Engines", words: "websites shortcuts default duckduckgo custom"},
        {title: "AI Assistant", description: "Search", page: "AI", words: "pi claude model executable harness answers"},
        {title: "File previews", description: "Search", page: "Previews", words: "images documents pdf markdown video size limit prepare"},
        {title: "Dock alignment", description: "Desktop › Dock", page: "Dock", target: "settingsDockAlignment", words: "position left centre right"},
        {title: "Sidebar screen edge", description: "Desktop › Sidebar", page: "Sidebar", target: "settingsSidebarEdge", words: "position left right top"},
        {title: "Prepare nearby previews", description: "Search › File Previews", page: "Previews", target: "settingsPrewarm", words: "background fast preload cache"},
        {title: "Icon size", description: "Desktop › Dock", page: "Dock", target: "settingsDockSize", words: "small medium large"},
        {title: "Click a running app", description: "Desktop › Dock", page: "Dock", target: "settingsDockClick", words: "minimise focus launch"},
        {title: "Middle-click an app", description: "Desktop › Dock", page: "Dock", target: "settingsDockMiddle", words: "launch close mouse"},
        {title: "Scroll over the dock", description: "Desktop › Dock", page: "Dock", target: "settingsDockScroll", words: "cycle windows direction natural reverse"},
        {title: "Maximum file size", description: "Search › File Previews", page: "Previews", target: "settingsPreviewLimit", words: "megabytes mb limit large"}
    ]
    readonly property var indexedSettings: destinations.concat([
        {title: "VPN", description: "Control Centre", page: "Controls", target: "settingsControlvpn", words: "tailscale connections exit node"},
        {title: "Do Not Disturb", description: "Control Centre", page: "Controls", target: "settingsControldnd", words: "notification banners pause"},
        {title: "Night Light", description: "Control Centre", page: "Controls", target: "settingsControlnightLight", words: "blue light display"},
        {title: "Power mode", description: "Control Centre", page: "Controls", target: "settingsControlpower", words: "battery performance"},
        {title: "Keep awake", description: "Control Centre", page: "Controls", target: "settingsControlawake", words: "sleep inhibit"},
        {title: "Sidebar panels", description: "Desktop › Sidebar", page: "Sidebar", words: "notes tasks calendar terminal media system"}
    ])
    readonly property var searchMatches: indexedSettings.filter(entry => settingsQuery.toLowerCase().trim().split(/\s+/).every(word => (entry.title + " " + entry.description + " " + entry.words).toLowerCase().includes(word)))
    function openDestination(entry) {
        settingsQuery = ""; globalSearch.clear(); page = entry.page; navigationOpen = false;
        const container = desktopLayoutSettings.containers.find(item => item.page === entry.page);
        if (container && entry.target) { openContainerCustomise(container.id); return; }
        if (["Providers", "Engines"].includes(entry.page)) searchSettings.goBack();
        focusDestination.targetName = entry.target || ""; focusDestination.restart();
    }
    Timer {
        id: focusDestination; property string targetName: ""; interval: 220
        function find(item) { if (item.objectName === targetName) return item; for (const child of item.children || []) { const result = find(child); if (result) return result; } return null; }
        onTriggered: { const item = targetName ? find(pageContent) : null; if (item) { item.forceActiveFocus(Qt.TabFocusReason); const point = item.mapToItem(pageContent, 0, 0); pageScroll.contentY = Math.max(0, Math.min(point.y - 24, pageScroll.contentHeight - pageScroll.height)); } }
    }
    property var draft: JSON.parse(JSON.stringify(BinguxPreferences.data))
    property var harnesses: []
    property var desktopDefaults: null
    property bool dirty: false
    property string status: ""
    property string selectedHarness: "pi"
    property string model: ""
    property string executable: ""
    property bool aiEnabled: false
    property bool ready: false
    property string operation: ""
    property var submittedDraft: ({})
    property var changedSettings: ({})
    property var pendingUndo: ({})
    property var undoHistory: []
    property bool replySucceeded: false
    readonly property bool canUndo: Object.keys(pendingUndo).length > 0 || undoHistory.length > 0
    function syncAI() {
        aiEnabled = !!draft.search.ai;
        selectedHarness = draft.search.ai?.harness || "pi";
        model = draft.search.ai?.model || "";
        executable = draft.search.ai?.executable || "";
    }
    function editAI() {
        update("search", "ai", aiEnabled ? {harness: selectedHarness, model: model.trim(), executable: executable.trim()} : null);
    }
    function undo() {
        if (busy || !canUndo) return;
        autoSave.stop();
        let patch;
        if (Object.keys(pendingUndo).length) { patch = pendingUndo; pendingUndo = {}; }
        else { patch = undoHistory[undoHistory.length - 1]; undoHistory = undoHistory.slice(0, -1); }
        for (const section of Object.keys(patch))
            for (const key of Object.keys(patch[section])) update(section, key, patch[section][key], false);
        syncAI();
        save();
    }
    Timer { id: autoSave; interval: 450; onTriggered: if (root.dirty && !root.busy && !customiser.visible) root.save() }
    Timer { id: savedNotice; interval: 1800; onTriggered: if (root.status === "Saved") root.status = "" }

    property var requestedCustomise: null
    function openContainerCustomise(container) {
        if (!customiser.containerChoices.some(item => item.id === container)) return;
        if (!shellHosted) { requestShellCustomise(container); return; }
        requestedCustomise = {widgetId: "", options: "Container", container};
        if (dirty && !busy) showCustomiser();
        else if (!busy) read();
    }
    function openCustomise(widgetId, options) {
        requestedCustomise = {widgetId: widgetId || "", options: options || ""};
        if (dirty && !busy) showCustomiser();
        else if (!busy) read();
    }
    function showCustomiser() {
        const request = requestedCustomise;
        requestedCustomise = null;
        customiser.open();
        customiser.selectedWidget = request.widgetId;
        if (request.options === "Remove") customiser.put(request.widgetId, "palette", 0);
        else customiser.optionsPage = request.options;
        if (request.container) customiser.selectContainer(request.container);
        else if (!request.widgetId) customiser.selectedContainer = "control-centre";
    }
    readonly property bool busy: operation !== ""
    function update(section, key, value, recordUndo = true) {
        if (JSON.stringify(draft[section][key]) === JSON.stringify(value)) return;
        if (recordUndo && !(key in (pendingUndo[section] || {})))
            pendingUndo = Object.assign({}, pendingUndo, {[section]: Object.assign({}, pendingUndo[section] || {}, {[key]: JSON.parse(JSON.stringify(draft[section][key]))})});
        const next = JSON.parse(JSON.stringify(draft));
        next[section][key] = value;
        draft = next;
        changedSettings = Object.assign({}, changedSettings, {[section]: Object.assign({}, changedSettings[section] || {}, {[key]: value})});
        dirty = true;
        status = "";
        autoSave.restart();
    }
    function setProvider(key, enabled) {
        const disabled = draft.search.disabledProviders.filter(id => id !== key);
        if (!enabled) disabled.push(key);
        update("search", "disabledProviders", disabled);
    }
    function read() { if (busy) return; autoSave.stop(); changedSettings = {}; pendingUndo = {}; dirty = false; receivedReply = false; replySucceeded = false; operation = "read"; backend.command = helper.concat(["read"]); backend.running = true; }
    function save() {
        if (busy) return;
        autoSave.stop();
        if (!Object.keys(changedSettings).length) { saved(); return; }
        submittedDraft = JSON.parse(JSON.stringify(changedSettings));
        if (Object.keys(pendingUndo).length) undoHistory = undoHistory.concat([pendingUndo]).slice(-30);
        pendingUndo = {};
        changedSettings = {};
        dirty = false;
        receivedReply = false;
        replySucceeded = false;
        operation = "save";
        backend.command = helper.concat(["save"]);
        backend.running = true;
    }
    readonly property var helper: BinguxPreferences.helper
    onVisibleChanged: if (visible && !dirty && !searchSettings.editing) read()
    Component.onCompleted: Qt.callLater(() => { if (visible && !ready && !busy) read(); })
    Shortcut { sequence: "Ctrl+S"; enabled: root.visible && root.dirty && !root.busy && !searchSettings.editing; onActivated: root.save() }
    Shortcut { sequence: "Ctrl+F"; enabled: root.visible; onActivated: { root.searchOpen = true; root.navigationOpen = true; globalSearch.forceActiveFocus(); globalSearch.selectAll(); } }
    Shortcut { sequence: "Alt+Left"; enabled: root.visible && root.canGoBack; onActivated: root.goBack() }
    Shortcut { sequence: "Escape"; enabled: root.visible && (root.navigationOpen || root.canGoBack || root.searchOpen); onActivated: { if (root.searchOpen) { root.searchOpen = false; globalSearch.clear(); } else if (root.navigationOpen) root.navigationOpen = false; else root.goBack(); } }
    Process {
        id: backend
        stdinEnabled: true
        onStarted: if (command[command.length - 1] === "save") { write(JSON.stringify(root.submittedDraft)); stdinEnabled = false; }
        onExited: {
            stdinEnabled = true;
            if (!root.receivedReply) root.status = "Settings could not be " + (root.operation === "save" ? "saved. Your changes are still here." : "loaded. Try again.");
            if (!root.replySucceeded && root.operation === "save") {
                for (const section of Object.keys(root.submittedDraft))
                    root.changedSettings = Object.assign({}, root.changedSettings, {[section]: Object.assign({}, root.submittedDraft[section], root.changedSettings[section] || {})});
                root.dirty = true;
            }
            root.operation = "";
            if (root.replySucceeded && root.dirty) autoSave.restart();
        }
        stdout: SplitParser {
            onRead: line => {
                let result;
                try { result = JSON.parse(line); } catch (_) { root.status = "Could not read settings."; return; }
                root.receivedReply = true;
                if (result.error) { root.status = result.error; return; }
                root.ready = true;
                root.replySucceeded = true;
                const merged = JSON.parse(JSON.stringify(result.data));
                for (const section of Object.keys(root.changedSettings)) Object.assign(merged[section], root.changedSettings[section]);
                root.draft = merged;
                root.dirty = Object.keys(root.changedSettings).length > 0;
                if (root.operation === "read") root.syncAI();
                if (result.harnesses) root.harnesses = result.harnesses;
                if (result.desktopDefaults) root.desktopDefaults = result.desktopDefaults;
                root.status = result.warning || (backend.command[backend.command.length - 1] === "save" ? "Saved" : "");
                BinguxPreferences.data = result.data;
                if (root.status === "Saved") savedNotice.restart();
                if (backend.command[backend.command.length - 1] === "save") root.saved();
                if (root.requestedCustomise) Qt.callLater(root.showCustomiser);
            }
        }
    }
    Process {
        id: customiseRequest
        command: ["qs", "-p", Quickshell.shellPath("shell.qml"), "ipc", "call", "shell"].concat(root.shellCustomiseContainer ? ["customiseContainer", root.shellCustomiseContainer] : ["customise"])
        stderr: StdioCollector { onStreamFinished: if (text.trim()) console.warn("Customise IPC:", text.trim()) }
        onExited: code => {
            if (code !== 0) root.status = "Could not open desktop customisation.";
            else root.visible = false;
        }
    }

    property bool shellCustomisePending: false
    property string shellCustomiseContainer: ""
    function requestShellCustomise(container = "") {
        shellCustomiseContainer = container;
        shellCustomisePending = true;
        dispatchShellCustomise();
    }
    function dispatchShellCustomise() {
        if (busy) return;
        if (dirty) { save(); return; }
        shellCustomisePending = false;
        customiseRequest.running = true;
    }
    onBusyChanged: if (!busy && shellCustomisePending) {
        if (ready && !dirty) Qt.callLater(root.dispatchShellCustomise);
        else shellCustomisePending = false;
    }
    DesktopCustomise { id: standaloneCustomiser; settings: root; screen: Quickshell.screens.find(s => s.name === root.screen.name) || Quickshell.screens[0] }
    readonly property bool wideLayout: width - shadowMargin * 2 >= 740
    property bool navigationOpen: false
    property bool advancedOpen: false
    property bool formatsOpen: false
    readonly property bool searchSubpage: ["Providers", "Engines"].includes(page) && (searchSettings.editing || searchSettings.detail !== "")
    readonly property string pageTitle: searchSubpage ? searchSettings.displayTitle : page === "Engines" ? "Search Engines" : page === "Providers" ? "Search Results" : page === "TopBar" ? "Top Bar" : page === "Controls" ? "Control Centre" : page === "AI" ? "AI Assistant" : page === "Previews" ? "File Previews" : page
    readonly property string saveState: operation === "read" ? "Loading settings…" : operation === "save" ? "Saving changes…" : dirty ? "Saving changes…" : status === "Saved" ? "Changes saved" : ""
    property bool receivedReply: false
    property string previousPage: "Search"
    property real pageOffset: 0
    property int slideDirection: 1
    function slidePage(direction) {
        pageScroll.contentY = 0;
        pageTransition.stop();
        if (Theme.reducedMotion) { pageOffset = 0; pageContent.opacity = 1; return; }
        slideDirection = direction;
        pageTransition.restart();
    }
    onPageChanged: {
        navigationOpen = false;
        if (["Providers", "Engines"].includes(page)) { searchSettings.goBack(); searchSettings.filter = ""; }
        const pages = ["Search", "AI", "Previews", "Desktop"];
        slidePage(pages.indexOf(page) >= pages.indexOf(previousPage) ? 1 : -1);
        previousPage = page;
    }

    component Caption: Text {
        Layout.fillWidth: true
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }
    component Section: SettingsHeading {}
    component Group: SettingsGroup {}
    component PreferenceRow: SettingsRow {
        iconName: ""
        toggleVisible: true
        // The switch is the single keyboard stop. Clicking the row toggles it too.
        focusPolicy: Qt.NoFocus
        Accessible.role: Accessible.Grouping
        onClicked: toggleRequested()
    }
    component EntryRow: SettingsField {}

    Rectangle {
        id: shadowShape
        x: root.shadowMargin; y: root.shadowMargin
        width: root.width - root.shadowMargin * 2; height: root.height - root.shadowMargin * 2
        radius: Theme.radius; color: Theme.settingsBackground; visible: false
    }
    MultiEffect {
        source: shadowShape
        anchors.fill: shadowShape
        visible: !root.maximised
        shadowEnabled: true; shadowBlur: 0.65; shadowOpacity: root.active ? 0.3 : 0.16
        shadowVerticalOffset: 3; shadowColor: "black"
        blurMax: 20
    }
    ClippingRectangle {
        id: windowFrame
        anchors.fill: parent
        anchors.margins: root.shadowMargin
        radius: root.maximised ? 0 : Theme.radius
        color: Theme.settingsBackground
    Rectangle {
        anchors.fill: parent
        color: Theme.settingsBackground
    }
    Item {
        id: mainPane
        anchors.fill: parent
        anchors.leftMargin: root.wideLayout ? navigation.width : 0
        Rectangle {
            id: header
            anchors.top: parent.top
            width: parent.width
            height: root.headerHeight
            color: Theme.settingsBackground
            MouseArea {
                anchors.fill: parent
                onPressed: root.startSystemMove()
                onDoubleClicked: root.toggleMaximised()
            }
            Row {
                id: windowControls
                anchors.right: parent.right; anchors.rightMargin: 7
                anchors.verticalCenter: parent.verticalCenter
                spacing: 3
                Repeater {
                    model: ["minimize", "maximize", "close"]
                    IconButton {
                        required property string modelData
                        objectName: "settingsWindow" + modelData
                        implicitWidth: 34; implicitHeight: 34
                        iconName: "window-" + (modelData === "maximize" && root.maximised ? "restore" : modelData) + "-symbolic"
                        label: modelData === "minimize" ? "Minimise" : modelData === "maximize" ? (root.maximised ? "Restore" : "Maximise") : "Close Settings"
                        background: Item {
                            ControlCentreButtonSurface { anchors.centerIn: parent; width: 24; height: 24; control: parent.parent; radius: 12; baseColor: Theme.surface }
                        }
                        onClicked: { if (modelData === "close") root.close(); else if (modelData === "minimize") root.showMinimized(); else root.toggleMaximised(); }
                    }
                }
            }
            Text {
                id: headerTitle
                readonly property real leftReserve: !root.wideLayout || root.canGoBack ? 56 : 16
                readonly property real rightReserve: windowControls.width + headerActions.width + 35
                width: Math.max(0, parent.width - leftReserve - rightReserve)
                x: leftReserve
                y: (parent.height - height) / 2
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                text: root.pageTitle
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize
                font.weight: Font.DemiBold
                color: Theme.text
            }
            IconButton {
                anchors.left: parent.left; anchors.leftMargin: Theme.padding
                anchors.verticalCenter: parent.verticalCenter
                visible: !root.wideLayout || root.canGoBack
                iconName: root.canGoBack ? "go-previous-symbolic" : "sidebar-show-symbolic"
                label: root.canGoBack ? "Back to " + (root.searchSubpage ? (root.page === "Engines" ? "Search Engines" : "Search Results") : root.category === "Controls" ? "Control Centre" : root.category) : "Show settings pages"
                objectName: "settingsNavigationToggle"
                onClicked: { if (root.canGoBack) root.goBack(); else root.navigationOpen = !root.navigationOpen; }
            }
            RowLayout {
                id: headerActions
                anchors.right: parent.right; anchors.rightMargin: windowControls.width + 19
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.spaceSmall
                IconButton {
                    objectName: "settingsUndo"
                    enabled: root.canUndo && !root.busy
                    iconName: "edit-undo-symbolic"
                    label: root.saveState || "Undo last change"
                    onClicked: root.undo()
                }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.barDivider; visible: pageScroll.contentY > 0 }
        }
        Flickable {
            id: pageScroll
            objectName: "settingsPageScroll"
            anchors.top: header.bottom
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
            contentWidth: width
            contentHeight: pageContent.implicitHeight + 56
            boundsBehavior: Flickable.StopAtBounds
            clip: true
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            ColumnLayout {
                id: pageContent
                x: Math.round((parent.width - width) / 2) + root.pageOffset
                y: 24
                width: Math.min(600, parent.width - (root.wideLayout ? 64 : 32))
                spacing: 24
                enabled: root.ready
                ParallelAnimation {
                    id: pageTransition
                    NumberAnimation { target: root; property: "pageOffset"; from: root.slideDirection * 42; to: 0; duration: 200; easing.type: Easing.OutCubic }
                    NumberAnimation { target: pageContent; property: "opacity"; from: 0.35; to: 1; duration: 160; easing.type: Easing.OutCubic }
                }
                Rectangle {
                    visible: root.status !== "" && root.status !== "Saved"
                    Layout.fillWidth: true
                    implicitHeight: notice.implicitHeight + Theme.padding * 2
                    color: Theme.surface; radius: Theme.radius
                    Text { id: notice; anchors.fill: parent; anchors.margins: Theme.padding; text: root.status; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.warning; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                }
                ColumnLayout {
                    visible: root.page === "Search"
                    Layout.fillWidth: true; spacing: 24
                    Section {
                        title: "Search"
                        Group {
                            SettingsRow { title: "Search results"; subtitle: "Applications, files, calculations and conversions"; navigation: true; onClicked: root.page = "Providers" }
                            SettingsRow { title: "Search engines"; subtitle: "Default website search and custom shortcuts"; navigation: true; onClicked: root.page = "Engines" }
                            SettingsRow { title: "AI Assistant"; subtitle: "Ask questions with ! using your CLI provider"; valueText: root.aiEnabled ? "On" : "Off"; navigation: true; onClicked: root.page = "AI" }
                            SettingsRow { title: "File previews"; subtitle: "Preview documents, images and media"; valueText: root.draft.previews.enabled ? "On" : "Off"; navigation: true; onClicked: root.page = "Previews" }
                        }
                    }
                }
                SearchSettings {
                    id: searchSettings
                    settings: root
                    enginesOnly: root.page === "Engines"
                    visible: ["Providers", "Engines"].includes(root.page)
                    onDetailChanged: root.slidePage(detail ? 1 : -1)
                    onEditingChanged: root.slidePage(editing ? 1 : -1)
                }
                ColumnLayout {
                    visible: root.page === "AI"
                    Layout.fillWidth: true; spacing: 24
                    Section {
                        title: "Quick answers"
                        Group {
                            PreferenceRow { objectName: "settingsAIEnabled"; title: "AI Assistant"; subtitle: "Start a search with ! to ask a question"; toggleChecked: root.aiEnabled; onToggleRequested: { root.aiEnabled = !root.aiEnabled; root.editAI(); } }
                        }
                        Caption { text: "Press Enter to send. Only ! questions are sent to your AI provider."; Layout.leftMargin: Theme.spaceSmall }
                    }
                    Section {
                        title: "Provider"
                        Group {
                            SettingsOptionRow {
                                objectName: "settingsAIProvider"
                                title: "CLI harness"
                                subtitle: root.harnesses.some(item => item.id === root.selectedHarness && item.path) ? "Installed · uses your existing login" : "Not found · install it or set an executable below"
                                value: root.selectedHarness
                                options: [{value: "pi", label: "Pi"}, {value: "claude", label: "Claude Code"}]
                                onChosen: value => { root.selectedHarness = value; root.executable = ""; root.editAI(); }
                            }
                        }
                    }
                    Section {
                        title: "Options"
                        Group {
                            EntryRow { label: "Model"; placeholderText: "Use the provider default"; text: root.model; onEdited: value => { root.model = value; root.editAI(); } }
                            SettingsRow { objectName: "settingsAdvanced"; title: "Custom executable"; navigationRotation: root.advancedOpen ? 90 : 0; iconName: ""; navigation: true; onClicked: root.advancedOpen = !root.advancedOpen }
                            SettingsReveal { expanded: root.advancedOpen
                            EntryRow { objectName: "settingsExecutable"; label: "Executable path"; placeholderText: "Detect automatically"; text: root.executable; onEdited: value => { root.executable = value; root.editAI(); } }
                            }
                        }
                    }
                    Caption { text: "Esc stops a reply. Tools and project access are disabled."; Layout.leftMargin: Theme.spaceSmall }
                }
                ColumnLayout {
                    visible: root.page === "Previews"
                    Layout.fillWidth: true; spacing: 24
                    Section {
                        title: "Behaviour"
                        description: "Open a preview beside your search results."
                        Group {
                            PreferenceRow { title: "Show previews"; subtitle: "Press Right Arrow on a selected file"; toggleChecked: root.draft.previews.enabled; onToggleRequested: root.update("previews", "enabled", !toggleChecked) }
                            PreferenceRow { objectName: "settingsPrewarm"; enabled: root.draft.previews.enabled; title: "Prepare nearby previews"; subtitle: "Make nearby results faster to open"; toggleChecked: root.draft.previews.prewarm; onToggleRequested: root.update("previews", "prewarm", !toggleChecked) }
                        }
                    }
                    Section {
                        title: "File size limit"
                        Group {
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.leftMargin: Theme.padding; Layout.rightMargin: Theme.padding
                                Layout.topMargin: Theme.spaceSmall; Layout.bottomMargin: Theme.spaceSmall
                                implicitHeight: 40
                                Text { Layout.fillWidth: true; text: "Maximum file size"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
                                IconButton { iconName: "list-remove-symbolic"; label: "Decrease preview size limit"; enabled: root.draft.previews.maxMegabytes > 1; onClicked: root.update("previews", "maxMegabytes", root.draft.previews.maxMegabytes - 1) }
                                TextField {
                                    objectName: "settingsPreviewLimit"
                                    text: root.draft.previews.maxMegabytes.toString()
                                    Layout.preferredWidth: 38; implicitHeight: 32
                                    horizontalAlignment: Text.AlignHCenter
                                    color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                                    selectByMouse: true; Accessible.name: "Maximum preview size in megabytes"
                                    validator: IntValidator { bottom: 1; top: 20 }
                                    background: Rectangle { radius: 6; color: parent.activeFocus ? Theme.elevated : "transparent"; border.width: parent.activeFocus ? 1 : 0; border.color: Theme.accent }
                                    onEditingFinished: {
                                        if (acceptableInput) root.update("previews", "maxMegabytes", parseInt(text));
                                        text = Qt.binding(() => root.draft.previews.maxMegabytes.toString());
                                    }
                                }
                                Text { text: "MB"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                                IconButton { iconName: "list-add-symbolic"; label: "Increase preview size limit"; enabled: root.draft.previews.maxMegabytes < 20; onClicked: root.update("previews", "maxMegabytes", root.draft.previews.maxMegabytes + 1) }
                            }
                        }
                        Caption { text: "Larger files will not be previewed. The maximum is 20 MB."; Layout.leftMargin: Theme.spaceSmall }
                    }
                    Section {
                        title: "Preview controls"
                        Group {
                            SettingsRow { title: "Open preview"; iconName: ""; valueText: "Right Arrow"; rowInteractive: false; implicitHeight: 44 }
                            SettingsRow { title: "Zoom"; iconName: ""; valueText: "Ctrl + scroll"; rowInteractive: false; implicitHeight: 44 }
                            SettingsRow { title: "Reset zoom"; iconName: ""; valueText: "Ctrl + 0"; rowInteractive: false; implicitHeight: 44 }
                        }
                    }
                    Group {
                        SettingsRow { objectName: "settingsSupportedFormats"; title: "Supported file types"; iconName: ""; navigation: true; navigationRotation: root.formatsOpen ? 90 : 0; onClicked: root.formatsOpen = !root.formatsOpen }
                        SettingsReveal { expanded: root.formatsOpen
                        Caption { Layout.margins: Theme.padding; text: "Images, video and audio\nPDFs and Office documents\nMarkdown, HTML and source code\nTables, email, archives and SQLite databases"; lineHeight: 1.5 }
                        }
                    }

                }
                DesktopLayoutSettings {
                    id: desktopLayoutSettings
                    settings: root
                    visible: ["Desktop", "TopBar", "Dock", "Sidebar", "Controls"].includes(root.page)
                }

            }
        }
    }
    Rectangle {
        anchors.fill: parent
        visible: !root.wideLayout && root.navigationOpen
        color: Qt.rgba(0, 0, 0, 0.35)
        MouseArea { anchors.fill: parent; onClicked: root.navigationOpen = false }
    }
    Rectangle {
        id: navigation
        objectName: "settingsNavigation"
        width: 260
        anchors.top: parent.top; anchors.bottom: parent.bottom; anchors.left: parent.left
        visible: root.wideLayout || root.navigationOpen
        color: Theme.settingsSidebar
        Rectangle { anchors.right: parent.right; height: parent.height; width: 1; color: Theme.barDivider }
        MouseArea {
            width: parent.width; height: root.headerHeight
            onPressed: root.startSystemMove()
            onDoubleClicked: root.toggleMaximised()
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            height: root.headerHeight
            text: "Settings"
            verticalAlignment: Text.AlignVCenter
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            font.weight: Font.DemiBold
        }
        IconButton {
            objectName: "settingsSearchToggle"
            x: 6; y: (root.headerHeight - height) / 2
            implicitWidth: 34; implicitHeight: 34
            iconName: "system-search-symbolic"; label: "Search settings"
            highlighted: root.searchOpen
            onClicked: root.toggleSearch()
        }
        ColumnLayout {
            id: navigationContent
            y: root.headerHeight

            x: 6
            width: parent.width - 12
            spacing: 2
            Item {
                Layout.fillWidth: true
                implicitHeight: root.searchOpen ? 46 : 0
                clip: true; visible: implicitHeight > 0
                Behavior on implicitHeight { NumberAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic } }
                FilterField {
                    id: globalSearch; objectName: "settingsSearch"
                    x: 0; y: 3; width: parent.width; implicitHeight: 34
                    placeholderText: "Search settings"; Accessible.name: placeholderText
                    onTextChanged: root.settingsQuery = text
                    onAccepted: if (root.searchMatches.length && root.searching) root.openDestination(root.searchMatches[0])
                    Keys.onEscapePressed: { root.searchOpen = false; clear(); }
                    Keys.onDownPressed: if (resultItems.count && root.searching) resultItems.itemAt(0).forceActiveFocus()
                }
            }
            Flickable {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(0, navigation.height - navigationContent.y - (root.searchOpen ? 48 : 2))
                contentHeight: sidebarRows.implicitHeight + 12; clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                ColumnLayout {
                    id: sidebarRows
                    y: 6; width: parent.width; spacing: 2
                    Repeater {
                        id: resultItems
                        model: root.searching ? root.searchMatches : []
                        SettingsNavigationRow {
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            text: modelData.title
                            description: modelData.description
                            iconName: modelData.description.startsWith("Desktop") ? "preferences-desktop-display-symbolic" : "system-search-symbolic"
                            onClicked: root.openDestination(modelData)
                            Keys.onPressed: event => {
                                if (event.key === Qt.Key_Up && index === 0) globalSearch.forceActiveFocus();
                                else if (event.key === Qt.Key_Down || event.key === Qt.Key_Up) resultItems.itemAt(Math.max(0, Math.min(resultItems.count - 1, index + (event.key === Qt.Key_Down ? 1 : -1)))).forceActiveFocus();
                                else return;
                                event.accepted = true;
                            }
                        }
                    }
                    Text {
                        visible: root.searching && !root.searchMatches.length
                        Layout.fillWidth: true; Layout.margins: 16
                        text: "No results found"; wrapMode: Text.Wrap
                        horizontalAlignment: Text.AlignHCenter
                        color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                    }
            Repeater {
                id: navItems
                model: [{name: "Desktop", label: "Desktop", icon: "preferences-desktop-display-symbolic"}, {name: "Controls", label: "Control Centre", icon: "preferences-system-symbolic"}, {name: "Search", label: "Search", icon: "system-search-symbolic"}]
                SettingsNavigationRow {
                    id: navButton
                    required property int index
                    required property var modelData
                    Layout.fillWidth: true
                    visible: !root.searching
                    text: modelData.label; iconName: modelData.icon; selected: !root.searching && root.category === modelData.name
                    objectName: "settingsNav" + modelData.name
                    Accessible.role: Accessible.PageTab
                    Accessible.selected: !root.searching && root.category === modelData.name
                    onClicked: { globalSearch.clear(); root.page = modelData.name; root.navigationOpen = false; }
                    Keys.onPressed: event => {
                        if (event.key !== Qt.Key_Up && event.key !== Qt.Key_Down) return;
                        const next = navItems.itemAt(Math.max(0, Math.min(navItems.count - 1, index + (event.key === Qt.Key_Down ? 1 : -1))));
                        root.page = next.modelData.name;
                        next.forceActiveFocus();
                        event.accepted = true;
                    }
                }
            }
                }
            }
        }
    }
    }
    Rectangle {
        anchors.fill: windowFrame
        color: "transparent"
        radius: root.maximised ? 0 : Theme.radius
        border.width: root.maximised ? 0 : 1
        border.color: Theme.barDivider
    }
    Repeater {
        model: [Qt.LeftEdge, Qt.RightEdge, Qt.TopEdge, Qt.BottomEdge,
                Qt.TopEdge | Qt.LeftEdge, Qt.TopEdge | Qt.RightEdge,
                Qt.BottomEdge | Qt.LeftEdge, Qt.BottomEdge | Qt.RightEdge]
        MouseArea {
            required property int modelData
            readonly property bool horizontalEdge: (modelData & (Qt.LeftEdge | Qt.RightEdge)) !== 0
            readonly property bool verticalEdge: (modelData & (Qt.TopEdge | Qt.BottomEdge)) !== 0
            enabled: !root.maximised
            width: horizontalEdge ? 6 : windowFrame.width - 12
            height: verticalEdge ? 6 : windowFrame.height - 12
            x: root.shadowMargin + (modelData & Qt.RightEdge ? windowFrame.width - width : horizontalEdge ? 0 : 6)
            y: root.shadowMargin + (modelData & Qt.BottomEdge ? windowFrame.height - height : verticalEdge ? 0 : 6)
            cursorShape: horizontalEdge && verticalEdge ? ((modelData === (Qt.TopEdge | Qt.LeftEdge) || modelData === (Qt.BottomEdge | Qt.RightEdge)) ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor) : horizontalEdge ? Qt.SizeHorCursor : Qt.SizeVerCursor
            onPressed: root.startSystemResize(modelData)
        }
    }

}
