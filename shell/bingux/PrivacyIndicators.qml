import QtQuick
import QtQuick.Layouts
import "DesktopLayout.js" as DesktopLayout

Item {
    id: root
    required property var systemMetrics
    required property var privacyState
    property var barWindow: null
    function appearance(label, icon) {
        return DesktopLayout.presentation(DesktopEditing.desktop, "privacy", DesktopLayout.zone(DesktopEditing.desktop.layout || {}, "privacy"), label, icon, true, false);
    }
    readonly property bool screenSharing: privacyState.screenSharing || (!privacyState.available && systemMetrics.screenSharing)
    readonly property bool active: sharingVisible || privacyState.cameraInUse
        || systemMetrics.microphoneInUse || systemMetrics.locationInUse
    visible: active
    property bool sharingVisible: screenSharing
    property double sharingShownAt: Date.now()
    implicitWidth: privacyRow.implicitWidth
    implicitHeight: Theme.barHeight
    width: implicitWidth
    height: implicitHeight
    onScreenSharingChanged: {
        sharingDelay.stop();
        if (screenSharing) {
            if (!sharingVisible) sharingShownAt = Date.now();
            sharingVisible = true;
        } else {
            sharingDelay.interval = Math.max(0, 5000 - (Date.now() - sharingShownAt));
            sharingDelay.start();
        }
    }
    Timer { id: sharingDelay; onTriggered: root.sharingVisible = false }
    RowLayout {
        id: privacyRow
        spacing: Theme.barControlGap
        ActivityIndicator {
            presentation: root.appearance(tooltip, iconName)
            objectName: "screenSharingIndicator"
            visible: root.sharingVisible
            barWindow: root.barWindow
            filled: true
            iconName: "screen-shared-symbolic"
            trailingIcon: "screencast-stop-symbolic"
            interactive: root.screenSharing && root.privacyState.available
            tooltip: root.screenSharing ? "Stop screen sharing" : "Screen sharing ended"
            onClicked: root.privacyState.stopSharing()
        }
        ActivityIndicator {
            presentation: root.appearance(tooltip, iconName)
            objectName: "cameraIndicator"
            visible: root.privacyState.cameraInUse
            barWindow: root.barWindow
            iconName: "camera-web-symbolic"
            tooltip: "Camera in use"
        }
        ActivityIndicator {
            presentation: root.appearance(tooltip, iconName)
            objectName: "microphoneIndicator"
            visible: root.systemMetrics.microphoneInUse
            barWindow: root.barWindow
            iconName: "microphone-sensitivity-high-symbolic"
            tooltip: "Microphone in use"
        }
        ActivityIndicator {
            presentation: root.appearance(tooltip, iconName)
            objectName: "locationIndicator"
            visible: root.systemMetrics.locationInUse
            barWindow: root.barWindow
            iconName: "find-location-symbolic"
            tooltip: "Location in use"
        }
    }
}
