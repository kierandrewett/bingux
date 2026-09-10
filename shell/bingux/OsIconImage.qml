import QtQuick
import Quickshell
import Quickshell.Widgets

// App icons and their themed fallbacks share the alpha-preserving renderer.
// Providers exposing live pixel data bypass
// the file resolver, preserving notification/tray updates and their alpha.
Item {
    id: root
    property string source: ""
    property int implicitSize: 32
    implicitWidth: implicitSize
    implicitHeight: implicitSize
    readonly property string normalizedSource: !source || source.startsWith("/") || /^[a-z][a-z0-9+.-]*:/i.test(source)
        ? source : Quickshell.iconPath(source, "application-x-executable")
    readonly property bool liveImage: normalizedSource.startsWith("data:") || (normalizedSource.startsWith("image://") && !normalizedSource.startsWith("image://icon/"))
    readonly property string resolvedSource: liveImage ? normalizedSource : OsIcons.sources[normalizedSource] || ""
    property string retainedSource: ""
    function retainSource() {
        const next = liveImage ? "" : normalizedSource;
        if (next === retainedSource) return;
        if (next) OsIcons.retain(next);
        if (retainedSource) OsIcons.release(retainedSource);
        retainedSource = next;
    }
    onNormalizedSourceChanged: retainSource()
    Component.onCompleted: retainSource()
    Component.onDestruction: if (retainedSource) OsIcons.release(retainedSource)
    IconImage {
        anchors.fill: parent
        source: root.resolvedSource
    }
}
