import QtQuick
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
    property string filter: ""
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
    spacing: 20
    readonly property string displayTitle: editing ? (editingId ? "Edit Search Engine" : "Add Search Engine") : selectedProvider.name
    function goBack() { editing = false; detail = ""; error = ""; }
    function editEngine(engine) {
        editingId = engine?.id || "";
        engineName = engine?.name || "";
        engineShortcut = engine?.shortcut || "";
        engineUrl = engine?.url || "";
        error = "";
        editing = true;
    }
    function storeEngine() {
        const shortcut = engineShortcut.trim().toLowerCase();
        const name = engineName.trim();
        const url = engineUrl.trim().replace("%s", "{query}");
        if (!name || name.length > 80) { error = "Enter a name of up to 80 characters."; return; }
        if (!/^[a-z0-9][a-z0-9-]{0,23}$/.test(shortcut) || ["wiki", "gh", "maps", "yt"].includes(shortcut)
            || engines.some(e => e.id !== editingId && e.shortcut === shortcut)) { error = "Choose a unique shortcut, such as docs or music."; return; }
        if (!/^https?:\/\/[^\s/@]+(?:\/|\?|#)/.test(url) || url.split("{query}").length !== 2) { error = "Enter a website URL with {query} where the search terms go."; return; }
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
    ColumnLayout {
        visible: !root.editing && root.detail === ""
        Layout.fillWidth: true; spacing: 20

        FilterField { Layout.fillWidth: true; placeholderText: "Find a provider or search engine"; Accessible.name: placeholderText; onTextEdited: root.filter = text.toLowerCase() }
        SettingsHeading { visible: root.matchingProviders === 0 && root.matchingEngines === 0; title: "No matching providers"; description: "Try a different name or clear the search field." }
        RowLayout {
            Layout.fillWidth: true
            visible: root.matchingEngines > 0 || !root.filter
            SettingsHeading { title: "Search engines"; description: "Use a shortcut followed by a colon to search a website." }
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
                    ControlRow {
                        title: modelData.name; subtitle: modelData.shortcut + ":" + (modelData.id === root.settings.draft.search.defaultEngine ? " · Default" : "")
                        iconName: "web-browser-symbolic"; navigation: true
                        toggleVisible: true; toggleChecked: modelData.enabled
                        toggleEnabled: modelData.id !== root.settings.draft.search.defaultEngine
                        onToggleRequested: root.toggleEngine(modelData)
                        onClicked: root.editEngine(modelData)
                    }

                }
            }
        }
        SettingsHeading { visible: root.matchingProviders > 0; title: "Built-in providers"; description: "Local results and connected services." }
        SettingsGroup {
            visible: root.matchingProviders > 0
            Repeater {
                model: root.builtins
                ControlRow {
                    required property var modelData
                    visible: !root.filter || modelData.name.toLowerCase().includes(root.filter)
                    objectName: modelData.id === "applications" ? "settingsApplications" : "provider-" + modelData.id
                    title: modelData.name; subtitle: modelData.description; iconName: modelData.icon
                    navigation: true; toggleVisible: true
                    toggleChecked: !root.settings.draft.search.disabledProviders.includes(modelData.id)
                    onToggleRequested: root.settings.setProvider(modelData.id, !toggleChecked)
                    onClicked: root.detail = modelData.id
                }
            }
        }

    }
    ColumnLayout {
        visible: root.detail !== "" && !root.editing
        Layout.fillWidth: true; spacing: 16
        SettingsHeading { title: "Search results"; description: root.selectedProvider.description }
        SettingsGroup {
            ControlRow { title: "Show in search"; iconName: ""; toggleVisible: true; toggleChecked: !root.settings.draft.search.disabledProviders.includes(root.detail); onToggleRequested: root.settings.setProvider(root.detail, !toggleChecked); onClicked: toggleRequested() }
        }
        SettingsHeading { title: "How it works"; description: root.selectedProvider.example }
        SettingsGroup {
            visible: root.detail === "files"
            SettingsField { label: "Search locations"; placeholderText: "/home/you/Documents; /home/you/Downloads"; text: (root.settings.draft.search.fileRoots || []).join("; "); onEdited: value => root.settings.update("search", "fileRoots", value.trim() ? value.split(";").map(p => p.trim()).filter(p => p) : null) }
        }
        ActionButton { visible: root.detail === "web" || root.detail === "web-shortcuts"; text: "Manage search engines"; onClicked: root.detail = "" }
    }
    ColumnLayout {
        visible: root.editing
        Layout.fillWidth: true; spacing: 16
        SettingsHeading { title: "Website search"; description: "Add a website's search URL. Bingux inserts your search terms at {query}." }
        SettingsGroup {
            SettingsField { objectName: "engineName"; label: "Name"; placeholderText: "Wikipedia"; text: root.engineName; onEdited: value => root.engineName = value }
            SettingsField { objectName: "engineShortcut"; label: "Shortcut"; placeholderText: "encyclopedia"; text: root.engineShortcut; onEdited: value => root.engineShortcut = value }
            SettingsField { objectName: "engineUrl"; label: "URL with {query} in place of search terms"; placeholderText: "https://example.com/search?q={query}"; text: root.engineUrl; onEdited: value => root.engineUrl = value }
        }
        Text { visible: root.error !== ""; text: root.error; color: Theme.warning; Layout.fillWidth: true; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
        SettingsGroup {
            visible: root.editingId !== ""
            ControlRow {
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
                text: "Remove Engine"; iconName: "user-trash-symbolic"; flat: true
                onClicked: { root.settings.update("search", "engines", root.engines.filter(e => e.id !== root.editingId)); root.goBack(); }
            }
            Item { Layout.fillWidth: true }
            ActionButton { text: "Cancel"; flat: true; onClicked: root.editing = false }
            ActionButton { objectName: "saveSearchEngine"; text: root.editingId ? "Save engine" : "Add engine"; onClicked: root.storeEngine() }
        }
    }
}
