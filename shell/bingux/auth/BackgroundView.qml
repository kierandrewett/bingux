import QtQuick
import ".."

// The wallpaper layer of an authentication screen.
//
// A new `source` loads into the hidden one of two image slots and fades in
// over the other once it has decoded, so changing the account never cuts and
// never shows half a picture. When the source is empty, or fails to load, the
// view fades to a safe tint of the account's accent instead of keeping the
// previous account's picture. A dark scrim keeps the clock and the prompt
// readable on bright pictures.
//
// `backdrop` is the picture layer alone; frosted surfaces sample it.
Item {
    id: root

    property url source: ""
    // Appended to the file URL as `?rev=N` so a rewritten file at the same path
    // is decoded again instead of being served from the pixmap cache.
    property int revision: 0
    property color tintTop: "#233a5c"
    property color tintBottom: "#0b1018"
    property int fadeDuration: 320
    // The lock screen decodes each picture once and keeps nothing. The greeter
    // switches between a few accounts and benefits from the cache.
    property bool cache: false
    property bool scrim: true
    property int scrimFadeDuration: 360
    // Scale of the picture layer, driven by the host's enter and exit motion.
    property real zoom: 1

    readonly property Item backdrop: pictures
    // True while a picture, not the tint, is what the screen shows.
    readonly property bool pictureShown: slotA.opacity > 0 || slotB.opacity > 0
    readonly property url resolvedSource: source.toString() === "" ? "" : root.revision > 0 ? source + "?rev=" + root.revision : source

    // The slot that is on screen, or null when the tint is.
    property Item front: null
    // The slot that is loading the next picture.
    property Item loading: null
    property Item retiring: null
    property int stacking: 1

    component Slot: Image {
        property bool shown: false
        anchors.fill: parent
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: root.cache
        opacity: shown && status === Image.Ready ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: root.fadeDuration
            }
        }
        onStatusChanged: {
            if (root.loading !== this)
                return;
            if (status === Image.Ready)
                root.show(this);
            else if (status === Image.Error)
                root.showTint();
        }
    }

    function other(slot) {
        return slot === slotA ? slotB : slotA;
    }

    // Fade the freshly decoded slot in over the one on screen. The old slot
    // stays opaque underneath until the new one covers it, so the middle of
    // the fade never lets the tint through.
    function show(slot) {
        const previous = root.front;
        root.loading = null;
        root.front = slot;
        slot.z = root.stacking++;
        slot.shown = true;
        if (previous && previous !== slot) {
            root.retiring = previous;
            if (root.fadeDuration > 0)
                retire.restart();
            else
                previous.shown = false;
        }
    }

    function showTint() {
        root.loading = null;
        root.front = null;
        slotA.shown = false;
        slotB.shown = false;
    }

    function load() {
        const wanted = root.resolvedSource;
        if (wanted.toString() === "") {
            showTint();
            return;
        }
        if (root.front && root.front.source === wanted) {
            root.loading = null;
            return;
        }
        const slot = root.front ? other(root.front) : slotA;
        root.loading = slot;
        slot.shown = false;
        slot.source = wanted;
        if (slot.status === Image.Ready)
            show(slot);
    }

    onResolvedSourceChanged: load()
    Component.onCompleted: load()

    Timer {
        id: retire
        interval: root.fadeDuration
        onTriggered: {
            if (root.retiring && root.retiring !== root.front)
                root.retiring.shown = false;
            root.retiring = null;
        }
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop {
                position: 0
                color: root.tintTop
                Behavior on color {
                    ColorAnimation {
                        duration: root.fadeDuration
                    }
                }
            }
            GradientStop {
                position: 1
                color: root.tintBottom
                Behavior on color {
                    ColorAnimation {
                        duration: root.fadeDuration
                    }
                }
            }
        }
    }

    Item {
        id: pictures
        anchors.fill: parent
        scale: root.zoom
        Slot {
            id: slotA
        }
        Slot {
            id: slotB
        }
    }

    // Keep the clock and the prompt readable on bright wallpapers.
    Rectangle {
        anchors.fill: parent
        visible: root.pictureShown
        opacity: root.scrim ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: root.scrimFadeDuration
            }
        }
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.rgba(0, 0, 0, 0.22)
            }
            GradientStop {
                position: 0.5
                color: Qt.rgba(0, 0, 0, 0.04)
            }
            GradientStop {
                position: 1
                color: Qt.rgba(0, 0, 0, 0.42)
            }
        }
    }
}
