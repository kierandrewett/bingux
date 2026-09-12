import QtQuick
import Quickshell

Item {
    id: root
    property string source: ""
    property bool active: false
    property bool firstFront: true
    readonly property var front: firstFront ? first : second
    readonly property var back: firstFront ? second : first
    readonly property int status: front.status
    onSourceChanged: load()
    onActiveChanged: if (active)
        load()
    Component.onCompleted: load()

    function load() {
        if (fade.running || front.source.toString() === source)
            return;
        AlbumArtCache.retain(source);
        back.source = source;
        ready(back);
    }
    function ready(image) {
        if (image !== back || image.source.toString() !== source || fade.running)
            return;
        if (image.status !== Image.Ready && image.status !== Image.Error && source)
            return;
        firstFront = !firstFront;
        if (active) {
            fade.restart();
        } else {
            first.opacity = firstFront ? 1 : 0;
            second.opacity = firstFront ? 0 : 1;
        }
    }
    component Cover: Image {
        anchors.fill: parent
        sourceSize.width: AlbumArtCache.imageSize
        sourceSize.height: AlbumArtCache.imageSize
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
        opacity: 0
        onStatusChanged: root.ready(this)
    }
    Cover {
        id: first
    }
    Cover {
        id: second
    }
    SymbolicIcon {
        anchors.centerIn: parent
        visible: root.status !== Image.Ready && !fade.running
        source: Quickshell.iconPath("audio-x-generic-symbolic")
    }
    ParallelAnimation {
        id: fade
        NumberAnimation {
            target: first
            property: "opacity"
            to: root.firstFront ? 1 : 0
            duration: Theme.reducedMotion ? 0 : Theme.mediaArtMotion
        }
        NumberAnimation {
            target: second
            property: "opacity"
            to: root.firstFront ? 0 : 1
            duration: Theme.reducedMotion ? 0 : Theme.mediaArtMotion
        }
        onFinished: root.load()
    }
}
