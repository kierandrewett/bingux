import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

FloatingWindow {
    id: root
    title: "Bingux Settings"
    visible: false
    implicitWidth: 960
    implicitHeight: 700
    color: Theme.barBackground
    property string page: "Search"
    property var draft: ({search: {disabledProviders: [], ai: null}, previews: {enabled: true, prewarm: true, maxMegabytes: 20}, desktop: {dock: true, sidebar: true, metrics: true}})
    property var harnesses: []
    property bool dirty: false
    property string status: ""
    property string selectedHarness: "pi"
    property string model: ""
    property string executable: ""
    property bool aiEnabled: false
    readonly property bool busy: backend.running
    function update(section, key, value) {
        const next = JSON.parse(JSON.stringify(draft));
        next[section][key] = value;
        draft = next;
        dirty = true;
        status = "";
    }
    function setProvider(key, enabled) {
        const disabled = draft.search.disabledProviders.filter(id => id !== key);
        if (!enabled) disabled.push(key);
        update("search", "disabledProviders", disabled);
    }
    function read() { backend.command = helper.concat(["read"]); backend.running = true; }
    function save() {
        update("search", "ai", aiEnabled ? {harness: selectedHarness, model: model.trim(), executable: executable.trim()} : null);
        backend.command = helper.concat(["save"]);
        backend.running = true;
    }
    readonly property var helper: Quickshell.env("BINGUX_SETTINGS_HELPER") ? [Quickshell.env("BINGUX_SETTINGS_HELPER")] : ["python3", decodeURIComponent(Qt.resolvedUrl("settings-backend.py").toString().replace(/^file:\/\//, ""))]
    onVisibleChanged: if (visible) read()
    Process {
        id: backend
        stdinEnabled: true
        onStarted: if (command[command.length - 1] === "save") { write(JSON.stringify(root.draft)); stdinEnabled = false; }
        onExited: { stdinEnabled = true; }
        stdout: SplitParser {
            onRead: line => {
                let result;
                try { result = JSON.parse(line); } catch (_) { root.status = "Could not read settings."; return; }
                if (result.error) { root.status = result.error; return; }
                root.draft = result.data;
                root.dirty = false;
                root.aiEnabled = !!result.data.search.ai;
                root.selectedHarness = result.data.search.ai?.harness || "pi";
                root.model = result.data.search.ai?.model || "";
                root.executable = result.data.search.ai?.executable || "";
                if (result.harnesses) root.harnesses = result.harnesses;
                root.status = result.warning || (backend.command[backend.command.length - 1] === "save" ? "Saved" : "");
                BinguxPreferences.data = result.data;
            }
        }
    }
    readonly property bool wideLayout: width >= 740
    property bool navigationOpen: false
    property bool advancedOpen: false
    readonly property string pageTitle: page === "AI" ? "AI Assistant" : page === "Previews" ? "File Previews" : page
    onPageChanged: { navigationOpen = false; pageScroll.contentY = 0; pageFade.restart(); }

    component Caption: Text {
        Layout.fillWidth: true
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }
    component Section: ColumnLayout {
        property string title
        property string description: ""
        Layout.fillWidth: true
        spacing: Theme.gap
        Text {
            Layout.fillWidth: true
            text: parent.title
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            font.weight: Font.DemiBold
            textFormat: Text.PlainText
        }
        Caption { text: parent.description; visible: text !== ""; Layout.bottomMargin: visible ? Theme.spaceSmall : 0 }
    }
    component Group: Rectangle {
        default property alias rows: groupRows.data
        Layout.fillWidth: true
        implicitHeight: groupRows.implicitHeight + Theme.spaceSmall * 2
        radius: Theme.radius
        color: Theme.surface
        ColumnLayout {
            id: groupRows
            x: Theme.spaceSmall; y: Theme.spaceSmall
            width: parent.width - Theme.spaceSmall * 2
            spacing: 0
        }
    }
    component Divider: Rectangle {
        Layout.fillWidth: true
        Layout.leftMargin: Theme.padding
        Layout.rightMargin: Theme.padding
        implicitHeight: 1
        color: Theme.barDivider
    }
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
    component EntryRow: ColumnLayout {
        property string label
        property alias text: entry.text
        property alias placeholderText: entry.placeholderText
        signal edited(string value)
        Layout.fillWidth: true
        Layout.margins: Theme.padding
        spacing: Theme.spaceSmall
        Caption { text: parent.label }
        TextField {
            id: entry
            Layout.fillWidth: true
            implicitHeight: 28
            padding: 0
            color: Theme.text
            placeholderTextColor: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            selectByMouse: true
            Accessible.name: parent.label
            onTextEdited: parent.edited(text)
            background: Rectangle {
                color: "transparent"
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.accent; visible: entry.activeFocus }
            }
        }
    }

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
            Text {
                anchors.centerIn: parent
                text: root.pageTitle
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontHeading
                font.weight: Font.DemiBold
                color: Theme.text
            }
            IconButton {
                anchors.left: parent.left; anchors.leftMargin: Theme.padding
                anchors.verticalCenter: parent.verticalCenter
                visible: !root.wideLayout
                iconName: "sidebar-show-symbolic"
                label: "Show settings pages"
                objectName: "settingsNavigationToggle"
                onClicked: root.navigationOpen = !root.navigationOpen
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
                    text: root.busy ? "Saving…" : "Apply"
                    enabled: root.dirty && !root.busy
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
                spacing: 28
                NumberAnimation { id: pageFade; target: pageContent; property: "opacity"; from: 0.65; to: 1; duration: Theme.reducedMotion ? 0 : Theme.motion }
                Rectangle {
                    visible: root.status !== "" && root.status !== "Saved"
                    Layout.fillWidth: true
                    implicitHeight: notice.implicitHeight + Theme.padding * 2
                    color: Theme.surface; radius: Theme.radius
                    Text { id: notice; anchors.fill: parent; anchors.margins: Theme.padding; text: root.status; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.warning; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                }
                ColumnLayout {
                    visible: root.page === "Search"
                    Layout.fillWidth: true; spacing: 28
                    Section {
                        title: "On This Device"
                        description: "Choose what appears when you search."
                        Group {
                            PreferenceRow { objectName: "settingsApplications"; title: "Applications"; subtitle: "Installed apps and desktop actions"; toggleChecked: !root.draft.search.disabledProviders.includes("applications"); onToggleRequested: root.setProvider("applications", !toggleChecked) }
                            Divider {}
                            PreferenceRow { title: "Files & Folders"; subtitle: "Find files in your indexed locations"; toggleChecked: !root.draft.search.disabledProviders.includes("files"); onToggleRequested: root.setProvider("files", !toggleChecked) }
                            Divider {}
                            PreferenceRow { title: "Calculator"; subtitle: "Calculate expressions and copy the answer"; toggleChecked: !root.draft.search.disabledProviders.includes("calculation"); onToggleRequested: root.setProvider("calculation", !toggleChecked) }
                            Divider {}
                            PreferenceRow { title: "Unit Conversions"; subtitle: "Length, weight, temperature, time and storage"; toggleChecked: !root.draft.search.disabledProviders.includes("conversions"); onToggleRequested: root.setProvider("conversions", !toggleChecked) }
                        }
                    }
                    Section {
                        title: "Online Search"
                        Group {
                            PreferenceRow { title: "Web Search"; subtitle: "Open a search in DuckDuckGo"; toggleChecked: !root.draft.search.disabledProviders.includes("web"); onToggleRequested: root.setProvider("web", !toggleChecked) }
                            Divider {}
                            PreferenceRow { title: "Website Shortcuts"; subtitle: "Wikipedia, GitHub, Maps and YouTube"; toggleChecked: !root.draft.search.disabledProviders.includes("web-shortcuts"); onToggleRequested: root.setProvider("web-shortcuts", !toggleChecked) }
                            Divider {}
                            PreferenceRow { title: "Connected Providers"; subtitle: "Include online suggestions from your providers"; toggleChecked: !root.draft.search.disabledProviders.includes("external"); onToggleRequested: root.setProvider("external", !toggleChecked) }
                        }
                        Caption { text: "Use wiki:, gh:, maps: or yt: before a website search."; Layout.leftMargin: Theme.spaceSmall }
                    }
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
                                    Divider { visible: index > 0 }
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
                        Divider { visible: root.advancedOpen }
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
                            Divider {}
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
                ColumnLayout {
                    visible: root.page === "Desktop"
                    Layout.fillWidth: true; spacing: 28
                    Section {
                        title: "Desktop Layout"
                        description: "Choose which parts of Bingux are visible."
                        Group {
                            PreferenceRow { title: "Dock"; subtitle: "Pinned and running applications"; toggleChecked: root.draft.desktop.dock; onToggleRequested: root.update("desktop", "dock", !toggleChecked) }
                            Divider {}
                            PreferenceRow { title: "Sidebar"; subtitle: "Notes, terminal, media and calendar"; toggleChecked: root.draft.desktop.sidebar; onToggleRequested: root.update("desktop", "sidebar", !toggleChecked) }
                            Divider {}
                            PreferenceRow { title: "System Monitors"; subtitle: "Performance metrics in the top bar"; toggleChecked: root.draft.desktop.metrics; onToggleRequested: root.update("desktop", "metrics", !toggleChecked) }
                        }
                        Caption { text: "Right-click the system monitors to choose which metrics appear."; Layout.leftMargin: Theme.spaceSmall }
                    }
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
