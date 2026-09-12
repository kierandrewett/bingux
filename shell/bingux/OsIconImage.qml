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
    property int rasterSize: 0
    property bool mipmap: false
    implicitWidth: implicitSize
    implicitHeight: implicitSize
    readonly property string normalizedSource: !source || source.startsWith("/") || /^[a-z][a-z0-9+.-]*:/i.test(source) ? source : Quickshell.iconPath(source, "application-x-executable")
    readonly property bool liveImage: normalizedSource.startsWith("data:") || (normalizedSource.startsWith("image://") && !normalizedSource.startsWith("image://icon/"))
    readonly property string requestedSource: {
        if (!normalizedSource || liveImage || rasterSize <= 0 || normalizedSource.startsWith("file-preview:"))
            return normalizedSource;
        const uri = normalizedSource.startsWith("/") ? "file://" + normalizedSource : normalizedSource;
        return uri + (uri.includes("?") ? "&" : "?") + "bingux-size=" + Math.min(1024, Math.ceil(rasterSize));
    }
    readonly property string resolvedSource: liveImage ? normalizedSource : OsIcons.sources[requestedSource] || OsIcons.sources[normalizedSource] || ""
    property string retainedSource: ""
    function retainSource() {
        const next = liveImage ? "" : requestedSource;
        if (next === retainedSource)
            return;
        if (next)
            OsIcons.retain(next);
        if (retainedSource)
            OsIcons.release(retainedSource);
        retainedSource = next;
    }
    onRequestedSourceChanged: retainSource()
    Component.onCompleted: retainSource()
    Component.onDestruction: if (retainedSource)
        OsIcons.release(retainedSource)
    IconImage {
        anchors.fill: parent
        source: root.resolvedSource
        mipmap: root.mipmap
    }
}
