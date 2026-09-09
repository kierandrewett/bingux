import QtQuick
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
    width: 960
    height: 700
    minimumWidth: 520
    minimumHeight: 420
    flags: Qt.Window | Qt.FramelessWindowHint
    color: "transparent"
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
    property string page: "Search"
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
    function update(section, key, value) {
        const next = JSON.parse(JSON.stringify(draft));
        next[section][key] = value;
        draft = next;
        changedSettings = Object.assign({}, changedSettings, {[section]: Object.assign({}, changedSettings[section] || {}, {[key]: value})});
        dirty = true;
        status = "";
    }
    function setProvider(key, enabled) {
        const disabled = draft.search.disabledProviders.filter(id => id !== key);
        if (!enabled) disabled.push(key);
        update("search", "disabledProviders", disabled);
    }
    function read() { if (busy) return; receivedReply = false; operation = "read"; backend.command = helper.concat(["read"]); backend.running = true; }
    function save() {
        if (busy) return;
        update("search", "ai", aiEnabled ? {harness: selectedHarness, model: model.trim(), executable: executable.trim()} : null);
        submittedDraft = JSON.parse(JSON.stringify(changedSettings));
        receivedReply = false;
        operation = "save";
        backend.command = helper.concat(["save"]);
        backend.running = true;
    }
    readonly property var helper: BinguxPreferences.helper
    onVisibleChanged: if (visible && !dirty && !searchSettings.editing) read()
    Process {
        id: backend
        stdinEnabled: true
        onStarted: if (command[command.length - 1] === "save") { write(JSON.stringify(root.submittedDraft)); stdinEnabled = false; }
        onExited: {
            stdinEnabled = true;
            if (!root.receivedReply) root.status = "Settings could not be " + (root.operation === "save" ? "saved. Your changes are still here." : "loaded. Try again.");
            root.operation = "";
        }
        stdout: SplitParser {
            onRead: line => {
                let result;
                try { result = JSON.parse(line); } catch (_) { root.status = "Could not read settings."; return; }
                root.receivedReply = true;
                if (result.error) { root.status = result.error; return; }
                root.ready = true;
                root.draft = result.data;
                root.changedSettings = {};
                root.dirty = false;
                root.aiEnabled = !!result.data.search.ai;
                root.selectedHarness = result.data.search.ai?.harness || "pi";
                root.model = result.data.search.ai?.model || "";
                root.executable = result.data.search.ai?.executable || "";
                if (result.harnesses) root.harnesses = result.harnesses;
                if (result.desktopDefaults) root.desktopDefaults = result.desktopDefaults;
                root.status = result.warning || (backend.command[backend.command.length - 1] === "save" ? "Saved" : "");
                BinguxPreferences.data = result.data;
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
    readonly property bool wideLayout: width >= 740
    property bool navigationOpen: false
    property bool advancedOpen: false
    readonly property bool searchSubpage: page === "Search" && (searchSettings.editing || searchSettings.detail !== "")
    readonly property string pageTitle: searchSubpage ? searchSettings.displayTitle : page === "AI" ? "AI Assistant" : page === "Previews" ? "File Previews" : page
    readonly property string saveState: operation === "read" ? "Loading settings…" : operation === "save" ? "Saving changes…" : dirty ? "Unsaved changes" : status === "Saved" ? "Changes saved" : ""
    property bool receivedReply: false
    onPageChanged: { navigationOpen = false; pageScroll.contentY = 0; pageFade.restart(); }

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
    component PreferenceRow: ControlRow {
        iconName: ""
        implicitHeight: subtitle ? 64 : 54
        leftPadding: Theme.spaceSmall
        rightPadding: Theme.spaceSmall
        toggleVisible: true
        // The switch is the single keyboard stop. Clicking the row toggles it too.
        focusPolicy: Qt.NoFocus
        Accessible.role: Accessible.Grouping
        onClicked: toggleRequested()
    }
    component EntryRow: SettingsField {}

    ClippingRectangle {
        anchors.fill: parent
        radius: root.maximised ? 0 : Theme.radius
        color: Theme.barBackground
    Rectangle {
        anchors.fill: parent
        color: Theme.barBackground
    }
    Item {
        id: mainPane
        anchors.fill: parent
        anchors.leftMargin: root.wideLayout ? navigation.width : 0
        Rectangle {
            id: header
            anchors.top: parent.top
            width: parent.width
            height: 56
            color: Theme.barBackground
            MouseArea {
                anchors.fill: parent
                onPressed: root.startSystemMove()
                onDoubleClicked: root.toggleMaximised()
            }
            IconButton {
                objectName: "settingsClose"
                anchors.right: parent.right; anchors.rightMargin: 56
                anchors.verticalCenter: parent.verticalCenter
                iconName: "window-close-symbolic"; label: "Close Settings"
                background: ControlCentreButtonSurface { control: parent; radius: 16; baseColor: Theme.surface }
                onClicked: root.close()
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                y: root.saveState ? 8 : (parent.height - height) / 2
                width: Math.max(80, parent.width - 320)
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                text: root.pageTitle
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontHeading
                font.weight: Font.DemiBold
                color: Theme.text
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                y: 31
                text: root.saveState
                visible: text !== ""
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
            IconButton {
                anchors.left: parent.left; anchors.leftMargin: Theme.padding
                anchors.verticalCenter: parent.verticalCenter
                visible: !root.wideLayout || root.searchSubpage
                iconName: root.searchSubpage ? "go-previous-symbolic" : "sidebar-show-symbolic"
                label: root.searchSubpage ? "Back to Search" : "Show settings pages"
                objectName: "settingsNavigationToggle"
                onClicked: { if (root.searchSubpage) searchSettings.goBack(); else root.navigationOpen = !root.navigationOpen; }
            }
            RowLayout {
                anchors.right: parent.right; anchors.rightMargin: Theme.padding
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.spaceSmall
                IconButton {
                    visible: root.dirty
                    enabled: !root.busy
                    iconName: "edit-undo-symbolic"
                    label: "Revert changes"
                    onClicked: root.read()
                }
                ActionButton {
                    objectName: "settingsApply"
                    text: root.operation === "save" ? "Saving…" : "Apply"
                    enabled: root.dirty && !root.busy && !searchSettings.editing
                    onClicked: root.save()
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
                x: Math.round((parent.width - width) / 2)
                y: 28
                width: Math.min(600, parent.width - (root.wideLayout ? 64 : 32))
                spacing: 24
                enabled: root.ready && !root.busy
                NumberAnimation { id: pageFade; target: pageContent; property: "opacity"; from: 0.65; to: 1; duration: Theme.reducedMotion ? 0 : Theme.motion }
                Rectangle {
                    visible: root.status !== "" && root.status !== "Saved"
                    Layout.fillWidth: true
                    implicitHeight: notice.implicitHeight + Theme.padding * 2
                    color: Theme.surface; radius: Theme.radius
                    Text { id: notice; anchors.fill: parent; anchors.margins: Theme.padding; text: root.status; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.warning; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                }
                SearchSettings {
                    id: searchSettings
                    settings: root
                    visible: root.page === "Search"
                    onDetailChanged: pageScroll.contentY = 0
                    onEditingChanged: pageScroll.contentY = 0
                }
                ColumnLayout {
                    visible: root.page === "AI"
                    Layout.fillWidth: true; spacing: 28
                    Section {
                        title: "Ask from Search"
                        Group {
                            PreferenceRow { objectName: "settingsAIEnabled"; title: "AI Assistant"; subtitle: "Ask questions without leaving search"; toggleChecked: root.aiEnabled; onToggleRequested: { root.aiEnabled = !root.aiEnabled; root.dirty = true; } }
                        }
                        Caption { text: "Type ! followed by a question, then press Enter. Replies stream as they arrive. Normal searches are never sent to AI."; Layout.leftMargin: Theme.spaceSmall }
                    }
                    Section {
                        title: "CLI Provider"
                        description: "Use an installed CLI and its existing login."
                        Group {
                            Repeater {
                                model: [{id: "pi", name: "Pi"}, {id: "claude", name: "Claude Code"}]
                                ColumnLayout {
                                    required property var modelData
                                    required property int index
                                    Layout.fillWidth: true; spacing: 0
                                    ControlRow {
                                        Layout.fillWidth: true
                                        implicitHeight: 64
                                        title: modelData.name; iconName: ""
                                        subtitle: root.harnesses.some(item => item.id === modelData.id && item.path) ? "Installed" : "Not found on this device"
                                        selected: root.selectedHarness === modelData.id
                                        leftPadding: Theme.spaceSmall; rightPadding: Theme.spaceSmall
                                        Accessible.role: Accessible.RadioButton
                                        Accessible.checked: selected
                                        onClicked: { root.selectedHarness = modelData.id; root.executable = ""; root.dirty = true; }
                                    }
                                }
                            }
                        }
                    }
                    Section {
                        title: "Model"
                        Group {
                            EntryRow { label: "Model name"; placeholderText: "Use the CLI default"; text: root.model; onEdited: value => { root.model = value; root.dirty = true; } }
                        }
                    }
                    Group {
                        ControlRow { objectName: "settingsAdvanced"; title: "Advanced"; navigationRotation: root.advancedOpen ? 90 : 0; subtitle: root.advancedOpen ? "Custom executable" : "Use a custom CLI executable"; iconName: ""; navigation: true; implicitHeight: 60; onClicked: root.advancedOpen = !root.advancedOpen }
                        EntryRow { visible: root.advancedOpen; objectName: "settingsExecutable"; label: "Executable path"; placeholderText: "Detect automatically"; text: root.executable; onEdited: value => { root.executable = value; root.dirty = true; } }
                    }
                    Caption { text: "Tools and project context are disabled. Press Esc in search to cancel an answer."; Layout.leftMargin: Theme.spaceSmall }
                }
                ColumnLayout {
                    visible: root.page === "Previews"
                    Layout.fillWidth: true; spacing: 28
                    Section {
                        title: "File Previews"
                        description: "View files beside your search results."
                        Group {
                            PreferenceRow { title: "Show Previews"; subtitle: "Press Right Arrow on a selected file"; toggleChecked: root.draft.previews.enabled; onToggleRequested: root.update("previews", "enabled", !toggleChecked) }
                            PreferenceRow { title: "Prepare Previews Ahead of Time"; subtitle: "Make nearby results faster to open"; toggleChecked: root.draft.previews.prewarm; onToggleRequested: root.update("previews", "prewarm", !toggleChecked) }
                        }
                    }
                    Section {
                        title: "File Size Limit"
                        Group {
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.margins: Theme.padding
                                implicitHeight: 40
                                Text { Layout.fillWidth: true; text: "Maximum file size"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
                                IconButton { iconName: "list-remove-symbolic"; label: "Decrease preview size limit"; enabled: root.draft.previews.maxMegabytes > 1; onClicked: root.update("previews", "maxMegabytes", root.draft.previews.maxMegabytes - 1) }
                                Text { text: root.draft.previews.maxMegabytes + " MB"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; Layout.preferredWidth: 54; horizontalAlignment: Text.AlignHCenter }
                                IconButton { iconName: "list-add-symbolic"; label: "Increase preview size limit"; enabled: root.draft.previews.maxMegabytes < 20; onClicked: root.update("previews", "maxMegabytes", root.draft.previews.maxMegabytes + 1) }
                            }
                        }
                        Caption { text: "Larger files will not be previewed. The maximum is 20 MB."; Layout.leftMargin: Theme.spaceSmall }
                    }
                    Section {
                        title: "Supported Files"
                        Caption { text: "Images, video, audio, PDFs and Office documents. Markdown, HTML, code, tables, email, archives and SQLite databases." }
                        Caption { text: "Ctrl + scroll to zoom. Ctrl + 0 resets the preview to 100%." }
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
        width: 220
        anchors.top: parent.top; anchors.bottom: parent.bottom; anchors.left: parent.left
        visible: root.wideLayout || root.navigationOpen
        color: Theme.surface
        Rectangle { anchors.right: parent.right; height: parent.height; width: 1; color: Theme.barDivider }
        MouseArea {
            width: parent.width; height: 56
            onPressed: root.startSystemMove()
            onDoubleClicked: root.toggleMaximised()
        }
        Text {
            x: Theme.paddingLarge
            height: 56
            text: "Settings"
            verticalAlignment: Text.AlignVCenter
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontHeading
            font.weight: Font.DemiBold
        }
        ColumnLayout {
            y: 68
            x: Theme.padding
            width: parent.width - Theme.padding * 2
            spacing: Theme.spaceSmall
            Repeater {
                id: navItems
                model: [{name: "Search", label: "Search", icon: "system-search-symbolic"}, {name: "AI", label: "AI Assistant", icon: "chat-message-new-symbolic"}, {name: "Previews", label: "File Previews", icon: "document-open-symbolic"}, {name: "Desktop", label: "Desktop", icon: "preferences-system-symbolic"}]
                ActionButton {
                    id: navButton
                    required property int index
                    required property var modelData
                    Layout.fillWidth: true
                    implicitHeight: 44
                    text: modelData.label; iconName: modelData.icon; alignLeft: true
                    objectName: "settingsNav" + modelData.name
                    Accessible.role: Accessible.PageTab
                    Accessible.selected: root.page === modelData.name
                    background: ControlCentreButtonSurface { control: navButton; radius: Theme.insetRadius(Theme.cardRadius, Theme.padding); baseColor: root.page === navButton.modelData.name ? Theme.selection : "transparent" }
                    onClicked: { root.page = modelData.name; root.navigationOpen = false; }
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
    Rectangle {
        anchors.fill: parent
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
            width: horizontalEdge ? 6 : root.width - 12
            height: verticalEdge ? 6 : root.height - 12
            x: modelData & Qt.RightEdge ? root.width - width : horizontalEdge ? 0 : 6
            y: modelData & Qt.BottomEdge ? root.height - height : verticalEdge ? 0 : 6
            cursorShape: horizontalEdge && verticalEdge ? ((modelData === (Qt.TopEdge | Qt.LeftEdge) || modelData === (Qt.BottomEdge | Qt.RightEdge)) ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor) : horizontalEdge ? Qt.SizeHorCursor : Qt.SizeVerCursor
            onPressed: root.startSystemResize(modelData)
        }
    }

}
