import QtQuick

Item {
    id: root
    required property string widgetId
    property bool preview: false
    property var anchorWindow: null
    readonly property var spec: ExtensionRegistry.widget(widgetId)
    readonly property var context: ExtensionContext {
        widgetId: root.widgetId
        extensionId: root.spec?.extensionId || ""
        preview: root.preview
        anchorWindow: root.anchorWindow
        anchorItem: root
    }
    implicitWidth: content.item ? content.item.implicitWidth : 32
    implicitHeight: content.item ? content.item.implicitHeight : Theme.barHeight
    enabled: !context.editing
    function load() {
        context.dispose();
        content.source = "";
        if (spec)
            content.setSource(preview ? spec.previewSource : spec.source, {
                context: context
            });
    }
    onSpecChanged: load()
    onPreviewChanged: load()
    Loader {
        id: content
        anchors.fill: parent
        onStatusChanged: if (status === Loader.Error)
            root.context.reportError("Could not load widget component")
    }
    Text {
        anchors.centerIn: parent
        visible: !root.spec || content.status === Loader.Error
        text: "!"
        color: Theme.text
        Accessible.name: "Extension widget unavailable: " + root.widgetId
    }
}
