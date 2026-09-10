import QtQuick
import QtQml.Models
import Quickshell
import Quickshell.Io

Scope {
    id: root
    property var shellObjects: null
    Component.onCompleted: ExtensionRegistry.internals = shellObjects
    onShellObjectsChanged: ExtensionRegistry.internals = shellObjects
    IpcHandler {
        target: "extensions"
        function status(): string {
            return JSON.stringify({apiVersion: ExtensionRegistry.apiVersion,
                extensions: ExtensionRegistry.extensions, widgets: ExtensionRegistry.widgets,
                actions: Object.keys(ExtensionRegistry.actions), errors: ExtensionRegistry.errors});
        }
        function reload(): void { ExtensionRegistry.reload(); }
        function enable(id: string): void { ExtensionRegistry.setEnabled(id, true); }
        function disable(id: string): void { ExtensionRegistry.setEnabled(id, false); }
        function invoke(id: string, payload: string): string {
            try { return JSON.stringify({result: ExtensionRegistry.invoke(id, payload ? JSON.parse(payload) : null)}); }
            catch (error) { return JSON.stringify({error: String(error)}); }
        }
    }
    Instantiator {
        model: ExtensionRegistry.extensions.filter(item => item.enabled && item.entrySource)
        delegate: Loader {
            id: service
            required property var modelData
            readonly property var context: ExtensionContext { extensionId: service.modelData.id }
            function load() {
                context.dispose();
                source = "";
                setSource(modelData.entrySource, {context: context});
            }
            onModelDataChanged: Qt.callLater(load)
            Component.onCompleted: Qt.callLater(load)
            onStatusChanged: if (status === Loader.Error) context.reportError("Could not load extension entry point")
        }
    }
}
