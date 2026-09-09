import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts
import Quickshell

ColumnLayout {
    id: root
    required property var settings
    property string detail: ""
    property bool editing: false
    property string editingId: ""
    property string engineName: ""
    property string engineShortcut: ""
    property string engineUrl: ""
    property string error: ""
    property string errorField: ""
    function focusFilter() { providerFilter.forceActiveFocus(); providerFilter.selectAll(); }
    property string filter: ""
    property bool enginesOnly: false
    readonly property var builtins: [
        {id: "applications", name: "Applications", icon: "view-app-grid-symbolic", description: "Installed applications and their desktop actions.", example: "Search by name, description or executable."},
        {id: "files", name: "Files and folders", icon: "folder-symbolic", description: "Search your indexed locations and recent files.", example: "Leave locations blank to use the locations from your system configuration."},
        {id: "calculation", name: "Calculator", icon: "accessories-calculator-symbolic", description: "Calculate expressions directly in search.", example: "Try 24 * 7 or sqrt(144). Activate a result to copy the answer."},
        {id: "conversions", name: "Unit conversions", icon: "accessories-calculator-symbolic", description: "Convert length, weight, temperature, time and storage.", example: "Try 10 km to mi or 32 f to c."},
        {id: "web", name: "Web search", icon: "web-browser-symbolic", description: "Open searches with your default search engine.", example: "Choose the default engine in the Search engines list."},
        {id: "web-shortcuts", name: "Website shortcuts", icon: "insert-link-symbolic", description: "Search a specific website with a shortcut.", example: "Built-in shortcuts: wiki:, gh:, maps: and yt:. Your search engines also have shortcuts."},
        {id: "external", name: "Connected providers", icon: "network-workgroup-symbolic", description: "Results from providers installed with your system.", example: "Provider processes use the manifests in your system configuration. Website engines can be added below without installing a provider."}
    ]
    readonly property var selectedProvider: builtins.find(p => p.id === detail) || builtins[0]
    readonly property int matchingProviders: builtins.filter(p => !filter || p.name.toLowerCase().includes(filter)).length
    readonly property int matchingEngines: engines.filter(e => !filter || e.name.toLowerCase().includes(filter) || e.shortcut.includes(filter)).length
    readonly property var engines: settings.draft.search.engines || []
    Layout.fillWidth: true
    spacing: 24
    readonly property string displayTitle: editing ? (editingId ? "Edit search engine" : "Add search engine") : selectedProvider.name
    function goBack() { editing = false; detail = ""; error = ""; }
    function editEngine(engine) {
        editingId = engine?.id || "";
        engineName = engine?.name || "";
        engineShortcut = engine?.shortcut || "";
        engineUrl = engine?.url || "";
        error = ""; errorField = "";
        editing = true;
    }
    function storeEngine() {
        error = ""; errorField = "";
        const shortcut = engineShortcut.trim().toLowerCase();
        const name = engineName.trim();
        const url = engineUrl.trim().replace("%s", "{query}");
        if (!name || name.length > 80) { errorField = "name"; error = "Enter a name of up to 80 characters."; nameField.input.forceActiveFocus(); return; }
        if (!/^[a-z0-9][a-z0-9-]{0,23}$/.test(shortcut) || ["wiki", "gh", "maps", "yt"].includes(shortcut)
            || engines.some(e => e.id !== editingId && e.shortcut === shortcut)) { errorField = "shortcut"; error = "Choose a unique shortcut, such as docs or music."; shortcutField.input.forceActiveFocus(); return; }
        if (!/^https?:\/\/[^\s/@]+(?:\/|\?|#)/.test(url) || url.split("{query}").length !== 2) { errorField = "url"; error = "Enter a website URL with {query} where the search terms go."; urlField.input.forceActiveFocus(); return; }
        const old = engines.find(e => e.id === editingId);
        const id = old?.id || "site-" + Date.now().toString(36);
        const engine = {id, name, shortcut, url, enabled: old ? old.enabled : true};
        settings.update("search", "engines", old ? engines.map(e => e.id === id ? engine : e) : engines.concat([engine]));
        editing = false;
    }
    function toggleEngine(engine) {
        if (engine.id === settings.draft.search.defaultEngine) return;
        settings.update("search", "engines", engines.map(e => e.id === engine.id ? Object.assign({}, e, {enabled: !e.enabled}) : e));
    }
    FolderDialog {
        id: folderPicker
        title: "Choose a search folder"
        onAccepted: {
            const path = decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, ""));
            const paths = root.settings.draft.search.fileRoots || [];
            if (path && !paths.includes(path)) root.settings.update("search", "fileRoots", paths.concat([path]));
        }
    }
    ColumnLayout {
        visible: !root.editing && root.detail === ""
        Layout.fillWidth: true; spacing: 24

        FilterField { id: providerFilter; objectName: "settingsProviderFilter"; Layout.fillWidth: true; placeholderText: root.enginesOnly ? "Find a search engine" : "Find a search provider"; Accessible.name: placeholderText; onTextEdited: root.filter = text.toLowerCase() }
        SettingsHeading { visible: root.enginesOnly ? root.matchingEngines === 0 : root.matchingProviders === 0; title: "No matches"; description: "Try a different name or clear the search field." }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            visible: !root.enginesOnly && root.matchingProviders > 0
            SettingsHeading { visible: !root.enginesOnly && root.matchingProviders > 0; title: "Search results"; description: "Choose what appears when you search." }
            SettingsGroup {
                visible: !root.enginesOnly && root.matchingProviders > 0
                Repeater {
                    model: root.builtins
                    SettingsRow {
                        required property var modelData
                        visible: !root.filter || modelData.name.toLowerCase().includes(root.filter)
                        objectName: modelData.id === "applications" ? "settingsApplications" : "provider-" + modelData.id
                        title: modelData.name; iconName: modelData.icon
                        navigation: true; toggleVisible: true
                        toggleChecked: !root.settings.draft.search.disabledProviders.includes(modelData.id)
                        onToggleRequested: root.settings.setProvider(modelData.id, !toggleChecked)
                        onClicked: root.detail = modelData.id
                    }
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            visible: root.enginesOnly && (root.matchingEngines > 0 || !root.filter)
            RowLayout {
                Layout.fillWidth: true
                visible: root.enginesOnly && (root.matchingEngines > 0 || !root.filter)
                SettingsHeading { title: "Website searches"; description: "Search directly with a shortcut, such as ddg: cats." }
                ActionButton { objectName: "addSearchEngine"; text: "Add"; iconName: "list-add-symbolic"; onClicked: root.editEngine(null) }
            }
            SettingsGroup {
                visible: root.matchingEngines > 0
                Repeater {
                    model: root.engines
                    ColumnLayout {
                        required property var modelData
                        Layout.fillWidth: true; spacing: 0
                        visible: !root.filter || modelData.name.toLowerCase().includes(root.filter) || modelData.shortcut.includes(root.filter)
                        SettingsRow {
                            title: modelData.name; subtitle: modelData.shortcut + ":"
                            valueText: modelData.id === root.settings.draft.search.defaultEngine ? "Default" : ""
                            iconName: "web-browser-symbolic"; navigation: true
                            toggleVisible: modelData.id !== root.settings.draft.search.defaultEngine; toggleChecked: modelData.enabled
                            toggleEnabled: modelData.id !== root.settings.draft.search.defaultEngine
                            onToggleRequested: root.toggleEngine(modelData)
                            onClicked: root.editEngine(modelData)
                        }

                    }
                }
            }
        }

    }
    ColumnLayout {
        visible: root.detail !== "" && !root.editing
        Layout.fillWidth: true; spacing: 24
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            visible: true
            SettingsHeading { title: "Search results"; description: root.selectedProvider.description }
            SettingsGroup {
                SettingsRow { title: "Show in search"; iconName: ""; toggleVisible: true; toggleChecked: !root.settings.draft.search.disabledProviders.includes(root.detail); onToggleRequested: root.settings.setProvider(root.detail, !toggleChecked); onClicked: toggleRequested() }
            }
        }
        SettingsHeading { title: "How it works"; description: root.selectedProvider.example }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            visible: root.detail === "files"
            SettingsHeading {
                visible: root.detail === "files"
                title: "Search locations"
                description: root.settings.draft.search.fileRoots?.length ? "Only these folders will be searched." : "Using your system’s configured search locations."
            }
            SettingsGroup {
                visible: root.detail === "files" && (root.settings.draft.search.fileRoots || []).length > 0
                Repeater {
                    model: root.settings.draft.search.fileRoots || []
                    SettingsRow {
                        required property string modelData
                        title: modelData.split("/").filter(p => p).pop() || "/"
                        subtitle: modelData; iconName: "folder-symbolic"
                        rowInteractive: false; actionLabel: "Remove"
                        onActionTriggered: {
                            const paths = root.settings.draft.search.fileRoots.filter(p => p !== modelData);
                            root.settings.update("search", "fileRoots", paths.length ? paths : null);
                        }
                    }
                }
            }
        }
        ActionButton { visible: root.detail === "files"; text: "Add folder…"; iconName: "list-add-symbolic"; onClicked: folderPicker.open() }
        ActionButton { visible: root.detail === "web" || root.detail === "web-shortcuts"; text: "Manage search engines"; onClicked: { root.detail = ""; root.settings.page = "Engines"; } }
    }
    ColumnLayout {
        visible: root.editing
        Layout.fillWidth: true; spacing: 24
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            visible: true
            SettingsHeading { title: "Website search"; description: "Add a website's search URL. Bingux inserts your search terms at {query}." }
            SettingsGroup {
                SettingsField { id: nameField; errorText: root.errorField === "name" ? root.error : ""; objectName: "engineName"; label: "Name"; placeholderText: "Wikipedia"; text: root.engineName; onEdited: value => root.engineName = value }
                SettingsField { id: shortcutField; errorText: root.errorField === "shortcut" ? root.error : ""; hint: "Type this followed by a colon in search."; objectName: "engineShortcut"; label: "Shortcut"; placeholderText: "encyclopedia"; text: root.engineShortcut; onEdited: value => root.engineShortcut = value }
                SettingsField { id: urlField; errorText: root.errorField === "url" ? root.error : ""; objectName: "engineUrl"; label: "Search URL"; hint: "Replace the search terms in the URL with {query}."; placeholderText: "https://example.com/search?q={query}"; text: root.engineUrl; onEdited: value => root.engineUrl = value }
            }
        }
        SettingsGroup {
            visible: root.editingId !== ""
            SettingsRow {
                title: "Default search engine"; iconName: ""; toggleVisible: true
                subtitle: "Used when searching the web without a shortcut"
                toggleChecked: root.settings.draft.search.defaultEngine === root.editingId
                toggleEnabled: !toggleChecked
                onToggleRequested: {
                    root.settings.update("search", "engines", root.engines.map(e => e.id === root.editingId ? Object.assign({}, e, {enabled: true}) : e));
                    root.settings.update("search", "defaultEngine", root.editingId);
                }
                onClicked: if (toggleEnabled) toggleRequested()
            }
        }
        RowLayout {
            Layout.fillWidth: true
            ActionButton {
                visible: root.editingId !== "" && root.settings.draft.search.defaultEngine !== root.editingId
                text: "Remove engine"; iconName: "user-trash-symbolic"; flat: true
                onClicked: { root.settings.update("search", "engines", root.engines.filter(e => e.id !== root.editingId)); root.goBack(); }
            }
            Item { Layout.fillWidth: true }
            ActionButton { text: "Cancel"; flat: true; onClicked: root.editing = false }
            ActionButton { objectName: "saveSearchEngine"; text: root.editingId ? "Save engine" : "Add engine"; onClicked: root.storeEngine() }
        }
    }
}
