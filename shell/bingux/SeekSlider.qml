import QtQuick
import QtQuick.Controls

Slider {
    id: root
    property color accent: Theme.accent
    property bool muted: false
    property real warningFrom: to
    property color warningColor: "#ff737d"
    property bool trackPreviewVisible: enabled && (hovered || pressed)
    property real trackPreviewPosition: pressed ? position : normalizedPositionAt(pointer.point.position.x)
    readonly property real warningPosition: Math.max(0, Math.min(1, (warningFrom - from) / Math.max(0.001, to - from)))
    readonly property bool interacting: enabled && (hovered || pressed || visualFocus)
    readonly property real previewFraction: Math.max(0, Math.min(1, trackPreviewPosition))
    function normalizedPositionAt(x) {
        const visual = Math.max(0, Math.min(1, (x - leftPadding - handle.width / 2) / Math.max(1, availableWidth - handle.width)));
        return mirrored ? 1 - visual : visual;
    }
    hoverEnabled: true
    activeFocusOnTab: true
    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0
    implicitHeight: Theme.sliderControlHeight
    implicitWidth: 160
    HoverHandler {
        id: pointer
        enabled: root.enabled
        cursorShape: Qt.PointingHandCursor
    }
    background: Item {
        id: track
        objectName: "sliderTrack"
        x: root.leftPadding + root.handle.width / 2
        y: root.topPadding + (root.availableHeight - height) / 2
        width: Math.max(0, root.availableWidth - root.handle.width)
        height: root.interacting ? Theme.sliderActiveTrackHeight : Theme.sliderTrackHeight
        opacity: root.enabled ? 1 : 0.4
        Behavior on height {
            NumberAnimation {
                duration: Theme.reducedMotion ? 0 : Theme.motion
                easing.type: Easing.OutCubic
            }
        }
        Behavior on opacity {
            NumberAnimation {
                duration: Theme.reducedMotion ? 0 : Theme.motion
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Theme.outline
        }
        Rectangle {
            id: preview
            objectName: "sliderPreviewFill"
            x: root.mirrored ? (1 - root.previewFraction) * track.width : 0
            width: root.previewFraction * track.width
            height: track.height
            radius: height / 2
            color: Theme.muted
            opacity: root.trackPreviewVisible && !root.pressed ? 0.32 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : Theme.motion
                }
            }
        }
        Item {
            id: previewBoost
            objectName: "sliderPreviewBoostFill"
            x: (root.mirrored ? 1 - root.previewFraction : root.warningPosition) * track.width
            width: Math.max(0, root.previewFraction - root.warningPosition) * track.width
            height: track.height
            opacity: preview.opacity
            clip: true
            Rectangle {
                x: preview.x - previewBoost.x
                width: preview.width
                height: track.height
                radius: height / 2
                color: root.warningColor
            }
        }
        Rectangle {
            objectName: "sliderValueFill"
            x: root.mirrored ? (1 - root.position) * track.width : 0
            width: root.position * track.width
            height: track.height
            radius: height / 2
            color: root.muted || !root.enabled ? Theme.muted : root.interacting ? root.accent : Theme.text
            Behavior on color {
                ColorAnimation {
                    duration: Theme.reducedMotion ? 0 : Theme.motion
                }
            }
        }
        Item {
            id: boost
            objectName: "sliderBoostFill"
            x: (root.mirrored ? 1 - root.position : root.warningPosition) * track.width
            width: Math.max(0, root.position - root.warningPosition) * track.width
            height: track.height
            visible: root.enabled && !root.muted
            clip: true
            Rectangle {
                x: (root.mirrored ? (1 - root.position) * track.width : 0) - boost.x
                width: root.position * track.width
                height: track.height
                radius: height / 2
                color: root.warningColor
            }
        }
        Rectangle {
            objectName: "sliderWarningMarker"
            visible: root.warningPosition > 0 && root.warningPosition < 1
            x: (root.mirrored ? 1 - root.warningPosition : root.warningPosition) * track.width - width / 2
            anchors.verticalCenter: parent.verticalCenter
            width: 2
            height: Math.max(2, track.height - 2)
            radius: 1
            color: Theme.background
            opacity: 0.7
        }
    }
    handle: Item {
        objectName: "sliderHandle"
        // Geometry is constant. Only the visible thumb scales, so hovering
        // cannot change value mapping or move the track's endpoints.
        x: root.leftPadding + root.visualPosition * (root.availableWidth - width)
        y: root.topPadding + (root.availableHeight - height) / 2
        width: Theme.sliderHandleSize
        height: Theme.sliderHandleSize
        Rectangle {
            anchors.centerIn: parent
            width: parent.width + Theme.spaceSmall
            height: width
            radius: width / 2
            color: "transparent"
            border.width: 1
            border.color: root.accent
            opacity: root.enabled && root.visualFocus ? 1 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : Theme.motion
                }
            }
        }
        Rectangle {
            objectName: "sliderThumb"
            anchors.centerIn: parent
            width: Theme.sliderThumbSize
            height: width
            radius: width / 2
            color: Theme.text
            border.width: 1
            border.color: Theme.elevated
            opacity: root.enabled ? 1 : 0.4
            scale: root.pressed ? 1.1 : root.interacting ? 1.3 : 1
            Behavior on scale {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : Theme.motion
                    easing.type: Easing.OutCubic
                }
            }
            Behavior on opacity {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : Theme.motion
                }
            }
        }
    }
}
