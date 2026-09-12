pragma Singleton
import QtQuick
import QtQml.Models
import Quickshell

Singleton {
    id: root
    readonly property int capacity: 12
    readonly property int imageSize: 512
    property var sources: []
    function retain(source) {
        const url = String(source || "");
        if (!url || sources[sources.length - 1] === url)
            return;
        sources = sources.filter(item => item !== url).concat([url]).slice(-capacity);
    }
    // Live Image references retain Qt's shared decoded-image cache across menu
    // destruction. Every consumer uses the same URL, size and cache settings.
    Instantiator {
        model: root.sources
        delegate: Image {
            required property string modelData
            source: modelData
            sourceSize.width: root.imageSize
            sourceSize.height: root.imageSize
            asynchronous: true
            fillMode: Image.PreserveAspectCrop
            cache: true
            visible: false
        }
    }
}
