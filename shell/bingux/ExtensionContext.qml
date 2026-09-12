import QtQuick
import "DesktopLayout.js" as DesktopLayout

QtObject {
    id: root
    readonly property int apiVersion: 1
    property string extensionId: ""
    property string widgetId: ""
    property bool preview: false
    readonly property bool editing: preview || DesktopEditing.active
    property string container: DesktopLayout.placement(DesktopEditing.desktop, widgetId)
    property var anchorWindow: null
    property var anchorItem: null
    readonly property var theme: Theme
    // Explicit escape hatch. These objects follow shell changes, not the public API contract.
    readonly property var unstable: ExtensionRegistry.internals
    readonly property var presentation: DesktopLayout.presentation(DesktopEditing.desktop, widgetId, container, ExtensionRegistry.widget(widgetId)?.label || "", ExtensionRegistry.widget(widgetId)?.icon || "", true, true)
    signal event(string name, var payload)
    readonly property Connections eventConnection: Connections {
        target: ExtensionRegistry
        function onEvent(name, payload) {
            root.event(name, payload);
        }
    }
    property var ownedObjects: []
    function createButton(parent, properties) {
        const button = Qt.createComponent(Qt.resolvedUrl("IconButton.qml")).createObject(parent, Object.assign({
            iconName: "",
            label: "",
            barStyle: Qt.binding(() => root.container.startsWith("top-")),
            barWindow: Qt.binding(() => root.anchorWindow),
            presentation: Qt.binding(() => root.presentation)
        }, properties || {}));
        if (!button)
            throw new Error("Could not create extension button");
        return button;
    }
    function createFace(parent, properties) {
        const face = Qt.createComponent(Qt.resolvedUrl("WidgetFace.qml")).createObject(parent, Object.assign({
            presentation: Qt.binding(() => root.presentation)
        }, properties || {}));
        if (!face)
            throw new Error("Could not create extension face");
        return face;
    }
    function createPopup(content, properties) {
        if (preview)
            return null;
        const popup = Qt.createComponent(Qt.resolvedUrl("ShellPopup.qml")).createObject(root, Object.assign({
            anchorItem: Qt.binding(() => root.anchorItem),
            anchorWindow: Qt.binding(() => root.anchorWindow)
        }, properties || {}));
        if (!popup)
            throw new Error("Could not create extension popup");
        ownedObjects = ownedObjects.concat([popup]);
        if (content && !content.createObject(popup.body, {
            context: root
        })) {
            ownedObjects = ownedObjects.filter(item => item !== popup);
            popup.destroy();
            throw new Error("Could not create extension popup content");
        }
        return popup;
    }
    onEditingChanged: if (editing) {
        for (const object of ownedObjects)
            if (object && "visible" in object && "anchorWindow" in object)
                object.visible = false;
    }
    property var ownedActions: []
    function registerAction(name, callback) {
        const id = extensionId + "/" + name;
        if (typeof callback !== "function")
            throw new Error("Action needs a function");
        ExtensionRegistry.actions = Object.assign({}, ExtensionRegistry.actions, {
            [id]: callback
        });
        ownedActions = ownedActions.concat([
            {
                id,
                callback
            }
        ]);
        return id;
    }
    function invokeAction(id, payload) {
        return ExtensionRegistry.invoke(id, payload);
    }
    function publish(name, payload) {
        ExtensionRegistry.event(name, payload);
    }
    function dispose() {
        for (const object of ownedObjects)
            if (object && typeof object.destroy === "function")
                object.destroy();
        ownedObjects = [];
        const remaining = Object.assign({}, ExtensionRegistry.actions);
        for (const action of ownedActions)
            if (remaining[action.id] === action.callback)
                delete remaining[action.id];
        ownedActions = [];
        ExtensionRegistry.actions = remaining;
    }
    Component.onDestruction: dispose()
    function reportError(message) {
        ExtensionRegistry.report(widgetId || extensionId, String(message));
    }
}
