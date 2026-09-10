import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: root
    objectName: "extensionSettings"
    spacing: 16
    property string selectedId: ""
    readonly property var selected: ExtensionRegistry.extensions.find(item => item.id === selectedId)
    Text {
        Layout.fillWidth: true
        text: "Extensions can add widgets and change shell behaviour. Enable extensions you trust: they run with your session access."
        color: Theme.muted; wrapMode: Text.Wrap; font.pixelSize: Theme.fontSize
    }
    SettingsRow { Layout.fillWidth: true; title: "Reload extensions"; subtitle: "Find installed extensions and reload their components"; onClicked: ExtensionRegistry.reload() }
    Repeater {
        model: ExtensionRegistry.extensions
        ColumnLayout {
            required property var modelData
            Layout.fillWidth: true
            SettingsRow {
                Layout.fillWidth: true
                title: modelData.name
                subtitle: modelData.description || modelData.id
                valueText: modelData.enabled ? "On" : "Off"
                onClicked: ExtensionRegistry.setEnabled(modelData.id, !modelData.enabled)
            }
            SettingsRow {
                Layout.fillWidth: true
                visible: modelData.enabled && !!modelData.settingsSource
                title: "Configure " + modelData.name
                navigation: true
                onClicked: root.selectedId = root.selectedId === modelData.id ? "" : modelData.id
            }
        }
    }
    Text {
        Layout.fillWidth: true; visible: !ExtensionRegistry.extensions.length
        text: "No extensions installed. Place an extension folder in ~/.local/share/bingux/extensions, then select Reload extensions."
        wrapMode: Text.Wrap; color: Theme.muted; font.pixelSize: Theme.fontSize
    }
    Repeater {
        model: ExtensionRegistry.errors
        Text {
            required property var modelData
            Layout.fillWidth: true
            text: modelData.id + ": " + modelData.message
            wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.warning; font.pixelSize: Theme.fontSmall
        }
    }
    Loader {
        id: page
        objectName: "extensionSettingsContent"
        Layout.fillWidth: true
        readonly property var context: ExtensionContext { extensionId: root.selectedId }
        function load() {
            source = "";
            if (root.selected?.enabled && root.selected.settingsSource)
                setSource(root.selected.settingsSource, {context: context});
        }
        Connections { target: root; function onSelectedChanged() { page.load(); } }
        onStatusChanged: if (status === Loader.Error) context.reportError("Could not load settings component")
    }
}
